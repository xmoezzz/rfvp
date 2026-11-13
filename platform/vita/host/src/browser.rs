//! The file browser the port opens before a game: walk to a folder, pick a `.hcb` script, start.
//!
//! It draws one screen-sized texture and only repaints when something changes, so browsing an
//! idle folder costs nothing and never reads the card per frame. Input comes from the platform
//! layer (C) because no game exists yet to receive events.
//!
//! Everything the browser allocates — the layer, the rasterized glyphs, the font and the
//! texture — is released before the game boots (see [`Browser::release`]).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use rfvp::host_api::{
    BlendMode, ColorRgba, DrawSpriteCommand, PixelFormat, RfvpRenderer, RfvpResult, TextureDesc,
    TextureFilter, TextureId, TextureRect, Vertex2D,
};
use rfvp::font::Font;
use rfvp_psv::fs::PsvFileSystem;

use crate::launcher::{self, Encoding, LaunchRequest, LauncherState};
use crate::text::{Layer, Rgba, TextPainter};

pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 544;

/// The browser's texture, deliberately outside the range the engine hands out.
const TEXTURE: TextureId = TextureId(0xffff_ff00);

// SCE_CTRL_* bits (psp2/ctrl.h). Only the buttons the browser reacts to.
const CTRL_SELECT: u32 = 0x0000_0001;
const CTRL_START: u32 = 0x0000_0008;
const CTRL_UP: u32 = 0x0000_0010;
const CTRL_RIGHT: u32 = 0x0000_0020;
const CTRL_DOWN: u32 = 0x0000_0040;
const CTRL_LEFT: u32 = 0x0000_0080;
const CTRL_LTRIGGER: u32 = 0x0000_0100;
const CTRL_RTRIGGER: u32 = 0x0000_0200;
const CTRL_CIRCLE: u32 = 0x0000_2000;
const CTRL_CROSS: u32 = 0x0000_4000;
const CTRL_SQUARE: u32 = 0x0000_8000;

// Layout, in screen pixels (the browser runs at 1:1 with the 960x544 screen).
const ROW_HEIGHT: i32 = 32;
const LIST_TOP: i32 = 110;
const LIST_BOTTOM: i32 = 462;
const VISIBLE_ROWS: usize = ((LIST_BOTTOM - LIST_TOP) / ROW_HEIGHT) as usize;

const COLOR_BG: Rgba = Rgba(16, 18, 24, 255);
const COLOR_HEAD: Rgba = Rgba(26, 30, 40, 255);
const COLOR_BAND: Rgba = Rgba(34, 40, 54, 255);
const COLOR_SELECTED: Rgba = Rgba(52, 88, 152, 255);
const COLOR_TEXT: Rgba = Rgba(228, 232, 240, 255);
const COLOR_DIM: Rgba = Rgba(148, 156, 172, 255);
const COLOR_ACCENT: Rgba = Rgba(126, 202, 255, 255);
const COLOR_ERROR: Rgba = Rgba(255, 138, 130, 255);
const COLOR_WARN: Rgba = Rgba(240, 202, 112, 255);

const SIZE_TITLE: f32 = 24.0;
const SIZE_PATH: f32 = 18.0;
const SIZE_ROW: f32 = 22.0;
const SIZE_FOOT: f32 = 17.0;

/// Frames to wait before a held direction repeats, and between repeats.
const REPEAT_DELAY: u32 = 20;
const REPEAT_EVERY: u32 = 5;

/// The key reminder on the second footer line; a passing note takes its place.
const HINT_TEXT: &str =
    "X enter/start   O up   SQUARE encoding   SELECT rescan   START quit   touch: tap, tap again";

/// The platform's view of the pad and touch screen.
#[repr(C)]
#[derive(Default)]
struct RawInput {
    pressed: u32,
    released: u32,
    held: u32,
    touch_down: i32,
    touch_up: i32,
    touch_held: i32,
    touch_x: i32,
    touch_y: i32,
    stick_x: i32,
    stick_y: i32,
}

extern "C" {
    fn rfvp_psv_platform_poll_browser(out: *mut RawInput) -> i32;
}

pub struct Input {
    pub pressed: u32,
    pub held: u32,
    pub touch_down: bool,
    /// A finger is on the panel right now (not just the landing edge), so a caller can wait for
    /// the release instead of the next poll's missing edge.
    pub touch_held: bool,
    pub touch_y: i32,
    stick_y: i32,
}

impl Default for Input {
    /// Nothing pressed and the stick centred: what the browser should assume when the platform
    /// cannot be read (it then simply does nothing this frame).
    fn default() -> Self {
        Self {
            pressed: 0,
            held: 0,
            touch_down: false,
            touch_held: false,
            touch_y: 0,
            stick_y: 128,
        }
    }
}

/// Read the pad and touch screen; `None` if the platform refused.
pub fn poll_input() -> Option<Input> {
    let mut raw = RawInput::default();
    if unsafe { rfvp_psv_platform_poll_browser(&mut raw) } != 0 {
        return None;
    }
    Some(Input {
        pressed: raw.pressed,
        held: raw.held,
        touch_down: raw.touch_down != 0,
        touch_held: raw.touch_held != 0,
        touch_y: raw.touch_y,
        stick_y: raw.stick_y,
    })
}

/// What the browser wants the application to do next.
pub enum Outcome {
    Idle,
    Launch(LaunchRequest),
    Quit,
}

struct Row {
    name: String,
    path: String,
    sort_key: String,
    is_dir: bool,
    len: u64,
    /// For scripts: the extension's default encoding (`.gbk` means GBK).
    extension_encoding: Encoding,
    is_script: bool,
}

struct Message {
    text: String,
    color: Rgba,
}

impl Message {
    /// Errors are the messages drawn in [`COLOR_ERROR`]; they outrank a settings warning in the
    /// footer band. Nothing else in the browser uses that colour.
    fn is_error(&self) -> bool {
        self.color == COLOR_ERROR
    }
}

pub struct Browser {
    layer: Layer,
    painter: TextPainter,
    texture_uploaded: bool,
    dirty: bool,
    dir: String,
    rows: Vec<Row>,
    selected: usize,
    top: usize,
    encoding: Encoding,
    encoding_source: &'static str,
    state: LauncherState,
    message: Option<Message>,
    /// Something the player must know but that does not stop them: settings could not be read
    /// or written. Shown in the footer until it is resolved.
    warning: Option<String>,
    log: Vec<String>,
    frame: u32,
    repeat_key: u32,
    repeat_frame: u32,
    stick_latched: bool,
    last_touch_selected: Option<usize>,
}

impl Browser {
    /// Load a font, the remembered settings and the starting directory. An error carries a
    /// message worth writing to the log: without a font there is nothing to draw with.
    pub fn new(fs: &mut PsvFileSystem) -> Result<Self, String> {
        let (font, font_path) = load_font(fs)?;
        let state = LauncherState::load(fs);
        let mut log = Vec::new();
        log.push(format!("build={}", crate::diagnostics::BUILD));
        log.push(format!("font={font_path}"));
        if let Some(warning) = state.warning.clone() {
            log.push(format!("state: {warning}"));
        }
        let mut browser = Self {
            layer: Layer::new(WIDTH as usize, HEIGHT as usize),
            painter: TextPainter::new(font),
            texture_uploaded: false,
            dirty: true,
            dir: launcher::normalize_dir(&state.last_dir),
            rows: Vec::new(),
            selected: 0,
            top: 0,
            encoding: Encoding::Auto,
            encoding_source: "default",
            state: LauncherState::default(),
            message: None,
            warning: state.warning.clone(),
            log,
            frame: 0,
            repeat_key: 0,
            repeat_frame: 0,
            stick_latched: false,
            last_touch_selected: None,
        };
        browser.state = state;
        browser.state.warning = None;
        browser.dir = browser.choose_start_dir(fs);
        browser.list(fs);
        Ok(browser)
    }

    /// Persist the launcher's own settings and make a failure visible: a player who is told the
    /// encoding was remembered, and finds it gone after a restart, has no way to tell why.
    fn save_state(&mut self, fs: &mut PsvFileSystem) {
        if self.state.save(fs) {
            if self
                .warning
                .as_deref()
                .map_or(false, |warning| warning.starts_with("settings could not be saved"))
            {
                self.warning = None;
            }
        } else {
            self.warning = Some(String::from(
                "settings could not be saved: this choice applies to this session only",
            ));
            self.log.push(String::from("launcher state write failed"));
        }
        self.dirty = true;
    }

    /// The remembered directory if it still lists, else the usual game folder, else its parent,
    /// else the card root. A missing first directory must not leave the player nowhere.
    fn choose_start_dir(&mut self, fs: &mut PsvFileSystem) -> String {
        let remembered = self.dir.clone();
        for candidate in [
            Some(remembered.clone()),
            Some(launcher::DEFAULT_GAME_DIR.to_string()),
            Some(launcher::APP_DIR.to_string()),
            Some(String::from("ux0:data")),
            Some(String::from("ux0:")),
        ]
        .into_iter()
        .flatten()
        {
            let candidate = launcher::normalize_dir(&candidate);
            if fs.list_dir(&candidate).is_ok() {
                if candidate != remembered {
                    self.log
                        .push(format!("start dir: {remembered} unreadable, using {candidate}"));
                }
                return candidate;
            }
        }
        self.log
            .push(String::from("no readable start directory, showing the card root"));
        String::from("ux0:")
    }

    /// Read the current directory into rows: folders first, then scripts, each by name.
    fn list(&mut self, fs: &mut PsvFileSystem) {
        self.rows.clear();
        self.selected = 0;
        self.top = 0;
        let listing = match fs.list_dir(&self.dir) {
            Ok(listing) => listing,
            Err(err) => {
                self.message = Some(Message {
                    text: format!("this folder cannot be read ({err:?})"),
                    color: COLOR_ERROR,
                });
                return;
            }
        };
        for entry in &listing.entries {
            let mut name = entry.name.clone();
            let mut is_script = false;
            let mut extension_encoding = Encoding::Auto;
            if !entry.is_dir {
                match launcher::script_extension(&name) {
                    Some((_extension, encoding)) => {
                        is_script = true;
                        extension_encoding = encoding;
                    }
                    None => continue,
                }
            }
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }
            if entry.is_dir {
                name.push('/');
            }
            self.rows.push(Row {
                path: launcher::join_path(&self.dir, &entry.name),
                sort_key: name.to_lowercase(),
                name,
                is_dir: entry.is_dir,
                len: entry.len,
                extension_encoding,
                is_script,
            });
        }
        self.rows.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.sort_key.cmp(&b.sort_key))
        });
        self.message = if listing.truncated {
            Some(Message {
                text: format!(
                    "this folder has more than {} entries; only the first are shown",
                    rfvp_psv::fs::MAX_DIR_ENTRIES
                ),
                color: COLOR_WARN,
            })
        } else if self.rows.is_empty() {
            Some(Message {
                text: String::from("no folders or .hcb scripts here"),
                color: COLOR_DIM,
            })
        } else {
            None
        };
        self.sync_encoding();
        self.dirty = true;
    }

    /// Keep the visible window on the selection.
    fn ensure_visible(&mut self) {
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + VISIBLE_ROWS {
            self.top = self.selected + 1 - VISIBLE_ROWS;
        }
        let max_top = self.rows.len().saturating_sub(VISIBLE_ROWS);
        if self.top > max_top {
            self.top = max_top;
        }
    }

    fn select(&mut self, index: usize) {
        if index < self.rows.len() && index != self.selected {
            self.selected = index;
            self.ensure_visible();
            self.sync_encoding();
            self.dirty = true;
        }
    }

    fn move_by(&mut self, delta: i32) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() - 1;
        let next = self.selected as i32 + delta;
        let next = next.clamp(0, last as i32) as usize;
        self.select(next);
    }

    fn page(&mut self, delta: i32) {
        self.move_by(delta * VISIBLE_ROWS as i32);
    }

    /// The encoding shown for the highlighted script.
    fn sync_encoding(&mut self) {
        let Some(row) = self.rows.get(self.selected) else {
            self.encoding = Encoding::Auto;
            self.encoding_source = "default";
            return;
        };
        if !row.is_script {
            self.encoding = Encoding::Auto;
            self.encoding_source = "default";
            return;
        }
        match self.state.encoding_for(&row.path) {
            Some(encoding) => {
                self.encoding = encoding;
                self.encoding_source = "remembered";
            }
            None => {
                self.encoding = row.extension_encoding;
                self.encoding_source = if row.extension_encoding == Encoding::Auto {
                    "default"
                } else {
                    "extension"
                };
            }
        }
    }

    fn cycle_encoding(&mut self, fs: &mut PsvFileSystem) {
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        if !row.is_script {
            self.message = Some(Message {
                text: String::from("choose a script to set its encoding"),
                color: COLOR_DIM,
            });
            self.dirty = true;
            return;
        }
        let path = row.path.clone();
        self.encoding = self.encoding.next();
        self.encoding_source = "ui";
        self.state.set_encoding(&path, self.encoding);
        self.save_state(fs);
        self.message = Some(Message {
            text: format!(
                "encoding for this script: {} (choose {} for the config/default)",
                self.encoding.label(),
                Encoding::Auto.label()
            ),
            color: COLOR_DIM,
        });
        self.dirty = true;
    }

    fn go_up(&mut self, fs: &mut PsvFileSystem) {
        match launcher::parent_dir(&self.dir) {
            Some(parent) => {
                self.dir = parent;
                self.list(fs);
                self.state.last_dir = self.dir.clone();
                self.save_state(fs);
            }
            None => {
                self.message = Some(Message {
                    text: String::from("already at the top of this device"),
                    color: COLOR_DIM,
                });
                self.dirty = true;
            }
        }
    }

    fn launch_request(&self) -> Option<LaunchRequest> {
        let row = self.rows.get(self.selected)?;
        if !row.is_script {
            return None;
        }
        Some(LaunchRequest {
            asset_root: self.dir.clone(),
            script: row.name.trim_end_matches('/').to_string(),
            nls: self.encoding.nls(),
            encoding: self.encoding,
            encoding_source: self.encoding_source,
        })
    }

    /// Handle one frame of input. `Outcome::Launch` is returned once, for the highlighted script.
    pub fn handle(&mut self, fs: &mut PsvFileSystem, input: &Input) -> Outcome {
        self.frame = self.frame.wrapping_add(1);
        let pressed = input.pressed;

        if pressed & CTRL_START != 0 {
            self.log.push(String::from("start pressed: leaving the application"));
            return Outcome::Quit;
        }
        if pressed & CTRL_CIRCLE != 0 || pressed & CTRL_LTRIGGER != 0 {
            self.go_up(fs);
        }
        if pressed & CTRL_SELECT != 0 {
            self.list(fs);
        }
        if pressed & CTRL_SQUARE != 0 {
            self.cycle_encoding(fs);
        }
        if pressed & CTRL_LEFT != 0 {
            self.page(-1);
        }
        if pressed & CTRL_RIGHT != 0 || pressed & CTRL_RTRIGGER != 0 {
            self.page(1);
        }

        // Direction: the first press moves at once, holding repeats.
        let up = pressed & CTRL_UP != 0;
        let down = pressed & CTRL_DOWN != 0;
        if up || down {
            self.repeat_key = if up { CTRL_UP } else { CTRL_DOWN };
            self.repeat_frame = self.frame + REPEAT_DELAY;
            self.move_by(if up { -1 } else { 1 });
        } else if self.repeat_key != 0 {
            if input.held & self.repeat_key != 0 {
                if self.frame >= self.repeat_frame {
                    self.repeat_frame = self.frame + REPEAT_EVERY;
                    self.move_by(if self.repeat_key == CTRL_UP { -1 } else { 1 });
                }
            } else {
                self.repeat_key = 0;
            }
        }
        // Analog stick, one row per push so it cannot run away.
        let stick = input.stick_y;
        if stick > 176 || stick < 80 {
            if !self.stick_latched {
                self.stick_latched = true;
                self.move_by(if stick < 80 { -1 } else { 1 });
            }
        } else {
            self.stick_latched = false;
        }

        if pressed & CTRL_CROSS != 0 {
            if let Some(request) = self.launch_request() {
                self.prepare_launch(fs, &request);
                return Outcome::Launch(request);
            }
            self.enter_selected(fs);
        }

        if input.touch_down {
            if let Some(index) = self.row_at(input.touch_y) {
                let again = self.last_touch_selected == Some(index) && index == self.selected;
                if again {
                    if let Some(request) = self.launch_request() {
                        self.prepare_launch(fs, &request);
                        return Outcome::Launch(request);
                    }
                    self.enter_selected(fs);
                } else {
                    self.select(index);
                    self.last_touch_selected = Some(index);
                }
            }
        }
        Outcome::Idle
    }

    /// Enter the highlighted folder, if it is one.
    fn enter_selected(&mut self, fs: &mut PsvFileSystem) {
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        if !row.is_dir {
            return;
        }
        self.dir = launcher::normalize_dir(&row.path);
        self.list(fs);
        self.state.last_dir = self.dir.clone();
        self.save_state(fs);
    }

    /// Which row a screen `y` falls on, if any.
    fn row_at(&self, y: i32) -> Option<usize> {
        if y < LIST_TOP || y >= LIST_BOTTOM {
            return None;
        }
        let index = self.top + ((y - LIST_TOP) / ROW_HEIGHT) as usize;
        if index < self.rows.len() {
            Some(index)
        } else {
            None
        }
    }

    /// Remember the choice before the game boots: a failed start must not lose the directory or
    /// the encoding, and the log must say what was chosen even when the game then fails.
    fn prepare_launch(&mut self, fs: &mut PsvFileSystem, request: &LaunchRequest) {
        self.state.last_dir = request.asset_root.clone();
        self.save_state(fs);
        let script_path = launcher::join_path(&request.asset_root, &request.script);
        self.log.push(format!(
            "launch script={} dir={} encoding={} source={} nls={:?}",
            script_path,
            request.asset_root,
            request.encoding.label(),
            request.encoding_source,
            request.nls
        ));
        self.write_log(fs);
    }

    pub fn note(&mut self, text: &str) {
        self.log.push(text.to_string());
    }

    /// Write the launcher log. Bounded, overwritten each launch: it is a breadcrumb for one
    /// start attempt, not a growing history.
    pub fn write_log(&mut self, fs: &mut PsvFileSystem) {
        let mut text = String::new();
        for line in self.log.iter().rev().take(60).rev() {
            text.push_str(line);
            text.push('\n');
        }
        let _ = rfvp::host_api::RfvpFileSystem::write_all(fs, launcher::LOG_FILE, text.as_bytes());
    }

    /// Report why a game did not start, in the browser, and keep the browser usable.
    pub fn show_error(&mut self, text: &str) {
        self.log.push(format!("launch failed: {text}"));
        self.message = Some(Message {
            text: launcher::shorten(text, 150),
            color: COLOR_ERROR,
        });
        self.dirty = true;
    }

    pub fn show_note(&mut self, text: &str) {
        self.message = Some(Message {
            text: launcher::shorten(text, 150),
            color: COLOR_DIM,
        });
        self.dirty = true;
    }

    /// Drop everything the browser owns but the game must not carry: the texture first (the
    /// renderer keeps its own table), then the pixels, glyphs and font with the value itself.
    pub fn release<R: RfvpRenderer>(&mut self, renderer: &mut R) {
        if self.texture_uploaded {
            renderer.destroy_texture(TEXTURE);
            self.texture_uploaded = false;
        }
        self.layer.pixels = Vec::new();
        self.rows = Vec::new();
        self.log = Vec::new();
    }

    fn paint(&mut self) {
        let painter = &mut self.painter;
        let layer = &mut self.layer;
        layer.clear(COLOR_BG);

        // Header: what this screen is.
        layer.fill_rect(0, 0, WIDTH as i32, 64, COLOR_HEAD);
        let baseline = 10 + painter.ascent(SIZE_TITLE);
        painter.draw(layer, 20, baseline, "RFVP", SIZE_TITLE, COLOR_ACCENT, WIDTH as i32 - 20);
        let build_width = painter.measure(crate::diagnostics::BUILD, SIZE_PATH);
        painter.draw(
            layer,
            WIDTH as i32 - 20 - build_width,
            16 + painter.ascent(SIZE_PATH),
            crate::diagnostics::BUILD,
            SIZE_PATH,
            COLOR_DIM,
            WIDTH as i32 - 20,
        );

        // Path band.
        layer.fill_rect(0, 64, WIDTH as i32, 42, COLOR_BAND);
        let path = painter.truncate(&self.dir, SIZE_PATH, WIDTH as i32 - 40);
        painter.draw(
            layer,
            20,
            64 + 8 + painter.ascent(SIZE_PATH),
            &path,
            SIZE_PATH,
            COLOR_TEXT,
            WIDTH as i32 - 20,
        );

        // Rows.
        let mut y = LIST_TOP;
        let end = (self.top + VISIBLE_ROWS).min(self.rows.len());
        for index in self.top..end {
            let selected = index == self.selected;
            if selected {
                layer.fill_rect(8, y - 2, WIDTH as i32 - 16, ROW_HEIGHT - 4, COLOR_SELECTED);
            }
            let (name, is_dir, len, is_script) = {
                let row = &self.rows[index];
                (row.name.clone(), row.is_dir, row.len, row.is_script)
            };
            let color = if selected {
                Rgba(255, 255, 255, 255)
            } else if is_dir {
                COLOR_ACCENT
            } else if is_script {
                COLOR_TEXT
            } else {
                COLOR_DIM
            };
            let size_text = if is_dir {
                String::new()
            } else {
                launcher::human_size(len)
            };
            let size_width = if size_text.is_empty() {
                0
            } else {
                painter.measure(&size_text, SIZE_FOOT) + 16
            };
            let name_limit = WIDTH as i32 - 40 - size_width;
            let shown = painter.truncate(&name, SIZE_ROW, name_limit);
            let baseline = y + 3 + painter.ascent(SIZE_ROW);
            painter.draw(layer, 20, baseline, &shown, SIZE_ROW, color, name_limit + 20);
            if !size_text.is_empty() {
                let width = painter.measure(&size_text, SIZE_FOOT);
                painter.draw(
                    layer,
                    WIDTH as i32 - 20 - width,
                    y + 6 + painter.ascent(SIZE_FOOT),
                    &size_text,
                    SIZE_FOOT,
                    COLOR_DIM,
                    WIDTH as i32 - 20,
                );
            }
            y += ROW_HEIGHT;
        }

        // Scroll hint: which part of the list is on screen.
        if self.rows.len() > VISIBLE_ROWS {
            let total = self.rows.len();
            let bar_x = WIDTH as i32 - 6;
            let track = LIST_BOTTOM - LIST_TOP;
            let height = (track as usize * VISIBLE_ROWS / total).max(24) as i32;
            let offset = (track - height) as usize * self.top / total.saturating_sub(VISIBLE_ROWS).max(1);
            layer.fill_rect(bar_x, LIST_TOP, 4, track, COLOR_BAND);
            layer.fill_rect(bar_x, LIST_TOP + offset as i32, 4, height, COLOR_DIM);
        }

        // Footer: encoding of the highlighted script, then the keys — or the newest note where the
        // keys were, so a note never hides a settings problem.
        layer.fill_rect(0, LIST_BOTTOM, WIDTH as i32, HEIGHT as i32 - LIST_BOTTOM, COLOR_HEAD);
        let encoding_text = format!(
            "encoding: {}   folder: {}",
            self.encoding.label(),
            if self.rows.is_empty() {
                String::from("-")
            } else {
                format!(
                    "{} folders, {} scripts",
                    self.rows.iter().filter(|row| row.is_dir).count(),
                    self.rows.iter().filter(|row| row.is_script).count()
                )
            }
        );
        let mut line_y = LIST_BOTTOM + 6;
        painter.draw(
            layer,
            20,
            line_y + painter.ascent(SIZE_FOOT),
            &encoding_text,
            SIZE_FOOT,
            COLOR_DIM,
            WIDTH as i32 - 20,
        );
        line_y += 24;

        // Priority of what the player must read: an error (a game that would not start), then a
        // settings warning (a choice that was not saved), then a passing note. A warning must not
        // be pushed out by the cheerful "encoding for this script…" line that follows it — that
        // was exactly the case in which a player believes a setting was stored when it was not.
        let (band, note) = match (&self.message, &self.warning) {
            (Some(message), Some(warning)) if !message.is_error() => (
                Some((warning.clone(), COLOR_WARN)),
                Some((message.text.clone(), message.color)),
            ),
            (Some(message), _) => (Some((message.text.clone(), message.color)), None),
            (None, Some(warning)) => (Some((warning.clone(), COLOR_WARN)), None),
            (None, None) => (None, None),
        };
        let (hint_text, hint_color) = match &note {
            Some((text, color)) => (text.as_str(), *color),
            None => (HINT_TEXT, COLOR_DIM),
        };
        let hint = painter.truncate(hint_text, SIZE_FOOT, WIDTH as i32 - 40);
        painter.draw(
            layer,
            20,
            line_y + painter.ascent(SIZE_FOOT),
            &hint,
            SIZE_FOOT,
            hint_color,
            WIDTH as i32 - 20,
        );

        if let Some((text, color)) = band {
            layer.fill_rect(0, HEIGHT as i32 - 26, WIDTH as i32, 26, COLOR_BAND);
            let shown = painter.truncate(&text, SIZE_FOOT, WIDTH as i32 - 40);
            painter.draw(
                layer,
                20,
                HEIGHT as i32 - 26 + 4 + painter.ascent(SIZE_FOOT),
                &shown,
                SIZE_FOOT,
                color,
                WIDTH as i32 - 20,
            );
        }
    }

    /// Draw the browser and present it. Repainting stops as soon as nothing changes.
    pub fn draw<R: RfvpRenderer>(&mut self, renderer: &mut R) -> RfvpResult<()> {
        if self.dirty || !self.texture_uploaded {
            self.paint();
            let desc = TextureDesc {
                width: WIDTH,
                height: HEIGHT,
                format: PixelFormat::Rgba8,
                mip_count: 1,
            };
            if self.texture_uploaded {
                renderer.update_texture(
                    TEXTURE,
                    TextureRect {
                        x: 0,
                        y: 0,
                        width: WIDTH,
                        height: HEIGHT,
                    },
                    &self.layer.pixels,
                )?;
            } else {
                renderer.create_texture(TEXTURE, desc, Some(&self.layer.pixels))?;
                self.texture_uploaded = true;
            }
            self.dirty = false;
        }

        renderer.begin_frame(
            WIDTH,
            HEIGHT,
            Some(ColorRgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
        )?;
        let white = ColorRgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };
        let (w, h) = (WIDTH as f32, HEIGHT as f32);
        renderer.draw_sprite(&DrawSpriteCommand {
            texture: TEXTURE,
            vertices: [
                Vertex2D {
                    position: [0.0, h],
                    tex_coord: [0.0, 1.0],
                    color: white,
                },
                Vertex2D {
                    position: [0.0, 0.0],
                    tex_coord: [0.0, 0.0],
                    color: white,
                },
                Vertex2D {
                    position: [w, h],
                    tex_coord: [1.0, 1.0],
                    color: white,
                },
                Vertex2D {
                    position: [w, 0.0],
                    tex_coord: [1.0, 0.0],
                    color: white,
                },
            ],
            blend: BlendMode::Opaque,
            filter: TextureFilter::Nearest,
            scissor: None,
        })?;
        renderer.end_frame()?;
        renderer.present()?;
        Ok(())
    }
}

impl Outcome {
}

/// Read a font from the first candidate that parses. Loading is deliberately separate from the
/// game's own `default.ttf`: the browser must show Chinese before any game directory is known,
/// and the game still uses the font in its own folder.
fn load_font(fs: &mut PsvFileSystem) -> Result<(Font, String), String> {
    let mut tried = String::new();
    for candidate in launcher::FONT_CANDIDATES {
        if !rfvp::host_api::RfvpFileSystem::exists(fs, candidate) {
            continue;
        }
        let bytes = match crate::load_file(fs, candidate, 32 * 1024 * 1024) {
            Ok(bytes) => bytes,
            Err(err) => {
                tried.push_str(&format!("{candidate}: {err:?}; "));
                continue;
            }
        };
        match Font::from_vec(bytes) {
            Ok(font) => return Ok((font, candidate.to_string())),
            Err(_) => tried.push_str(&format!("{candidate}: not a usable font; ")),
        }
    }
    Err(if tried.is_empty() {
        format!(
            "no browser font found; put a TrueType font at {}",
            launcher::FONT_CANDIDATES[0]
        )
    } else {
        format!("no usable browser font: {tried}")
    })
}

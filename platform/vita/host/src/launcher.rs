//! What the file browser remembers between launches, and what it hands to the core.
//!
//! The launcher keeps its own small state file (`ux0:data/rfvp/launcher-state.txt`): the last
//! directory browsed and the encoding the player chose for a particular script. It never writes
//! to a game's configuration, script or data, so a game copied from another card behaves exactly
//! as before. A missing, unreadable or corrupt state file only costs the remembered settings.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use rfvp::host_api::{RfvpFile, RfvpFileSystem};
use rfvp::script::parser::Nls;
use rfvp_psv::fs::PsvFileSystem;

/// Where the game goes when nothing else is configured: the directory every earlier build read.
pub const DEFAULT_GAME_DIR: &str = "ux0:data/rfvp/game";
/// The application's own data directory: the launcher's state, log and font live here rather
/// than in a game's folder.
pub const APP_DIR: &str = "ux0:data/rfvp";
pub const STATE_FILE: &str = "ux0:data/rfvp/launcher-state.txt";
pub const LOG_FILE: &str = "ux0:data/rfvp/launcher.log";
/// Fonts the browser tries, in order. The packaged build ships none: the player's own font is
/// used, so nothing is redistributed and the package does not grow by megabytes.
pub const FONT_CANDIDATES: [&str; 4] = [
    "ux0:data/rfvp/browser.ttf",
    "ux0:data/rfvp/default.ttf",
    "ux0:data/rfvp/game/default.ttf",
    "ux0:data/rfvp/launcher.ttf",
];

/// The encoding shown for one script.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// Follow the game's configuration, then the default (ShiftJIS).
    Auto,
    ShiftJis,
    Gbk,
    Utf8,
}

impl Encoding {
    pub fn label(self) -> &'static str {
        match self {
            Encoding::Auto => "config/default",
            Encoding::ShiftJis => "ShiftJIS",
            Encoding::Gbk => "GBK",
            Encoding::Utf8 => "UTF-8",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Encoding::Auto => Encoding::ShiftJis,
            Encoding::ShiftJis => Encoding::Gbk,
            Encoding::Gbk => Encoding::Utf8,
            Encoding::Utf8 => Encoding::Auto,
        }
    }

    /// What the core is told. `Auto` passes nothing, leaving the game configuration in charge.
    pub fn nls(self) -> Option<Nls> {
        match self {
            Encoding::Auto => None,
            Encoding::ShiftJis => Some(Nls::ShiftJIS),
            Encoding::Gbk => Some(Nls::GBK),
            Encoding::Utf8 => Some(Nls::UTF8),
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(Encoding::Auto),
            "sjis" => Some(Encoding::ShiftJis),
            "gbk" => Some(Encoding::Gbk),
            "utf8" => Some(Encoding::Utf8),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Encoding::Auto => "auto",
            Encoding::ShiftJis => "sjis",
            Encoding::Gbk => "gbk",
            Encoding::Utf8 => "utf8",
        }
    }
}

/// A script the player chose: everything the core needs to start that game.
pub struct LaunchRequest {
    /// The game's resource root, as an absolute path the platform layer can prefix itself with.
    pub asset_root: String,
    /// The chosen script, relative to `asset_root`.
    pub script: String,
    pub nls: Option<Nls>,
    pub encoding: Encoding,
    /// Where the encoding came from, for the log: `ui`, `remembered`, `extension`, `default`.
    pub encoding_source: &'static str,
}

pub struct LauncherState {
    pub last_dir: String,
    /// Script path (absolute) and the encoding chosen for it.
    pub encodings: Vec<(String, Encoding)>,
    /// A fault worth telling the player about (state file unreadable, settings not saved).
    pub warning: Option<String>,
}

impl Default for LauncherState {
    fn default() -> Self {
        Self {
            last_dir: String::from(DEFAULT_GAME_DIR),
            encodings: Vec::new(),
            warning: None,
        }
    }
}

fn read_text(fs: &mut PsvFileSystem, path: &str) -> Option<String> {
    let mut file = RfvpFileSystem::open(fs, path).ok()?;
    let bytes = file.read_to_vec(256 * 1024).ok()?;
    core::str::from_utf8(&bytes).ok().map(String::from)
}

impl LauncherState {
    pub fn load(fs: &mut PsvFileSystem) -> Self {
        let mut state = Self::default();
        if !RfvpFileSystem::exists(fs, STATE_FILE) {
            return state;
        }
        let Some(text) = read_text(fs, STATE_FILE) else {
            state.warning = Some(String::from("settings file unreadable; using defaults"));
            return state;
        };
        let mut version_ok = false;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(value) = line.strip_prefix("version=") {
                version_ok = value.trim() == "1";
                continue;
            }
            if let Some(value) = line.strip_prefix("dir=") {
                let dir = value.trim();
                if dir.starts_with("ux0:") || dir.starts_with("app0:") || dir.starts_with("uma0:") {
                    state.last_dir = normalize_dir(dir);
                }
                continue;
            }
            if let Some(value) = line.strip_prefix("enc=") {
                // `enc=<code>|<absolute script path>`; a path may itself contain '|', so only the
                // first separator splits.
                if let Some((code, path)) = value.split_once('|') {
                    if let Some(encoding) = Encoding::parse(code.trim()) {
                        if encoding != Encoding::Auto {
                            state.encodings.push((path.trim().to_string(), encoding));
                        }
                    }
                }
            }
        }
        if !version_ok && (!state.encodings.is_empty() || state.last_dir != DEFAULT_GAME_DIR) {
            // Something of ours was read even though the version line was not recognised: keep
            // the settings but say so, instead of quietly behaving as if nothing was stored.
            state.warning = Some(String::from("settings file has an unknown version"));
        }
        state
    }

    pub fn encoding_for(&self, script_path: &str) -> Option<Encoding> {
        self.encodings
            .iter()
            .find(|(path, _)| path == script_path)
            .map(|(_, encoding)| *encoding)
    }

    pub fn set_encoding(&mut self, script_path: &str, encoding: Encoding) {
        if encoding == Encoding::Auto {
            self.encodings.retain(|(path, _)| path != script_path);
            return;
        }
        match self.encodings.iter_mut().find(|(path, _)| path == script_path) {
            Some(entry) => entry.1 = encoding,
            None => {
                // Bounded: the file is a convenience, not a database.
                if self.encodings.len() >= 64 {
                    self.encodings.remove(0);
                }
                self.encodings
                    .push((script_path.to_string(), encoding));
            }
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("version=1\n# rfvp PSV launcher settings; safe to delete\n");
        out.push_str(&format!("dir={}\n", self.last_dir));
        for (path, encoding) in &self.encodings {
            out.push_str(&format!("enc={}|{}\n", encoding.code(), path));
        }
        out
    }

    /// Write the state and report whether it reached the card. A failure only costs the
    /// remembered settings; the caller must show it, not swallow it, or the player believes the
    /// encoding was remembered when it was not.
    pub fn save(&mut self, fs: &mut PsvFileSystem) -> bool {
        let text = self.to_text();
        RfvpFileSystem::write_all(fs, STATE_FILE, text.as_bytes()).is_ok()
    }
}

/// Path rules live in the library so they can be unit-tested; the launcher re-exports them for
/// its own callers and for the browser.
pub use rfvp_psv::paths::{join_path, normalize_dir, parent_dir};

/// Whether a file name is a script the browser may start, and what encoding it implies.
///
/// `.hcb` is the game's own script. `.gbk` is what earlier builds of this port accepted for
/// Chinese fan translations (`MemoriaGBK.gbk` renamed a script); keeping it listed means those
/// users keep their entry point instead of having to rename files.
pub fn script_extension(name: &str) -> Option<(&'static str, Encoding)> {
    let (_, extension) = name.rsplit_once('.')?;
    if extension.eq_ignore_ascii_case("hcb") {
        return Some(("hcb", Encoding::Auto));
    }
    if extension.eq_ignore_ascii_case("gbk") {
        return Some(("gbk", Encoding::Gbk));
    }
    None
}

/// A file size for the list, short enough not to crowd out the name.
pub fn human_size(len: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if len >= GIB {
        format!("{}.{:01} GB", len / GIB, (len % GIB) * 10 / GIB)
    } else if len >= MIB {
        format!("{}.{:01} MB", len / MIB, (len % MIB) * 10 / MIB)
    } else if len >= KIB {
        format!("{} KB", len / KIB)
    } else {
        format!("{len} B")
    }
}

/// One line of text from a possibly multi-line error, bounded so the footer stays one line.
pub fn shorten(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            if !out.ends_with(' ') {
                out.push(' ');
            }
            continue;
        }
        if count >= max_chars {
            out.push('\u{2026}');
            break;
        }
        out.push(ch);
        count += 1;
    }
    out.trim().to_string()
}

use std::{fs, os::unix::fs::MetadataExt};

use crate::{
    subsystem::resources::color_manager::ColorItem,
    utils::file::app_base_path,
};
use anyhow::{anyhow, bail, Result};
use atomic_refcell::AtomicRefCell;
use serde::{Deserialize, Serialize};

// ＭＳ ゴシック
pub const FONTFACE_MS_GOTHIC: i32 = -4;
// ＭＳ 明朝
pub const FONTFACE_MS_MINCHO: i32 = -3;
// ＭＳ Ｐゴシック
pub const FONTFACE_MS_PGOTHIC: i32 = -2;
// ＭＳ Ｐ明朝
pub const FONTFACE_MS_PMINCHO: i32 = -1;

#[derive(Debug, Clone)]
enum FontItem {
    Font(char),
    RubyFont(Vec<char>, Vec<char>),
}

fn fontitem_char_count(it: &FontItem) -> usize {
    match it {
        FontItem::Font(_) => 1,
        // Reveal budget follows base text length; ruby is rendered only when the
        // corresponding base run becomes visible.
        FontItem::RubyFont(_ruby, base) => base.len(),
    }
}

pub struct FontEnumerator {
    // Default fallback font used when id == 0 or when a requested font is missing.
    default_font: AtomicRefCell<fontdue::Font>,

    // Built-in/system fontfaces indexed by negative ids (-4..-1).
    sys_ms_gothic: AtomicRefCell<fontdue::Font>,
    sys_ms_mincho: AtomicRefCell<fontdue::Font>,
    sys_ms_pgothic: AtomicRefCell<fontdue::Font>,
    sys_ms_pmincho: AtomicRefCell<fontdue::Font>,

    // User-loaded fonts list, 1-based id: 1..=fonts.len()
    fonts: Vec<(String, AtomicRefCell<fontdue::Font>)>,

    system_fontface_id: i32,
    current_font_name: String,
}

impl Default for FontEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl FontEnumerator {
    pub fn new() -> Self {
        // System fontfaces (special negative ids).
        // These font files are shipped with the project under ./fonts/.
        let msgothic_b = include_bytes!("./fonts/MSGOTHIC.TTF") as &[u8];
        let msmincho_b = include_bytes!("./fonts/MSMINCHO.TTF") as &[u8];
        let mspgothic_b = include_bytes!("./fonts/MS-PGothic.ttf") as &[u8];
        let mspmincho_b = include_bytes!("./fonts/MS-PMincho-2.ttf") as &[u8];

        let msgothic = fontdue::Font::from_bytes(msgothic_b, fontdue::FontSettings::default())
            .expect("MSGOTHIC.TTF must be valid");
        let msmincho = fontdue::Font::from_bytes(msmincho_b, fontdue::FontSettings::default())
            .unwrap_or_else(|_| msgothic.clone());
        let mspgothic = fontdue::Font::from_bytes(mspgothic_b, fontdue::FontSettings::default())
            .unwrap_or_else(|_| msgothic.clone());
        let mspmincho = fontdue::Font::from_bytes(mspmincho_b, fontdue::FontSettings::default())
            .unwrap_or_else(|_| msgothic.clone());

        Self {
            default_font: AtomicRefCell::new(msgothic.clone()),
            sys_ms_gothic: AtomicRefCell::new(msgothic),
            sys_ms_mincho: AtomicRefCell::new(msmincho),
            sys_ms_pgothic: AtomicRefCell::new(mspgothic),
            sys_ms_pmincho: AtomicRefCell::new(mspmincho),
            fonts: vec![],
            system_fontface_id: 0,
            current_font_name: String::new(),
        }
    }

    pub fn init_fontface(&mut self) -> Result<()> {
        // if font is already loaded, just set system_fontface_id
        if self.system_fontface_id != 0 {
            return Ok(());
        }

        let base = app_base_path();
        let path = base.join("font");
        if !path.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(path.get_path())? {
            let entry = entry?;
            let md = entry.metadata()?;
            if !md.is_file() {
                continue;
            }
            // ignore 0-byte files
            if md.size() == 0 {
                continue;
            }

            let p = entry.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            if ext != "ttf" && ext != "otf" && ext != "ttc" {
                continue;
            }

            let buf = fs::read(path.get_path())?;
            let font = match fontdue::Font::from_bytes(buf, fontdue::FontSettings::default()) {
                Ok(f) => f,
                Err(e) => {
                    log::error!("Warning: failed to load font {:?}: {}", p, e);
                    continue;
                }
            };
            let name = p.to_string_lossy().to_string();
            self.fonts.push((name, AtomicRefCell::new(font)));
        }

        // default: MSGOTHIC
        self.system_fontface_id = FONTFACE_MS_GOTHIC;
        Ok(())
    }

    pub fn set_current_font_name(&mut self, name: &str) {
        self.current_font_name = name.into();
    }

    pub fn get_current_font_name(&self) -> &str {
        &self.current_font_name
    }

    /// Font id semantics:
    /// - id < 0: built-in/system fontfaces (special ids, NOT a bug)
    /// - id == 0: default fallback font
    /// - id > 0: user-loaded fonts (1-based)
    pub fn get_font(&self, id: i32) -> AtomicRefCell<fontdue::Font> {
        match id {
            FONTFACE_MS_GOTHIC => self.sys_ms_gothic.clone(),
            FONTFACE_MS_MINCHO => self.sys_ms_mincho.clone(),
            FONTFACE_MS_PGOTHIC => self.sys_ms_pgothic.clone(),
            FONTFACE_MS_PMINCHO => self.sys_ms_pmincho.clone(),
            0 => self.default_font.clone(),
            _ if id > 0 => {
                let idx = (id - 1) as usize;
                if idx >= self.fonts.len() {
                    self.default_font.clone()
                } else {
                    self.fonts[idx].1.clone()
                }
            }
            _ => self.default_font.clone(),
        }
    }

    pub fn get_font_name(&self, id: i32) -> Option<String> {
        match id {
            FONTFACE_MS_GOTHIC => Some("MS Gothic".to_string()),
            FONTFACE_MS_MINCHO => Some("MS Mincho".to_string()),
            FONTFACE_MS_PGOTHIC => Some("MS PGothic".to_string()),
            FONTFACE_MS_PMINCHO => Some("MS PMincho".to_string()),
            0 => None,
            _ if id > 0 => {
                let idx = (id - 1) as usize;
                if idx >= self.fonts.len() {
                    None
                } else {
                    Some(self.fonts[idx].0.clone())
                }
            }
            _ => None,
        }
    }

    pub fn get_font_count(&self) -> i32 {
        self.fonts.len() as i32
    }

    pub fn get_system_fontface_id(&self) -> i32 {
        self.system_fontface_id
    }

    pub fn set_system_fontface_id(&mut self, id: i32) {
        self.system_fontface_id = id;
    }
}

#[derive(Debug, Clone)]
pub struct TextItem {
    offset_x: u16,
    offset_y: u16,
    suspend_chrs: Vec<char>,

    text_content: String,
    content_text: String,
    content_items: Vec<FontItem>,

    font_name_id: i32,
    font_text_id: i32,
    main_text_size: u8,
    ruby_text_size: u8,
    main_text_outline: u8,
    ruby_text_outline: u8,
    distance: u8,
    color1: ColorItem,
    color2: ColorItem,
    color3: ColorItem,
    func1: u8,
    func2: u8,
    func3: u8,
    space_vertical: i16,
    space_horizon: i16,
    text_start_horizon: u16,
    text_start_vertical: u16,
    // IDA: ruby_horizon (TextFormat arg5) and ruby_vertical (arg6) are signed in [-16,16]
    ruby_vertical: i16,
    ruby_horizon: i16,

    // Reverse-engineered: wrapping margin used for non-suspend characters.
    // Current mapping is best-effort; only this mapping should change later, not layout logic.
    suspend_margin: i16,

    skip_mode: u8,
    is_suspended: bool,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    // IDA: speed is signed and allows -1
    speed: i32,
    loaded: bool,
    pixel_buffer: Vec<u8>,
    dirty: bool,
    elapsed: u32,

    // reveal-by-time state (best-effort baseline)
    total_chars: usize,
    visible_chars: usize,
}

impl TextItem {
    pub fn new() -> Self {
        Self {
            offset_x: 0,
            offset_y: 0,
            suspend_chrs: vec![],
            text_content: String::new(),
            content_text: String::new(),
            content_items: vec![],
            font_name_id: 0,
            font_text_id: 0,
            main_text_size: 0,
            ruby_text_size: 0,
            main_text_outline: 0,
            ruby_text_outline: 0,
            distance: 0,
            color1: ColorItem::new(),
            color2: ColorItem::new(),
            color3: ColorItem::new(),
            func1: 0,
            func2: 0,
            func3: 0,
            space_vertical: 0,
            space_horizon: 0,
            text_start_horizon: 0,
            text_start_vertical: 0,
            ruby_vertical: 0,
            ruby_horizon: 0,
            suspend_margin: 0,
            skip_mode: 0,
            is_suspended: false,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            speed: 0,
            loaded: false,
            pixel_buffer: vec![],
            dirty: false,
            elapsed: 0,
            total_chars: 0,
            visible_chars: 0,
        }
    }

    pub fn get_loaded(&self) -> bool {
        self.loaded
    }

    pub fn get_dirty(&self) -> bool {
        self.dirty
    }

    pub fn get_suspend(&self) -> bool {
        self.is_suspended
    }

    pub fn set_suspend(&mut self, suspend: bool) {
        self.is_suspended = suspend;
    }

    pub fn set_w(&mut self, w: u16) {
        self.w = w;
    }

    pub fn set_h(&mut self, h: u16) {
        self.h = h;
    }

    pub fn set_color1(&mut self, color: &ColorItem) {
        self.color1 = color.clone();
        self.dirty = true;
    }

    pub fn set_color2(&mut self, color: &ColorItem) {
        self.color2 = color.clone();
        self.dirty = true;
    }

    pub fn set_color3(&mut self, color: &ColorItem) {
        self.color3 = color.clone();
        self.dirty = true;
    }

    pub fn set_font_name(&mut self, id: i32) {
        self.font_name_id = id;
        self.dirty = true;
    }

    pub fn set_font_text(&mut self, id: i32) {
        self.font_text_id = id;
        self.dirty = true;
    }

    pub fn set_horizon_space(&mut self, space: i16) {
        self.space_horizon = space;
        self.dirty = true;
    }

    pub fn set_vertical_space(&mut self, space: i16) {
        self.space_vertical = space;
        self.dirty = true;
    }

    pub fn set_text_skip(&mut self, skip: u8) {
        self.skip_mode = skip;
    }

    pub fn set_main_text_size(&mut self, size: u8) {
        self.main_text_size = size;
        self.dirty = true;
    }

    pub fn set_ruby_text_size(&mut self, size: u8) {
        self.ruby_text_size = size;
        self.dirty = true;
    }

    pub fn set_main_text_outline(&mut self, outline: u8) {
        self.main_text_outline = outline;
        self.dirty = true;
    }

    pub fn set_ruby_text_outline(&mut self, outline: u8) {
        self.ruby_text_outline = outline;
        self.dirty = true;
    }

    pub fn set_shadow_dist(&mut self, dist: u8) {
        self.distance = dist;
        self.dirty = true;
    }

    pub fn set_function1(&mut self, func: u8) {
        self.func1 = func;
    }

    pub fn set_function2(&mut self, func: u8) {
        self.func2 = func;
    }

    pub fn set_function3(&mut self, func: u8) {
        self.func3 = func;
    }

    pub fn set_text_pos_x(&mut self, x: u16) {
        self.x = x;
    }

    pub fn set_text_pos_y(&mut self, y: u16) {
        self.y = y;
    }

    pub fn set_text_suspend_chr(&mut self, chrs: &str) {
        self.suspend_chrs = chrs.chars().collect();
        self.dirty = true;
    }

    pub fn set_speed(&mut self, speed: i32) {
        self.speed = speed;
        self.elapsed = 0;
        // Recompute visible budget on next tick; force refresh.
        self.visible_chars = 0;
        self.dirty = true;
    }

    fn ensure_buffer(&mut self) {
        let w = self.w as usize;
        let h = self.h as usize;
        let expected = w.saturating_mul(h).saturating_mul(4);
        if expected == 0 {
            self.pixel_buffer.clear();
            self.loaded = false;
            return;
        }
        if self.pixel_buffer.len() != expected {
            self.pixel_buffer.resize(expected, 0);
        }
        self.loaded = true;
    }

    fn clear_buffer(&mut self) {
        if !self.pixel_buffer.is_empty() {
            self.pixel_buffer.fill(0);
        }
    }

    fn is_suspend_chr(&self, ch: char) -> bool {
        self.suspend_chrs.contains(&ch)
    }

    fn wrap_limit_px(&self, ch: char) -> i32 {
        let w = self.w as i32;
        if w <= 0 {
            return 0;
        }
        if self.is_suspend_chr(ch) {
            w
        } else {
            (w - self.suspend_margin as i32).max(0)
        }
    }

    fn parse_content_text(&mut self, content_text: &str) {
        let content_chrs = content_text.chars().collect::<Vec<_>>();

        let mut items: Vec<FontItem> = vec![];
        let mut i = 0;

        // aaa[bbb|ccc]
        // bbb : ruby
        // ccc : base
        while i < content_chrs.len() {
            let chr = content_chrs[i];
            if chr == '[' {
                // try parse ruby pattern, otherwise treat as literal '['
                let mut parsed = false;
                let mut j = i + 1;
                while j < content_chrs.len() {
                    if content_chrs[j] == '|' {
                        let mut k = j + 1;
                        while k < content_chrs.len() {
                            if content_chrs[k] == ']' {
                                items.push(FontItem::RubyFont(
                                    content_chrs[i + 1..j].to_vec(),
                                    content_chrs[j + 1..k].to_vec(),
                                ));
                                i = k + 1;
                                parsed = true;
                                break;
                            }
                            k += 1;
                        }
                        break;
                    }
                    j += 1;
                }
                if !parsed {
                    items.push(FontItem::Font('['));
                    i += 1;
                }
            } else {
                items.push(FontItem::Font(chr));
                i += 1;
            }
        }

        self.text_content = content_text.to_string();
        self.content_text = content_text.to_string();
        self.content_items = items;

        self.total_chars = self.content_items.iter().map(fontitem_char_count).sum();
        // When speed <= 0 (including -1) or skip_mode != 0, show all immediately.
        if self.speed <= 0 || self.skip_mode != 0 {
            self.visible_chars = self.total_chars;
        } else {
            self.visible_chars = 0;
        }
        self.dirty = true;
    }

    fn tick_reveal(&mut self, delta_ms: u32) {
        if !self.loaded {
            return;
        }

        // TextPause freezes reveal-by-time. We still allow immediate modes
        // (speed <= 0) and skip-mode to force full visibility.
        if self.is_suspended && !(self.speed <= 0 || self.skip_mode != 0) {
            return;
        }
        if self.speed <= 0 || self.skip_mode != 0 {
            // Immediate.
            if self.visible_chars != self.total_chars {
                self.visible_chars = self.total_chars;
                self.dirty = true;
            }
            return;
        }

        // speed: milliseconds per visible character (baseline assumption)
        let speed_ms = self.speed as u32;
        if speed_ms == 0 {
            if self.visible_chars != self.total_chars {
                self.visible_chars = self.total_chars;
                self.dirty = true;
            }
            return;
        }

        self.elapsed = self.elapsed.saturating_add(delta_ms);
        let new_vis = (self.elapsed / speed_ms) as usize;
        let new_vis = new_vis.min(self.total_chars);
        if new_vis != self.visible_chars {
            self.visible_chars = new_vis;
            self.dirty = true;
        }
    }

    fn put_pixel_blend(buf: &mut [u8], w: u32, h: u32, x: i32, y: i32, rgba: (u8, u8, u8, u8)) {
        if x < 0 || y < 0 {
            return;
        }
        let x = x as u32;
        let y = y as u32;
        if x >= w || y >= h {
            return;
        }
        let idx = ((y * w + x) * 4) as usize;
        let (sr, sg, sb, sa) = rgba;
        if sa == 0 {
            return;
        }
        let dr = buf[idx] as u16;
        let dg = buf[idx + 1] as u16;
        let db = buf[idx + 2] as u16;
        let da = buf[idx + 3] as u16;

        let sa_u = sa as u16;
        let inv = 255u16.saturating_sub(sa_u);

        let out_r = (sr as u16 * sa_u + dr * inv) / 255;
        let out_g = (sg as u16 * sa_u + dg * inv) / 255;
        let out_b = (sb as u16 * sa_u + db * inv) / 255;
        let out_a = (sa_u + (da * inv) / 255).min(255);

        buf[idx] = out_r as u8;
        buf[idx + 1] = out_g as u8;
        buf[idx + 2] = out_b as u8;
        buf[idx + 3] = out_a as u8;
    }

    fn draw_glyph_mask(
        buf: &mut [u8],
        bw: u32,
        bh: u32,
        x0: i32,
        y0: i32,
        mask: &[u8],
        mw: usize,
        mh: usize,
        color: &ColorItem,
    ) {
        let cr = color.get_r();
        let cg = color.get_g();
        let cb = color.get_b();
        let ca = color.get_a();

        for my in 0..mh {
            for mx in 0..mw {
                let cov = mask[my * mw + mx];
                if cov == 0 {
                    continue;
                }
                let a = ((ca as u16 * cov as u16) / 255) as u8;
                Self::put_pixel_blend(buf, bw, bh, x0 + mx as i32, y0 + my as i32, (cr, cg, cb, a));
            }
        }
    }

    fn draw_char(
        &self,
        buf: &mut [u8],
        bw: u32,
        bh: u32,
        font: &fontdue::Font,
        size: f32,
        x: i32,
        y: i32,
        ch: char,
        color: &ColorItem,
        outline: u8,
        outline_color: &ColorItem,
        shadow_dist: u8,
        shadow_color: &ColorItem,
    ) -> i32 {
        let (metrics, bitmap) = font.rasterize(ch, size);
        let gx = x + metrics.xmin;
        let gy = y + metrics.ymin;

        // shadow
        if shadow_dist != 0 {
            let d = shadow_dist as i32;
            Self::draw_glyph_mask(buf, bw, bh, gx + d, gy + d, &bitmap, metrics.width, metrics.height, shadow_color);
        }

        // outline (naive)
        if outline != 0 {
            let r = outline as i32;
            for oy in -r..=r {
                for ox in -r..=r {
                    if ox == 0 && oy == 0 {
                        continue;
                    }
                    Self::draw_glyph_mask(buf, bw, bh, gx + ox, gy + oy, &bitmap, metrics.width, metrics.height, outline_color);
                }
            }
        }

        // fill
        Self::draw_glyph_mask(buf, bw, bh, gx, gy, &bitmap, metrics.width, metrics.height, color);

        metrics.advance_width.ceil() as i32
    }

    fn rasterize_full(&mut self, fonts: &FontEnumerator) -> Result<()> {
        self.ensure_buffer();
        if !self.loaded {
            return Ok(());
        }
        self.clear_buffer();

        // font selection: prefer font_text_id, fallback to default
        let font_cell = fonts.get_font(self.font_text_id);
        let font_ref = font_cell.borrow();

        let bw = self.w as u32;
        let bh = self.h as u32;

        let main_size = if self.main_text_size == 0 { 16.0 } else { self.main_text_size as f32 };
        let ruby_size = if self.ruby_text_size == 0 { (main_size * 0.6).max(8.0) } else { self.ruby_text_size as f32 };

        let line_h = main_size.ceil() as i32 + self.space_vertical as i32;

        let mut pen_x = self.text_start_horizon as i32;
        let mut pen_y = 0i32;
        let baseline_y = self.text_start_vertical as i32;

        // Reveal budget: count in "characters" (newline counts as one; ruby counts by base text length).
        let mut remaining = self.visible_chars;
        let mut pixel_buffer = self.pixel_buffer.clone();

        for item in self.content_items.clone() {
            if remaining == 0 {
                break;
            }
            match item {
                FontItem::Font(ch) => {
                    // newline participates in reveal.
                    remaining = remaining.saturating_sub(1);
                    if ch == '\n' {
                        pen_x = self.text_start_horizon as i32;
                        pen_y += line_h;
                        continue;
                    }

                    let limit = self.wrap_limit_px(ch);
                    if limit > 0 && pen_x >= limit {
                        pen_x = self.text_start_horizon as i32;
                        pen_y += line_h;
                    }

                    let adv = self.draw_char(
                        &mut pixel_buffer,
                        bw,
                        bh,
                        &font_ref,
                        main_size,
                        pen_x,
                        baseline_y + pen_y,
                        ch,
                        &self.color1,
                        self.main_text_outline,
                        &self.color2,
                        self.distance,
                        &self.color3,
                    );
                    pen_x += adv + self.space_horizon as i32;
                }
                FontItem::RubyFont(ruby, base) => {
                    // Ruby becomes visible only when the corresponding base run is fully visible.
                    let to_draw = base.len().min(remaining);
                    if to_draw == 0 {
                        break;
                    }
                    // simplistic: render base, then render ruby above the first base character
                    let mut base_start_x = pen_x;
                    let mut base_total_adv = 0i32;

                    for (idx, ch) in base.iter().take(to_draw).enumerate() {
                        let limit = self.wrap_limit_px(*ch);
                        if limit > 0 && pen_x >= limit {
                            pen_x = self.text_start_horizon as i32;
                            pen_y += line_h;
                        }
                        if idx == 0 {
                            base_start_x = pen_x;
                        }

                        let adv = self.draw_char(
                            &mut pixel_buffer,
                            bw,
                            bh,
                            &font_ref,
                            main_size,
                            pen_x,
                            baseline_y + pen_y,
                            *ch,
                            &self.color1,
                            self.main_text_outline,
                            &self.color2,
                            self.distance,
                            &self.color3,
                        );
                        pen_x += adv + self.space_horizon as i32;
                        base_total_adv += adv + self.space_horizon as i32;
                    }

                    remaining = remaining.saturating_sub(to_draw);

                    if to_draw < base.len() {
                        // Base run partially visible: do not draw ruby yet.
                        break;
                    }

                    // ruby placement: center over base run (best-effort)
                    let ruby_y = baseline_y + pen_y - (main_size.ceil() as i32) + self.ruby_vertical as i32;
                    let mut ruby_x = base_start_x + (base_total_adv / 2) + self.ruby_horizon as i32;

                    // compute ruby width (rough)
                    let mut ruby_width = 0i32;
                    for rch in ruby.iter() {
                        let (metrics, _) = font_ref.rasterize(*rch, ruby_size);
                        ruby_width += metrics.advance_width.ceil() as i32;
                    }
                    ruby_x -= ruby_width / 2;

                    for rch in ruby {
                        let adv = self.draw_char(
                            &mut pixel_buffer,
                            bw,
                            bh,
                            &font_ref,
                            ruby_size,
                            ruby_x,
                            ruby_y,
                            rch,
                            &self.color1,
                            self.ruby_text_outline,
                            &self.color2,
                            0,
                            &self.color3,
                        );
                        ruby_x += adv;
                    }
                }
            }
        }

        self.pixel_buffer = pixel_buffer;

        Ok(())
    }
}

impl Default for TextItem {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TextManager {
    pub items: Vec<TextItem>,
    /// Bitmap for script const-string offsets (< 0x800000). Each bit marks whether it has been seen.
    pub readed_text: Vec<u32>,
}

impl Default for TextManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TextManager {
    pub fn new() -> Self {
        Self {
            items: vec![TextItem::new(); 32],
            readed_text: vec![0u32; 0x800000 / 32],
        }
    }

    /// Tick reveal-by-time for all text slots.
    pub fn tick(&mut self, delta_ms: u32) {
        for t in self.items.iter_mut() {
            t.tick_reveal(delta_ms);
        }
    }

    /// Force-reveal all characters for every loaded, non-suspended slot.
    ///
    /// This is used to implement the original engine's behavior where holding Ctrl
    /// (or issuing `ControlPulse`) makes the current line render immediately.
    ///
    /// Note: the original engine skips text updates entirely while a `TextPause`
    /// is active; we preserve that by ignoring suspended slots here.
    pub fn force_reveal_all_non_suspended(&mut self) {
        for t in self.items.iter_mut() {
            if !t.loaded {
                continue;
            }
            if t.is_suspended {
                continue;
            }
            if t.visible_chars != t.total_chars {
                t.visible_chars = t.total_chars;
                t.dirty = true;
            }
        }
    }


    /// Debug helper for HUD: one-line summary per text slot.
    pub fn debug_lines(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.items.len());
        for (i, t) in self.items.iter().enumerate() {
            if !t.loaded && !t.dirty && t.content_text.is_empty() && t.text_content.is_empty() {
                continue;
            }
            let mut preview = if !t.content_text.is_empty() {
                t.content_text.clone()
            } else {
                t.text_content.clone()
            };
            preview = preview.replace('\n', " ");
            if preview.len() > 120 {
                preview.truncate(120);
                preview.push_str("...");
            }
            out.push(format!(
                "slot={:02} loaded={} dirty={} suspended={} pos=({}, {}) size=({}x{}) font(name={}, text={}) speed={} reveal={}/{} text=\"{}\"",
                i,
                if t.loaded { 1 } else { 0 },
                if t.dirty { 1 } else { 0 },
                if t.is_suspended { 1 } else { 0 },
                t.x,
                t.y,
                t.w,
                t.h,
                t.font_name_id,
                t.font_text_id,
                t.speed,
                t.visible_chars,
                t.total_chars,
                preview,
            ));
        }
        out
    }

    pub fn mark_readed_text_first(&mut self, addr: u32) -> bool {
        if addr >= 0x800000 {
            return false;
        }
        let idx = (addr / 32) as usize;
        let bit = addr % 32;
        let mask = 1u32 << bit;
        let prev = self.readed_text[idx] & mask;
        self.readed_text[idx] |= mask;
        prev == 0
    }

    pub fn set_text_clear(&mut self, id: i32) {
        let text = &mut self.items[id as usize];
        if text.get_loaded() {
            text.clear_buffer();
            text.x = text.text_start_horizon;
            text.y = 0;
            text.elapsed = 0;
            text.dirty = true;
        }
    }

    pub fn set_text_buff(&mut self, id: i32, w: i32, h: i32) {
        let text = &mut self.items[id as usize];
        text.set_w(w.max(0) as u16);
        text.set_h(h.max(0) as u16);
        text.ensure_buffer();
        text.clear_buffer();
        text.loaded = true;
        text.elapsed = 0;
        text.dirty = true;
    }

    pub fn set_text_color1(&mut self, id: i32, color: &ColorItem) {
        self.items[id as usize].set_color1(color);
    }

    pub fn set_text_color2(&mut self, id: i32, color: &ColorItem) {
        self.items[id as usize].set_color2(color);
    }

    pub fn set_text_color3(&mut self, id: i32, color: &ColorItem) {
        self.items[id as usize].set_color3(color);
    }

    pub fn set_text_font_name(&mut self, id: i32, font_id: i32) {
        self.items[id as usize].set_font_name(font_id);
    }

    pub fn set_text_font_text(&mut self, id: i32, font_id: i32) {
        self.items[id as usize].set_font_text(font_id);
    }

    pub fn set_text_main_text_size(&mut self, id: i32, size: u8) {
        self.items[id as usize].set_main_text_size(size);
    }

    pub fn set_text_ruby_text_size(&mut self, id: i32, size: u8) {
        self.items[id as usize].set_ruby_text_size(size);
    }

    pub fn set_text_main_text_outline(&mut self, id: i32, outline: u8) {
        self.items[id as usize].set_main_text_outline(outline);
    }

    pub fn set_text_ruby_text_outline(&mut self, id: i32, outline: u8) {
        self.items[id as usize].set_ruby_text_outline(outline);
    }

    pub fn set_text_shadow_dist(&mut self, id: i32, dist: u8) {
        self.items[id as usize].set_shadow_dist(dist);
    }

    pub fn set_text_horizon_space(&mut self, id: i32, space: i16) {
        self.items[id as usize].set_horizon_space(space);
    }

    pub fn set_text_vertical_space(&mut self, id: i32, space: i16) {
        self.items[id as usize].set_vertical_space(space);
    }

    pub fn set_text_format(
        &mut self,
        id: i32,
        space_vertical: i16,
        space_horizon: i16,
        text_start_vertical: u16,
        text_start_horizon: u16,
        ruby_horizon: i16,
        ruby_vertical: i16,
    ) {
        let t = &mut self.items[id as usize];
        t.space_vertical = space_vertical;
        t.space_horizon = space_horizon;
        t.text_start_vertical = text_start_vertical;
        t.text_start_horizon = text_start_horizon;
        t.ruby_horizon = ruby_horizon;
        t.ruby_vertical = ruby_vertical;

        // current best-effort mapping for suspend_margin (see reverse-engineered format state)
        t.suspend_margin = text_start_vertical as i16;

        if t.x < t.text_start_horizon {
            t.x = t.text_start_horizon;
        }
        t.dirty = true;
    }

    pub fn set_text_pos_x(&mut self, id: i32, x: u16) {
        self.items[id as usize].set_text_pos_x(x);
    }

    pub fn set_text_pos_y(&mut self, id: i32, y: u16) {
        self.items[id as usize].set_text_pos_y(y);
    }

    pub fn set_text_suspend_chr(&mut self, id: i32, chrs: &str) {
        self.items[id as usize].set_text_suspend_chr(chrs);
    }

    pub fn set_text_speed(&mut self, id: i32, speed: i32) {
        self.items[id as usize].set_speed(speed);
        // reveal-by-time is not implemented yet; keep as metadata
    }

    pub fn set_text_content(&mut self, id: i32, content_text: &str) {
        self.items[id as usize].parse_content_text(content_text);
    }

    pub fn set_text_function1(&mut self, id: i32, func: u8) {
        self.items[id as usize].set_function1(func);
    }

    pub fn set_text_function2(&mut self, id: i32, func: u8) {
        self.items[id as usize].set_function2(func);
    }

    pub fn set_text_function3(&mut self, id: i32, func: u8) {
        self.items[id as usize].set_function3(func);
    }

    pub fn set_text_suspend(&mut self, id: i32, suspend: bool) {
        self.items[id as usize].set_suspend(suspend);
    }

    pub fn get_text_suspend(&self, id: i32) -> bool {
        self.items[id as usize].get_suspend()
    }

    pub fn set_text_skip(&mut self, id: i32, skip: u8) {
        self.items[id as usize].set_text_skip(skip);
    }

    pub fn set_text_space_vertical(&mut self, id: i32, space: i16) {
        self.items[id as usize].set_vertical_space(space);
    }

    pub fn set_text_space_horizon(&mut self, id: i32, space: i16) {
        self.items[id as usize].set_horizon_space(space);
    }

    /// Render (if dirty) and export RGBA8 buffer for uploading to GraphBuff.
    pub fn build_slot_rgba(
        &mut self,
        id: i32,
        fonts: &FontEnumerator,
        force: bool,
    ) -> Result<Option<(Vec<u8>, u32, u32)>> {
        if !(0..32).contains(&id) {
            return Ok(None);
        }
        let t = &mut self.items[id as usize];
        if !t.loaded {
            return Ok(None);
        }
        if force || t.dirty {
            t.rasterize_full(fonts)?;
            t.dirty = false;
        }
        let w = t.w as u32;
        let h = t.h as u32;
        let expected = (w as usize)
            .checked_mul(h as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or_else(|| anyhow!("build_slot_rgba: size overflow"))?;
        if t.pixel_buffer.len() != expected {
            bail!("build_slot_rgba: invalid buffer length");
        }
        Ok(Some((t.pixel_buffer.clone(), w, h)))
    }
}

// ----------------------------
// Save/Load snapshots
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItemSnapshotV1 {
    pub offset_x: u16,
    pub offset_y: u16,
    pub suspend_chrs: Vec<char>,

    pub text_content: String,
    pub content_text: String,

    pub font_name_id: i32,
    pub font_text_id: i32,
    pub main_text_size: u8,
    pub ruby_text_size: u8,
    pub main_text_outline: u8,
    pub ruby_text_outline: u8,
    pub distance: u8,
    pub color1: ColorItem,
    pub color2: ColorItem,
    pub color3: ColorItem,
    pub func1: u8,
    pub func2: u8,
    pub func3: u8,
    pub space_vertical: i16,
    pub space_horizon: i16,
    pub text_start_horizon: u16,
    pub text_start_vertical: u16,
    pub ruby_vertical: i16,
    pub ruby_horizon: i16,
    pub suspend_margin: i16,

    pub skip_mode: u8,
    pub is_suspended: bool,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub speed: i32,
    pub loaded: bool,
    pub pixel_buffer: Vec<u8>,
    pub dirty: bool,
    pub elapsed: u32,
    pub total_chars: usize,
    pub visible_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextManagerSnapshotV1 {
    pub items: Vec<TextItemSnapshotV1>,
    pub readed_text: Vec<u32>,
}

impl TextItem {
    fn capture_snapshot_v1(&self) -> TextItemSnapshotV1 {
        TextItemSnapshotV1 {
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            suspend_chrs: self.suspend_chrs.clone(),
            text_content: self.text_content.clone(),
            content_text: self.content_text.clone(),
            font_name_id: self.font_name_id,
            font_text_id: self.font_text_id,
            main_text_size: self.main_text_size,
            ruby_text_size: self.ruby_text_size,
            main_text_outline: self.main_text_outline,
            ruby_text_outline: self.ruby_text_outline,
            distance: self.distance,
            color1: self.color1.clone(),
            color2: self.color2.clone(),
            color3: self.color3.clone(),
            func1: self.func1,
            func2: self.func2,
            func3: self.func3,
            space_vertical: self.space_vertical,
            space_horizon: self.space_horizon,
            text_start_horizon: self.text_start_horizon,
            text_start_vertical: self.text_start_vertical,
            ruby_vertical: self.ruby_vertical,
            ruby_horizon: self.ruby_horizon,
            suspend_margin: self.suspend_margin,
            skip_mode: self.skip_mode,
            is_suspended: self.is_suspended,
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
            speed: self.speed,
            loaded: self.loaded,
            pixel_buffer: self.pixel_buffer.clone(),
            dirty: self.dirty,
            elapsed: self.elapsed,
            total_chars: self.total_chars,
            visible_chars: self.visible_chars,
        }
    }

    fn apply_snapshot_v1(&mut self, snap: &TextItemSnapshotV1) {
        self.offset_x = snap.offset_x;
        self.offset_y = snap.offset_y;
        self.suspend_chrs = snap.suspend_chrs.clone();

        self.text_content = snap.text_content.clone();
        self.content_text = snap.content_text.clone();

        self.font_name_id = snap.font_name_id;
        self.font_text_id = snap.font_text_id;
        self.main_text_size = snap.main_text_size;
        self.ruby_text_size = snap.ruby_text_size;
        self.main_text_outline = snap.main_text_outline;
        self.ruby_text_outline = snap.ruby_text_outline;
        self.distance = snap.distance;
        self.color1 = snap.color1.clone();
        self.color2 = snap.color2.clone();
        self.color3 = snap.color3.clone();
        self.func1 = snap.func1;
        self.func2 = snap.func2;
        self.func3 = snap.func3;
        self.space_vertical = snap.space_vertical;
        self.space_horizon = snap.space_horizon;
        self.text_start_horizon = snap.text_start_horizon;
        self.text_start_vertical = snap.text_start_vertical;
        self.ruby_vertical = snap.ruby_vertical;
        self.ruby_horizon = snap.ruby_horizon;
        self.suspend_margin = snap.suspend_margin;

        self.skip_mode = snap.skip_mode;
        self.is_suspended = snap.is_suspended;
        self.x = snap.x;
        self.y = snap.y;
        self.w = snap.w;
        self.h = snap.h;
        self.speed = snap.speed;
        self.loaded = snap.loaded;
        self.pixel_buffer = snap.pixel_buffer.clone();
        self.dirty = snap.dirty;
        self.elapsed = snap.elapsed;
        self.total_chars = snap.total_chars;
        self.visible_chars = snap.visible_chars;

        // Rebuild derived token list to keep future incremental rendering functional.
        self.content_items = self.content_items.clone();

        // Buffer size must match w/h.
        self.ensure_buffer();

        // If buffer is present in snapshot and size matches, prefer it.
        let expected = (self.w as usize)
            .checked_mul(self.h as usize)
            .and_then(|v| v.checked_mul(4))
            .unwrap_or(0);
        if self.pixel_buffer.len() != expected {
            // Snapshot buffer missing or inconsistent. Force re-render on next tick.
            self.pixel_buffer = vec![0; expected];
            self.dirty = true;
        }
    }
}

impl TextManager {
    pub fn capture_snapshot_v1(&self) -> TextManagerSnapshotV1 {
        TextManagerSnapshotV1 {
            items: self.items.iter().map(|t| t.capture_snapshot_v1()).collect(),
            readed_text: self.readed_text.clone(),
        }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &TextManagerSnapshotV1) {
        // Resize if needed, but keep at least 32.
        if self.items.len() != snap.items.len() {
            self.items = vec![TextItem::new(); snap.items.len().max(32)];
        }

        let n = self.items.len().min(snap.items.len());
        for i in 0..n {
            self.items[i].apply_snapshot_v1(&snap.items[i]);
        }

        self.readed_text = snap.readed_text.clone();
    }
}

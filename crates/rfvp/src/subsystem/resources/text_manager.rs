use std::fs;

use crate::{
    subsystem::resources::color_manager::ColorItem,
    utils::file::app_base_path,
};
use anyhow::{anyhow, bail, Result};
use atomic_refcell::AtomicRefCell;
use serde::{Deserialize, Serialize};
use super::gaiji_manager::GaijiManager;

// Current font (reverse-engineered special id used by TextFont(..., -1, ...)).
pub const FONTFACE_CURRENT: i32 = -1;
// ＭＳ ゴシック
pub const FONTFACE_MS_GOTHIC: i32 = -2;
// ＭＳ 明朝
pub const FONTFACE_MS_MINCHO: i32 = -3;
// ＭＳ Ｐゴシック
pub const FONTFACE_MS_PGOTHIC: i32 = -4;
// ＭＳ Ｐ明朝
pub const FONTFACE_MS_PMINCHO: i32 = -5;

#[derive(Debug, Clone)]
enum FontItem {
    Font(char),
    RubyFont(Vec<char>, Vec<char>),
    SpecialUnit(String),
    Wait(i32),
}

fn push_literal(items: &mut Vec<FontItem>, chars: &[char]) {
    for &ch in chars {
        items.push(FontItem::Font(ch));
    }
}

fn tokenize_content_text(content_text: &str, special_unit_mode: u8, ruby_text_mode: u8, wait_control_mode: u8) -> Vec<FontItem> {
    let chrs = content_text.chars().collect::<Vec<_>>();
    let mut items: Vec<FontItem> = Vec::new();
    let mut i: usize = 0;

    while i < chrs.len() {
        let chr = chrs[i];

        if chr == '[' && ruby_text_mode != 0 {
            let mut bar_idx: Option<usize> = None;
            let mut end_idx: Option<usize> = None;
            let mut j = i + 1;
            while j < chrs.len() {
                if chrs[j] == '|' {
                    bar_idx = Some(j);
                    break;
                }
                if chrs[j] == ']' {
                    break;
                }
                j += 1;
            }
            if let Some(bar) = bar_idx {
                let mut k = bar + 1;
                while k < chrs.len() {
                    if chrs[k] == ']' {
                        end_idx = Some(k);
                        break;
                    }
                    k += 1;
                }
            }
            if let (Some(bar), Some(end)) = (bar_idx, end_idx) {
                let ruby = chrs[i + 1..bar].to_vec();
                let base = chrs[bar + 1..end].to_vec();
                if ruby_text_mode == 1 {
                    push_literal(&mut items, &base);
                } else {
                    items.push(FontItem::RubyFont(ruby, base));
                }
                i = end + 1;
                continue;
            }
        }

        if chr == '{' && wait_control_mode != 0 {
            let mut end_idx: Option<usize> = None;
            let mut j = i + 1;
            let mut digits = String::new();
            while j < chrs.len() {
                if chrs[j] == '}' {
                    end_idx = Some(j);
                    break;
                }
                if !chrs[j].is_ascii_digit() {
                    end_idx = None;
                    break;
                }
                digits.push(chrs[j]);
                j += 1;
            }
            if let Some(end) = end_idx {
                if wait_control_mode == 2 {
                    let n = digits.parse::<i32>().unwrap_or(0);
                    let wait = if n == 0 { -1 } else { n.saturating_mul(100) };
                    items.push(FontItem::Wait(wait));
                }
                i = end + 1;
                continue;
            }
        }

        if chr == '<' && special_unit_mode != 0 {
            let mut end_idx: Option<usize> = None;
            let mut j = i + 1;
            let mut inner = String::new();
            while j < chrs.len() {
                if chrs[j] == '>' {
                    end_idx = Some(j);
                    break;
                }
                inner.push(chrs[j]);
                j += 1;
            }
            if let Some(end) = end_idx {
                items.push(FontItem::SpecialUnit(inner));
                i = end + 1;
                continue;
            }
        }

        items.push(FontItem::Font(chr));
        i += 1;
    }

    items
}

fn fontitem_char_count(it: &FontItem) -> usize {
    match it {
        FontItem::Font(_) => 1,
        FontItem::RubyFont(_ruby, base) => base.len(),
        FontItem::SpecialUnit(_) => 1,
        FontItem::Wait(_) => 0,
    }
}

fn is_builtin_line_start_prohibited(ch: char) -> bool {
    matches!(
        ch,
        ')' | ']' | '}'
            | '）' | '］' | '｝'
            | '〉' | '》' | '」' | '』' | '】' | '〕' | '〗' | '〙' | '〛'
            | '﹂' | '﹄' | '｠' | '»'
            | '、' | '。' | '，' | '．' | '・' | '：' | '；'
            | '！' | '？' | '!' | '?'
            | 'ヽ' | 'ヾ' | 'ゝ' | 'ゞ' | '々' | '〻'
            | 'ー' | '〜' | '゠'
            | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'っ' | 'ゃ' | 'ゅ' | 'ょ' | 'ゎ'
            | 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ' | 'ッ' | 'ャ' | 'ュ' | 'ョ' | 'ヮ' | 'ヵ' | 'ヶ'
    )
}

fn fontitem_first_char(it: &FontItem) -> Option<char> {
    match it {
        FontItem::Font(ch) => Some(*ch),
        FontItem::RubyFont(_ruby, base) => base.first().copied(),
        FontItem::SpecialUnit(_) | FontItem::Wait(_) => None,
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct RectI32 {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl RectI32 {
    fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    fn right(&self) -> i32 {
        self.x + self.w
    }

    fn bottom(&self) -> i32 {
        self.y + self.h
    }

    fn union(self, other: RectI32) -> RectI32 {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = self.right().max(other.right());
        let y1 = self.bottom().max(other.bottom());
        RectI32 { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }

    fn clamp_to_buffer(self, bw: u32, bh: u32) -> RectI32 {
        if self.is_empty() {
            return self;
        }
        let bw = bw as i32;
        let bh = bh as i32;
        let x0 = self.x.clamp(0, bw);
        let y0 = self.y.clamp(0, bh);
        let x1 = self.right().clamp(0, bw);
        let y1 = self.bottom().clamp(0, bh);
        RectI32 { x: x0, y: y0, w: (x1 - x0).max(0), h: (y1 - y0).max(0) }
    }
}

#[derive(Debug, Clone, Copy)]
struct RevealQueueItem {
    rect: RectI32,
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
            system_fontface_id: FONTFACE_MS_GOTHIC,
            current_font_name: "MS Gothic".to_string(),
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
            if md.len() == 0 {
                continue;
            }

            let p = entry.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            if ext != "ttf" && ext != "otf" && ext != "ttc" {
                continue;
            }

            let buf = fs::read(&p)?;
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
        if self.current_font_name.is_empty() {
            self.current_font_name = "MS Gothic".to_string();
        }
        Ok(())
    }

    pub fn set_current_font_name(&mut self, name: &str) {
        self.current_font_name = name.into();
    }

    pub fn get_current_font_name(&self) -> &str {
        &self.current_font_name
    }

    fn resolve_current_font(&self) -> AtomicRefCell<fontdue::Font> {
        let cur = self.current_font_name.as_str();
        if cur.eq_ignore_ascii_case("MS Gothic") || cur.eq_ignore_ascii_case("ＭＳ ゴシック") {
            return self.sys_ms_gothic.clone();
        }
        if cur.eq_ignore_ascii_case("MS Mincho") || cur.eq_ignore_ascii_case("ＭＳ 明朝") {
            return self.sys_ms_mincho.clone();
        }
        if cur.eq_ignore_ascii_case("MS PGothic") || cur.eq_ignore_ascii_case("ＭＳ Ｐゴシック") {
            return self.sys_ms_pgothic.clone();
        }
        if cur.eq_ignore_ascii_case("MS PMincho") || cur.eq_ignore_ascii_case("ＭＳ Ｐ明朝") {
            return self.sys_ms_pmincho.clone();
        }
        for (name, font) in &self.fonts {
            if name == cur {
                return font.clone();
            }
        }
        match self.system_fontface_id {
            FONTFACE_MS_GOTHIC => self.sys_ms_gothic.clone(),
            FONTFACE_MS_MINCHO => self.sys_ms_mincho.clone(),
            FONTFACE_MS_PGOTHIC => self.sys_ms_pgothic.clone(),
            FONTFACE_MS_PMINCHO => self.sys_ms_pmincho.clone(),
            _ => self.default_font.clone(),
        }
    }

    /// Font id semantics:
    /// - id < 0: built-in/system fontfaces (special ids, NOT a bug)
    /// - id == 0: default fallback font
    /// - id > 0: user-loaded fonts (1-based)
    pub fn get_font(&self, id: i32) -> AtomicRefCell<fontdue::Font> {
        match id {
            FONTFACE_CURRENT => self.resolve_current_font(),
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
            FONTFACE_CURRENT => {
                if self.current_font_name.is_empty() { None } else { Some(self.current_font_name.clone()) }
            }
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
// TextItem mirrors the reverse-engineered text object state.
// Names here are corrected to reflect confirmed parser / layout semantics rather than
// older guessed meanings.
pub struct TextItem {
    offset_x: u16,
    offset_y: u16,
    line_head_forbidden_chars: Vec<char>,

    text_content: String,
    content_text: String,
    content_items: Vec<FontItem>,

    // Keep the original idx1/idx2 naming here. These are the two text style slots copied
    // into the parser / reveal state before TextPrint runs.
    text_font_idx1: i32,
    text_font_idx2: i32,
    text_size1: u8,
    text_size2: u8,
    outline_size1: u8,
    outline_size2: u8,
    // Reverse-engineered TextShadowDist target. This participates in layout width, not only shading.
    shadow_distance: u8,
    color1: ColorItem,
    color2: ColorItem,
    color3: ColorItem,
    // Reverse-engineered TextFunction mapping:
    //   special_unit_mode -> <...>
    //   ruby_text_mode    -> [...]
    //   wait_control_mode -> {n}
    special_unit_mode: u8,
    ruby_text_mode: u8,
    wait_control_mode: u8,
    line_gap_y: i16,
    main_gap_x: i16,
    line_start_x: u16,
    wrap_reserve_right: u16,
    // IDA: ruby_gap_x (TextFormat arg5) and ruby_extra_y (arg6) are signed in [-16,16]
    ruby_extra_y: i16,
    ruby_gap_x: i16,


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

    // reveal-by-time state (column sweep)
    // total_chars: total required reveal columns in pixels across all lines (computed during rasterization)
    // visible_chars: current reveal budget in pixels (0..=total_chars)
    total_chars: usize,
    visible_chars: usize,
    wait_points: Vec<(usize, i32)>,
    next_wait_index: usize,
    pending_wait_ms: u32,
    pending_special_wait: bool,
    reveal_carry: i64,

    // Runtime-only incremental text surface state.
    // Reverse-engineered draw_text_to_texture/copy_to_texture appends newly revealed columns
    // into an already submitted texture; it does not rebuild the visible surface from column 0
    // every tick. Keep the fully rasterized sentence and the reveal queue here so reveal ticks can
    // append only the delta.
    full_buffer: Vec<u8>,
    reveal_queue: Vec<RevealQueueItem>,
    applied_visible_chars: usize,
    layout_dirty: bool,

    // Runtime-only original-engine sync-print wait state used by preview slots.
    sync_wait_thread: Option<u32>,
    sync_wait_active: bool,
}

impl TextItem {
    pub fn new() -> Self {
        Self {
            offset_x: 0,
            offset_y: 0,
            line_head_forbidden_chars: vec![],
            text_content: String::new(),
            content_text: String::new(),
            content_items: vec![],
            text_font_idx1: 0,
            text_font_idx2: 0,
            text_size1: 0,
            text_size2: 0,
            outline_size1: 0,
            outline_size2: 0,
            shadow_distance: 0,
            color1: ColorItem::new(),
            color2: ColorItem::new(),
            color3: ColorItem::new(),
            special_unit_mode: 0,
            ruby_text_mode: 0,
            wait_control_mode: 0,
            line_gap_y: 0,
            main_gap_x: 0,
            line_start_x: 0,
            wrap_reserve_right: 0,
            ruby_extra_y: 0,
            ruby_gap_x: 0,
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
            wait_points: vec![],
            next_wait_index: 0,
            pending_wait_ms: 0,
            pending_special_wait: false,
            reveal_carry: 0,
            full_buffer: vec![],
            reveal_queue: vec![],
            applied_visible_chars: 0,
            layout_dirty: false,
            sync_wait_thread: None,
            sync_wait_active: false,
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

    fn mark_layout_dirty(&mut self) {
        self.layout_dirty = true;
        self.dirty = true;
    }

    pub fn set_w(&mut self, w: u16) {
        self.w = w;
    }

    pub fn set_h(&mut self, h: u16) {
        self.h = h;
    }

    pub fn set_color1(&mut self, color: &ColorItem) {
        self.color1 = color.clone();
        self.mark_layout_dirty();
    }

    pub fn set_color2(&mut self, color: &ColorItem) {
        self.color2 = color.clone();
        self.mark_layout_dirty();
    }

    pub fn set_color3(&mut self, color: &ColorItem) {
        self.color3 = color.clone();
        self.mark_layout_dirty();
    }

    pub fn set_text_font_idx1(&mut self, id: i32) {
        self.text_font_idx1 = id;
        self.mark_layout_dirty();
    }

    pub fn set_text_font_idx2(&mut self, id: i32) {
        self.text_font_idx2 = id;
        self.mark_layout_dirty();
    }

    pub fn set_horizon_space(&mut self, space: i16) {
        self.main_gap_x = space;
        self.mark_layout_dirty();
    }

    pub fn set_vertical_space(&mut self, space: i16) {
        self.line_gap_y = space;
        self.mark_layout_dirty();
    }

    pub fn set_text_skip(&mut self, skip: u8) {
        self.skip_mode = skip;
    }

    pub fn set_text_size1(&mut self, size: u8) {
        self.text_size1 = size;
        self.mark_layout_dirty();
    }

    pub fn set_text_size2(&mut self, size: u8) {
        self.text_size2 = size;
        self.mark_layout_dirty();
    }

    pub fn set_outline_size1(&mut self, outline: u8) {
        self.outline_size1 = outline;
        self.mark_layout_dirty();
    }

    pub fn set_outline_size2(&mut self, outline: u8) {
        self.outline_size2 = outline;
        self.mark_layout_dirty();
    }

    pub fn set_shadow_dist(&mut self, dist: u8) {
        self.shadow_distance = dist;
        self.mark_layout_dirty();
    }

    pub fn set_special_unit_mode(&mut self, func: u8) {
        self.special_unit_mode = func;
    }

    pub fn set_ruby_text_mode(&mut self, func: u8) {
        self.ruby_text_mode = func;
    }

    pub fn set_wait_control_mode(&mut self, func: u8) {
        self.wait_control_mode = func;
    }

    pub fn set_text_pos_x(&mut self, x: u16) {
        self.x = x;
        // TextPos affects layout immediately.
        self.mark_layout_dirty();
    }

    pub fn set_text_pos_y(&mut self, y: u16) {
        self.y = y;
        // TextPos affects layout immediately.
        self.mark_layout_dirty();
    }

    pub fn set_line_head_forbidden_chars(&mut self, chrs: &str) {
        self.line_head_forbidden_chars = chrs.chars().collect();
        self.mark_layout_dirty();
    }

    pub fn set_speed(&mut self, speed: i32) {
        self.speed = speed;
        self.elapsed = 0;
        // Reset reveal budget; immediate modes show all.
        self.next_wait_index = 0;
        self.pending_wait_ms = 0;
        self.pending_special_wait = false;
        self.reveal_carry = 0;
        self.applied_visible_chars = 0;
        if self.speed == 0 {
            self.visible_chars = self.total_chars;
        } else {
            self.visible_chars = 0;
        }
        self.mark_layout_dirty();
    }

    fn ensure_buffer(&mut self) {
        let w = self.w as usize;
        let h = self.h as usize;
        let expected = w.saturating_mul(h).saturating_mul(4);
        if expected == 0 {
            self.pixel_buffer.clear();
            self.full_buffer.clear();
            self.reveal_queue.clear();
            self.applied_visible_chars = 0;
            self.loaded = false;
            return;
        }
        if self.pixel_buffer.len() != expected {
            self.pixel_buffer.resize(expected, 0);
        }
        if self.full_buffer.len() != expected {
            self.full_buffer.resize(expected, 0);
        }
        self.loaded = true;
    }

    fn clear_buffer(&mut self) {
        if !self.pixel_buffer.is_empty() {
            self.pixel_buffer.fill(0);
        }
        if !self.full_buffer.is_empty() {
            self.full_buffer.fill(0);
        }
        self.reveal_queue.clear();
        self.applied_visible_chars = 0;
        self.layout_dirty = false;
    }

    fn is_suspend_chr(&self, ch: char) -> bool {
        self.line_head_forbidden_chars.contains(&ch)
    }

    fn is_line_start_prohibited(&self, ch: char) -> bool {
        self.is_suspend_chr(ch) || is_builtin_line_start_prohibited(ch)
    }

    fn wrap_limit_px(&self, ch: char) -> i32 {
        let w = self.w as i32;
        if w <= 0 {
            return 0;
        }
        if self.is_line_start_prohibited(ch) {
            w
        } else {
            (w - self.wrap_reserve_right as i32).max(0)
        }
    }

    #[inline]
    fn layout_extra_advance_px(outline: u8, distance: u8) -> i32 {
        ((2 * outline as i32) + distance as i32 + 3) / 4
    }

    #[inline]
    fn effective_gdi_font_size(size: f32, outline: u8, distance: u8) -> f32 {
        let eff = 0.95f32 * (size - (outline as f32 * 0.5) - (distance as f32 * 0.25));
        eff.max(1.0)
    }

    fn measure_char_advance(
        &self,
        font: &fontdue::Font,
        size: f32,
        _gaiji: &GaijiManager,
        _size_slot: u8,
        ch: char,
        outline: u8,
        distance: u8,
    ) -> i32 {
        let extra = Self::layout_extra_advance_px(outline, distance);
        let eff_size = Self::effective_gdi_font_size(size, outline, distance);
        let (metrics, _) = font.rasterize(ch, eff_size);
        metrics.advance_width.ceil() as i32 + extra
    }

    fn measure_string_unit_advance(
        &self,
        font: &fontdue::Font,
        size: f32,
        gaiji: &GaijiManager,
        size_slot: u8,
        s: &str,
        outline: u8,
        distance: u8,
    ) -> i32 {
        let mut adv = 0i32;
        for ch in s.chars() {
            adv += self.measure_char_advance(font, size, gaiji, size_slot, ch, outline, distance);
        }
        adv
    }

    fn measure_special_unit_advance(
        &self,
        font: &fontdue::Font,
        size: f32,
        gaiji: &GaijiManager,
        size_slot: u8,
        s: &str,
        outline: u8,
        distance: u8,
    ) -> i32 {
        if let Some(gb) = gaiji.get_texture(s, size_slot) {
            return gb.get_width() as i32;
        }
        self.measure_string_unit_advance(font, size, gaiji, size_slot, s, outline, distance)
    }

    fn measure_item_advance(
        &self,
        main_font: &fontdue::Font,
        ruby_font: &fontdue::Font,
        main_size: f32,
        ruby_size: f32,
        gaiji: &GaijiManager,
        main_slot: u8,
        ruby_slot: u8,
        item: &FontItem,
    ) -> i32 {
        match item {
            FontItem::Font(ch) => self.measure_char_advance(
                main_font,
                main_size,
                gaiji,
                main_slot,
                *ch,
                self.outline_size1,
                self.shadow_distance,
            ) + self.main_gap_x as i32,
            FontItem::RubyFont(ruby, base) => {
                let mut base_adv = 0i32;
                for ch in base {
                    base_adv += self.measure_char_advance(
                        main_font,
                        main_size,
                        gaiji,
                        main_slot,
                        *ch,
                        self.outline_size1,
                        self.shadow_distance,
                    ) + self.main_gap_x as i32;
                }
                if self.ruby_text_mode == 2 {
                    let mut ruby_adv = 0i32;
                    for ch in ruby {
                        ruby_adv += self.measure_char_advance(
                            ruby_font,
                            ruby_size,
                            gaiji,
                            ruby_slot,
                            *ch,
                            self.outline_size2,
                            self.shadow_distance,
                        ) + self.ruby_gap_x as i32;
                    }
                    base_adv.max(ruby_adv)
                } else {
                    base_adv
                }
            }
            FontItem::SpecialUnit(s) => {
                self.measure_special_unit_advance(
                    main_font,
                    main_size,
                    gaiji,
                    main_slot,
                    s,
                    self.outline_size1,
                    self.shadow_distance,
                ) + self.main_gap_x as i32
            }
            FontItem::Wait(_) => 0,
        }
    }

    fn should_wrap_before_item(
        &self,
        pen_x: i32,
        line_has_any: bool,
        item_adv: i32,
        first_char: Option<char>,
    ) -> bool {
        if !line_has_any || item_adv <= 0 {
            return false;
        }
        let limit = first_char.map(|ch| self.wrap_limit_px(ch)).unwrap_or(self.w as i32);
        if limit <= 0 {
            return false;
        }
        pen_x + item_adv > limit
    }

    fn parse_content_text(&mut self, content_text: &str) {
        let items = tokenize_content_text(content_text, self.special_unit_mode, self.ruby_text_mode, self.wait_control_mode);

        self.text_content = content_text.to_string();
        self.content_text = content_text.to_string();
        self.content_items = items;

        // Reverse-engineered reveal works on queued texture rectangles, not raw character counts.
        // We rebuild reveal budget and wait positions during rasterization using pixel reserves.
        self.total_chars = 0;
        self.wait_points.clear();
        self.next_wait_index = 0;
        self.pending_wait_ms = 0;
        self.pending_special_wait = false;
        self.reveal_carry = 0;
        self.applied_visible_chars = 0;
        self.reveal_queue.clear();
        self.full_buffer.fill(0);
        self.sync_wait_active = false;
        self.sync_wait_thread = None;

        if self.speed == 0 {
            self.visible_chars = self.total_chars;
        } else {
            self.visible_chars = 0;
        }
        self.elapsed = 0;
        self.mark_layout_dirty();
    }

    #[inline]
    fn normalize_reveal_speed_units(speed: i64) -> i64 {
        if (1..=10).contains(&speed) {
            return speed.saturating_mul(1000);
        }
        speed
    }

    fn effective_reveal_speed_units(&self, global_var0: i32) -> i64 {
        let mut eff_speed: i64 = self.speed as i64;
        if eff_speed < 0 {
            eff_speed = global_var0 as i64;
        }
        Self::normalize_reveal_speed_units(eff_speed)
    }

    fn reveal_is_complete(&self) -> bool {
        self.visible_chars >= self.total_chars
            && self.next_wait_index >= self.wait_points.len()
            && self.pending_wait_ms == 0
            && !self.pending_special_wait
    }

    fn should_use_sync_print_wait(&self, global_var0: i32, ctrl_down: bool, pulse: bool) -> bool {
        if self.skip_mode != 0 {
            return false;
        }
        if ctrl_down || pulse {
            return false;
        }
        self.effective_reveal_speed_units(global_var0) > 0
    }

    fn arm_sync_print_wait(&mut self, thread_id: u32) {
        self.sync_wait_thread = Some(thread_id);
        self.sync_wait_active = true;
    }

    fn clear_sync_print_wait(&mut self) {
        self.sync_wait_thread = None;
        self.sync_wait_active = false;
    }

    fn tick_reveal(&mut self, delta_ms: u32, global_var0: i32, release_special_wait: bool) {
        if !self.loaded || self.is_suspended {
            return;
        }
        if self.pending_special_wait {
            if release_special_wait {
                self.pending_special_wait = false;
                self.reveal_carry = 0;
                self.dirty = true;
            } else {
                return;
            }
        }
        if self.pending_wait_ms != 0 {
            if delta_ms >= self.pending_wait_ms {
                self.pending_wait_ms = 0;
                self.reveal_carry = 0;
            } else {
                self.pending_wait_ms -= delta_ms;
                return;
            }
        }

        let mut eff_speed: i64 = self.speed as i64;
        if eff_speed < 0 {
            eff_speed = global_var0 as i64;
        }
        eff_speed = Self::normalize_reveal_speed_units(eff_speed);
        if eff_speed <= 0 {
            let target = self.total_chars;
            if self.visible_chars != target {
                self.visible_chars = target;
                if !self.layout_dirty {
                    self.apply_reveal_delta_to_current_target();
                }
                self.dirty = true;
            }
            return;
        }

        self.elapsed = self.elapsed.saturating_add(delta_ms);
        self.reveal_carry = self.reveal_carry.saturating_add(1000i64.saturating_mul(delta_ms as i64));
        let step: i64 = self.reveal_carry / eff_speed;
        self.reveal_carry %= eff_speed;
        if step <= 0 {
            return;
        }

        let mut new_units = self.visible_chars.saturating_add(step as usize).min(self.total_chars);
        if self.next_wait_index < self.wait_points.len() {
            let (wait_pos, wait_ms) = self.wait_points[self.next_wait_index];
            if self.visible_chars < wait_pos && new_units >= wait_pos {
                new_units = wait_pos;
                self.reveal_carry = 0;
                if wait_ms < 0 {
                    self.pending_special_wait = true;
                } else if wait_ms > 0 {
                    self.pending_wait_ms = wait_ms as u32;
                }
                self.next_wait_index += 1;
            }
        }

        if new_units != self.visible_chars {
            self.visible_chars = new_units;
            if !self.layout_dirty {
                self.apply_reveal_delta_to_current_target();
            }
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

        let dr = buf[idx] as u32;
        let dg = buf[idx + 1] as u32;
        let db = buf[idx + 2] as u32;
        let da = buf[idx + 3] as u32;

        let sa_u = sa as u32;
        let inv = 255u32 - sa_u;

        let out_a = sa_u + (da * inv + 127) / 255;
        if out_a == 0 {
            buf[idx] = 0;
            buf[idx + 1] = 0;
            buf[idx + 2] = 0;
            buf[idx + 3] = 0;
            return;
        }

        let tmp_r = (dr * da * inv + 127) / 255;
        let tmp_g = (dg * da * inv + 127) / 255;
        let tmp_b = (db * da * inv + 127) / 255;

        let num_r = (sr as u32) * sa_u + tmp_r;
        let num_g = (sg as u32) * sa_u + tmp_g;
        let num_b = (sb as u32) * sa_u + tmp_b;

        buf[idx] = ((num_r + out_a / 2) / out_a).min(255) as u8;
        buf[idx + 1] = ((num_g + out_a / 2) / out_a).min(255) as u8;
        buf[idx + 2] = ((num_b + out_a / 2) / out_a).min(255) as u8;
        buf[idx + 3] = out_a.min(255) as u8;
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
        clip_max_x: i32,
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
                let dx = x0 + mx as i32;
                if dx >= clip_max_x {
                    continue;
                }
                Self::put_pixel_blend(buf, bw, bh, dx, y0 + my as i32, (cr, cg, cb, a));
            }
        }
    }

    fn draw_char_advance_px(&self, raw_advance: i32, outline: u8, shadow_dist: u8) -> i32 {
        raw_advance + Self::layout_extra_advance_px(outline, shadow_dist)
    }

    fn glyph_bounds(x: i32, y: i32, w: i32, h: i32, outline: u8, shadow_dist: u8) -> RectI32 {
        if w <= 0 || h <= 0 {
            return RectI32::default();
        }
        let mut rect = RectI32 { x, y, w, h };
        let r = outline as i32;
        if r > 0 {
            rect = RectI32 { x: rect.x - r, y: rect.y - r, w: rect.w + r * 2, h: rect.h + r * 2 };
        }
        if shadow_dist != 0 {
            let d = shadow_dist as i32;
            rect = rect.union(RectI32 { x: x + d, y: y + d, w, h });
        }
        rect
    }

    fn char_draw_bounds(
        &self,
        font: &fontdue::Font,
        size: f32,
        x: i32,
        y: i32,
        ch: char,
        outline: u8,
        shadow_dist: u8,
    ) -> RectI32 {
        let eff_size = Self::effective_gdi_font_size(size, outline, shadow_dist);
        let (metrics, _) = font.rasterize(ch, eff_size);
        let gx = x + metrics.xmin;
        let gy = y - (metrics.ymin + metrics.height as i32);
        Self::glyph_bounds(gx, gy, metrics.width as i32, metrics.height as i32, outline, shadow_dist)
    }

    fn gaiji_draw_bounds(
        &self,
        gb: &crate::subsystem::resources::graph_buff::GraphBuff,
        x: i32,
        y: i32,
        _outline: u8,
        _shadow_dist: u8,
    ) -> RectI32 {
        let Some((mw, mh, ox, oy, _mask)) = gb.export_alpha_mask() else {
            return RectI32::default();
        };
        let gx = x + ox as i32;
        let gy = y + oy as i32;
        RectI32 { x: gx, y: gy, w: mw as i32, h: mh as i32 }
    }

    fn draw_char(
        &self,
        buf: &mut [u8],
        bw: u32,
        bh: u32,
        font: &fontdue::Font,
        size: f32,
        _gaiji: &GaijiManager,
        _size_slot: u8,
        x: i32,
        y: i32,
        ch: char,
        color: &ColorItem,
        outline: u8,
        outline_color: &ColorItem,
        shadow_dist: u8,
        shadow_color: &ColorItem,
        clip_max_x: i32,
        do_draw: bool,
    ) -> i32 {
        let eff_size = Self::effective_gdi_font_size(size, outline, shadow_dist);
        let (metrics, bitmap) = font.rasterize(ch, eff_size);
        let gx = x + metrics.xmin;
        let gy = y - (metrics.ymin + metrics.height as i32);

        let adv = self.draw_char_advance_px(metrics.advance_width.ceil() as i32, outline, shadow_dist);

        if !do_draw {
            return adv;
        }

        if clip_max_x <= gx {
            return adv;
        }

        if shadow_dist != 0 {
            let d = shadow_dist as i32;
            Self::draw_glyph_mask(
                buf,
                bw,
                bh,
                gx + d,
                gy + d,
                &bitmap,
                metrics.width,
                metrics.height,
                shadow_color,
                clip_max_x,
            );
        }

        if outline != 0 {
            let r = outline as i32;
            for oy in -r..=r {
                for ox in -r..=r {
                    if ox == 0 && oy == 0 {
                        continue;
                    }
                    if (ox * ox + oy * oy) > (r * r) {
                        continue;
                    }
                    Self::draw_glyph_mask(
                        buf,
                        bw,
                        bh,
                        gx + ox,
                        gy + oy,
                        &bitmap,
                        metrics.width,
                        metrics.height,
                        outline_color,
                        clip_max_x,
                    );
                }
            }
        }

        Self::draw_glyph_mask(buf, bw, bh, gx, gy, &bitmap, metrics.width, metrics.height, color, clip_max_x);

        adv
    }

    fn draw_gaiji_unit(
        &self,
        buf: &mut [u8],
        bw: u32,
        bh: u32,
        gb: &crate::subsystem::resources::graph_buff::GraphBuff,
        x: i32,
        y: i32,
        color: &ColorItem,
        _outline: u8,
        _outline_color: &ColorItem,
        _shadow_dist: u8,
        _shadow_color: &ColorItem,
        clip_max_x: i32,
        do_draw: bool,
    ) -> i32 {
        let Some((mw, mh, ox, oy, mask)) = gb.export_alpha_mask() else {
            return gb.get_width() as i32;
        };

        let gx = x + ox as i32;
        let gy = y + oy as i32;
        let mw_usize = mw as usize;
        let mh_usize = mh as usize;
        let adv = gb.get_width() as i32;

        if !do_draw {
            return adv;
        }

        Self::draw_glyph_mask(buf, bw, bh, gx, gy, &mask, mw_usize, mh_usize, color, clip_max_x);
        adv
    }



    fn draw_string_unit(
        &self,
        buf: &mut [u8],
        bw: u32,
        bh: u32,
        font: &fontdue::Font,
        size: f32,
        gaiji: &GaijiManager,
        size_slot: u8,
        x: i32,
        y: i32,
        s: &str,
        color: &ColorItem,
        outline: u8,
        outline_color: &ColorItem,
        shadow_dist: u8,
        shadow_color: &ColorItem,
        clip_max_x: i32,
        do_draw: bool,
    ) -> i32 {
        let mut pen_x = x;
        for ch in s.chars() {
            let adv = self.draw_char(
                buf, bw, bh, font, size, gaiji, size_slot, pen_x, y, ch,
                color, outline, outline_color, shadow_dist, shadow_color, clip_max_x, do_draw,
            );
            pen_x += adv;
        }
        pen_x - x
    }

    fn trim_reveal_rect_to_alpha(src: &[u8], bw: u32, bh: u32, rect: RectI32) -> RectI32 {
        let rect = rect.clamp_to_buffer(bw, bh);
        if rect.is_empty() {
            return rect;
        }

        let bw_usize = bw as usize;
        let x0 = rect.x as usize;
        let y0 = rect.y as usize;
        let w = rect.w as usize;
        let h = rect.h as usize;

        let mut left = None;
        let mut right = None;

        for col in 0..w {
            let mut any = false;
            for row in 0..h {
                let idx = (((y0 + row) * bw_usize + (x0 + col)) * 4 + 3) as usize;
                if src[idx] != 0 {
                    any = true;
                    break;
                }
            }
            if any {
                left = Some(col as i32);
                break;
            }
        }

        for col in (0..w).rev() {
            let mut any = false;
            for row in 0..h {
                let idx = (((y0 + row) * bw_usize + (x0 + col)) * 4 + 3) as usize;
                if src[idx] != 0 {
                    any = true;
                    break;
                }
            }
            if any {
                right = Some(col as i32);
                break;
            }
        }

        match (left, right) {
            (Some(l), Some(r)) if r >= l => RectI32 {
                x: rect.x + l,
                y: rect.y,
                w: r - l + 1,
                h: rect.h,
            },
            _ => RectI32::default(),
        }
    }

    fn copy_reveal_columns_range(
        dst: &mut [u8],
        src: &[u8],
        bw: u32,
        bh: u32,
        rect: RectI32,
        skip_cols: i32,
        cols: i32,
    ) {
        if rect.is_empty() || cols <= 0 {
            return;
        }
        let rect = rect.clamp_to_buffer(bw, bh);
        if rect.is_empty() {
            return;
        }
        let skip_cols = skip_cols.max(0).min(rect.w);
        let copy_w = cols.min(rect.w - skip_cols).max(0) as usize;
        if copy_w == 0 {
            return;
        }
        let bw_usize = bw as usize;
        let x = (rect.x + skip_cols) as usize;
        let y0 = rect.y as usize;
        let h = rect.h as usize;
        for row in 0..h {
            let yy = y0 + row;
            let start = ((yy * bw_usize + x) * 4) as usize;
            let len = copy_w * 4;
            let end = start + len;
            dst[start..end].copy_from_slice(&src[start..end]);
        }
    }

    fn clear_visible_surface(&mut self) {
        if !self.pixel_buffer.is_empty() {
            self.pixel_buffer.fill(0);
        }
        self.applied_visible_chars = 0;
    }

    fn apply_reveal_delta_to_current_target(&mut self) {
        if self.visible_chars <= self.applied_visible_chars {
            return;
        }
        if self.pixel_buffer.len() != self.full_buffer.len() {
            return;
        }
        let bw = self.w as u32;
        let bh = self.h as u32;
        let mut consumed_cols = 0usize;
        let old_visible = self.applied_visible_chars;
        let target_visible = self.visible_chars.min(self.total_chars);
        for item in &self.reveal_queue {
            let rect = item.rect;
            if rect.is_empty() {
                continue;
            }
            let item_cols = rect.w.max(0) as usize;
            let item_start = consumed_cols;
            let item_end = consumed_cols.saturating_add(item_cols);
            if target_visible <= item_start {
                break;
            }
            if old_visible < item_end {
                let start_in_item = old_visible.saturating_sub(item_start).min(item_cols) as i32;
                let end_in_item = target_visible.saturating_sub(item_start).min(item_cols) as i32;
                if end_in_item > start_in_item {
                    Self::copy_reveal_columns_range(
                        &mut self.pixel_buffer,
                        &self.full_buffer,
                        bw,
                        bh,
                        rect,
                        start_in_item,
                        end_in_item - start_in_item,
                    );
                }
            }
            consumed_cols = item_end;
        }
        self.applied_visible_chars = target_visible;
    }

    fn rebuild_visible_surface_from_cache(&mut self) {
        self.clear_visible_surface();
        self.apply_reveal_delta_to_current_target();
    }

    fn rasterize_full(&mut self, fonts: &FontEnumerator, gaiji: &GaijiManager) -> Result<()> {
        self.ensure_buffer();
        if !self.loaded {
            return Ok(());
        }

        let main_font_cell = fonts.get_font(self.text_font_idx1);
        let main_font_ref = main_font_cell.borrow();
        let ruby_font_cell = fonts.get_font(self.text_font_idx2);
        let ruby_font_ref = ruby_font_cell.borrow();

        let bw = self.w as u32;
        let bh = self.h as u32;

        let main_size = if self.text_size1 == 0 { 16.0 } else { self.text_size1 as f32 };
        let ruby_size = if self.text_size2 == 0 { (main_size * 0.6).max(8.0) } else { self.text_size2 as f32 };
        let main_layout_size = Self::effective_gdi_font_size(main_size, self.outline_size1, self.shadow_distance);
        let ruby_layout_size = Self::effective_gdi_font_size(ruby_size, self.outline_size2, self.shadow_distance);

        let main_slot: u8 = if self.text_size1 == 0 { 16 } else { self.text_size1 };
        let ruby_slot: u8 = if self.text_size2 == 0 { (main_slot as f32 * 0.6).round().clamp(8.0, 64.0) as u8 } else { self.text_size2 };

        let lm = main_font_ref.horizontal_line_metrics(main_layout_size);
        let ascent_f = lm.map(|m| m.ascent).unwrap_or(main_layout_size);
        let descent_f = lm.map(|m| m.descent).unwrap_or(-main_layout_size * 0.25);
        let line_gap_f = lm.map(|m| m.line_gap).unwrap_or(0.0);

        let main_top_reserve = ascent_f.ceil() as i32 + self.outline_size1 as i32;
        let main_bottom_reserve = (-descent_f).ceil() as i32 + self.outline_size1 as i32 + self.shadow_distance as i32;

        // Ruby must participate in vertical layout.
        // Keep the current baseline-up policy, but derive line height from the combined
        // main+ruby reserves instead of only adding ruby font size heuristically.
        let ruby_baseline_up = if self.ruby_text_mode == 2 {
            (main_layout_size.ceil() as i32 + self.ruby_extra_y as i32).max(0)
        } else {
            0
        };
        let (line_top_reserve, line_bottom_reserve) = if self.ruby_text_mode == 2 {
            let rlm = ruby_font_ref.horizontal_line_metrics(ruby_layout_size);
            let ruby_ascent_f = rlm.map(|m| m.ascent).unwrap_or(ruby_layout_size);
            let ruby_descent_f = rlm.map(|m| m.descent).unwrap_or(-ruby_layout_size * 0.25);
            let ruby_top_reserve = ruby_ascent_f.ceil() as i32 + self.outline_size2 as i32;
            let ruby_bottom_reserve = (-ruby_descent_f).ceil() as i32 + self.outline_size2 as i32 + self.shadow_distance as i32;
            (
                main_top_reserve.max(ruby_baseline_up + ruby_top_reserve),
                main_bottom_reserve.max((ruby_bottom_reserve - ruby_baseline_up).max(0)),
            )
        } else {
            (main_top_reserve, main_bottom_reserve)
        };
        let line_h = line_top_reserve + line_bottom_reserve + line_gap_f.ceil() as i32 + self.line_gap_y as i32;

        let mut pen_x = (self.x as i32).max(self.line_start_x as i32);
        let mut pen_y = self.y as i32 + line_top_reserve;

        let mut total_required_units: i64 = 0;
        let mut line_has_any = false;
        let mut full_buffer = vec![0u8; self.pixel_buffer.len()];
        let mut reveal_queue: Vec<RevealQueueItem> = Vec::new();

        self.wait_points.clear();
        self.next_wait_index = 0;

        let queue_reveal_rect = |queue: &mut Vec<RevealQueueItem>, total_required_units: &mut i64, bounds: RectI32, src: &[u8]| {
            let rect = Self::trim_reveal_rect_to_alpha(src, bw, bh, bounds);
            if rect.is_empty() {
                return;
            }
            *total_required_units += rect.w as i64;
            queue.push(RevealQueueItem { rect });
        };

        for item in self.content_items.clone() {
            match item {
                FontItem::Wait(ms) => {
                    self.wait_points.push((total_required_units.max(0) as usize, ms));
                    continue;
                }
                FontItem::SpecialUnit(s) => {
                    let item_for_measure = FontItem::SpecialUnit(s.clone());
                    let item_adv = self.measure_item_advance(
                        &main_font_ref,
                        &ruby_font_ref,
                        main_size,
                        ruby_size,
                        gaiji,
                        main_slot,
                        ruby_slot,
                        &item_for_measure,
                    );
                    if self.should_wrap_before_item(pen_x, line_has_any, item_adv, None) {
                        line_has_any = false;
                        pen_x = self.line_start_x as i32;
                        pen_y += line_h;
                    }

                    let mut item_bounds = RectI32::default();
                    if let Some(gb) = gaiji.get_texture(&s, main_slot) {
                        let adv = self.draw_gaiji_unit(
                            &mut full_buffer, bw, bh, gb, pen_x, pen_y, &self.color1,
                            self.outline_size1, &self.color2, self.shadow_distance, &self.color3, i32::MAX, true,
                        );
                        item_bounds = self.gaiji_draw_bounds(gb, pen_x, pen_y, self.outline_size1, self.shadow_distance);
                        pen_x += adv + self.main_gap_x as i32;
                    } else {
                        let mut unit_x = pen_x;
                        for ch in s.chars() {
                            let adv = self.draw_char(
                                &mut full_buffer, bw, bh, &main_font_ref, main_size, gaiji, main_slot, unit_x, pen_y, ch,
                                &self.color1, self.outline_size1, &self.color2, self.shadow_distance, &self.color3, i32::MAX, true,
                            );
                            item_bounds = item_bounds.union(self.char_draw_bounds(&main_font_ref, main_size, unit_x, pen_y, ch, self.outline_size1, self.shadow_distance));
                            unit_x += adv;
                        }
                        pen_x = unit_x + self.main_gap_x as i32;
                    }
                    queue_reveal_rect(&mut reveal_queue, &mut total_required_units, item_bounds, &full_buffer);
                    line_has_any = true;
                }
                FontItem::Font(ch) => {
                    if ch == '\n' {
                        line_has_any = false;
                        pen_x = self.line_start_x as i32;
                        pen_y += line_h;
                        continue;
                    }

                    let item_for_measure = FontItem::Font(ch);
                    let item_adv = self.measure_item_advance(
                        &main_font_ref,
                        &ruby_font_ref,
                        main_size,
                        ruby_size,
                        gaiji,
                        main_slot,
                        ruby_slot,
                        &item_for_measure,
                    );
                    if self.should_wrap_before_item(pen_x, line_has_any, item_adv, Some(ch)) {
                        line_has_any = false;
                        pen_x = self.line_start_x as i32;
                        pen_y += line_h;
                    }

                    let adv = self.draw_char(
                        &mut full_buffer, bw, bh, &main_font_ref, main_size, gaiji, main_slot, pen_x, pen_y, ch,
                        &self.color1, self.outline_size1, &self.color2, self.shadow_distance, &self.color3, i32::MAX, true,
                    );
                    let bounds = self.char_draw_bounds(&main_font_ref, main_size, pen_x, pen_y, ch, self.outline_size1, self.shadow_distance);
                    pen_x += adv + self.main_gap_x as i32;
                    queue_reveal_rect(&mut reveal_queue, &mut total_required_units, bounds, &full_buffer);
                    line_has_any = true;
                }
                FontItem::RubyFont(ruby, base) => {
                    let item_for_measure = FontItem::RubyFont(ruby.clone(), base.clone());
                    let item_adv = self.measure_item_advance(
                        &main_font_ref,
                        &ruby_font_ref,
                        main_size,
                        ruby_size,
                        gaiji,
                        main_slot,
                        ruby_slot,
                        &item_for_measure,
                    );
                    if self.should_wrap_before_item(
                        pen_x,
                        line_has_any,
                        item_adv,
                        fontitem_first_char(&item_for_measure),
                    ) {
                        line_has_any = false;
                        pen_x = self.line_start_x as i32;
                        pen_y += line_h;
                    }

                    let mut item_bounds = RectI32::default();
                    let base_start_x = pen_x;
                    let mut base_total_adv = 0i32;

                    for ch in base.iter() {
                        let adv = self.draw_char(
                            &mut full_buffer, bw, bh, &main_font_ref, main_size, gaiji, main_slot, pen_x, pen_y, *ch,
                            &self.color1, self.outline_size1, &self.color2, self.shadow_distance, &self.color3, i32::MAX, true,
                        );
                        item_bounds = item_bounds.union(self.char_draw_bounds(&main_font_ref, main_size, pen_x, pen_y, *ch, self.outline_size1, self.shadow_distance));
                        pen_x += adv + self.main_gap_x as i32;
                        base_total_adv += adv + self.main_gap_x as i32;
                    }

                    let ruby_y = pen_y - ruby_baseline_up;
                    let mut ruby_width = 0i32;
                    for rch in ruby.iter() {
                        ruby_width += self.measure_char_advance(&ruby_font_ref, ruby_size, gaiji, ruby_slot, *rch, self.outline_size2, self.shadow_distance)
                            + self.ruby_gap_x as i32;
                    }
                    let mut ruby_x = base_start_x + (base_total_adv - ruby_width) / 2;

                    for rch in ruby {
                        let adv = self.draw_char(
                            &mut full_buffer, bw, bh, &ruby_font_ref, ruby_size, gaiji, ruby_slot, ruby_x, ruby_y, rch,
                            &self.color1, self.outline_size2, &self.color2, self.shadow_distance, &self.color3, i32::MAX, true,
                        );
                        item_bounds = item_bounds.union(self.char_draw_bounds(&ruby_font_ref, ruby_size, ruby_x, ruby_y, rch, self.outline_size2, self.shadow_distance));
                        ruby_x += adv + self.ruby_gap_x as i32;
                    }

                    queue_reveal_rect(&mut reveal_queue, &mut total_required_units, item_bounds, &full_buffer);
                    line_has_any = true;
                }
            }
        }

        line_has_any = false;
        self.total_chars = total_required_units.max(0) as usize;
        if self.speed == 0 {
            self.visible_chars = self.total_chars;
        } else if self.visible_chars > self.total_chars {
            self.visible_chars = self.total_chars;
        }

        self.full_buffer = full_buffer;
        self.reveal_queue = reveal_queue;
        self.clear_visible_surface();
        self.apply_reveal_delta_to_current_target();
        self.layout_dirty = false;
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

    pub fn should_block_on_print(&self, id: i32, global_var0: i32, ctrl_down: bool, pulse: bool) -> bool {
        if !(0..32).contains(&id) {
            return false;
        }
        let t = &self.items[id as usize];
        if !t.loaded {
            return false;
        }
        t.should_use_sync_print_wait(global_var0, ctrl_down, pulse)
    }

    pub fn arm_sync_print_wait(&mut self, id: i32, thread_id: u32) {
        if !(0..32).contains(&id) {
            return;
        }
        self.items[id as usize].arm_sync_print_wait(thread_id);
    }

    pub fn collect_completed_sync_print_waiters(&mut self) -> Vec<u32> {
        let mut out = Vec::new();
        for t in self.items.iter_mut() {
            if t.sync_wait_active && t.reveal_is_complete() {
                if let Some(id) = t.sync_wait_thread.take() {
                    out.push(id);
                }
                t.sync_wait_active = false;
            }
        }
        out
    }

    /// Tick reveal-by-time for all text slots.
    pub fn tick(&mut self, delta_ms: u32, global_var0: i32, release_special_wait: bool) {
        for t in self.items.iter_mut() {
            t.tick_reveal(delta_ms, global_var0, release_special_wait);
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
            let target = t.total_chars;
            if t.visible_chars != target || t.pending_wait_ms != 0 || t.pending_special_wait {
                t.visible_chars = target;
                t.pending_wait_ms = 0;
                t.pending_special_wait = false;
                t.reveal_carry = 0;
                t.next_wait_index = t.wait_points.len();
                if !t.layout_dirty {
                    t.apply_reveal_delta_to_current_target();
                }
                t.dirty = true;
            }
            if t.reveal_is_complete() {
                t.sync_wait_active = false;
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
                t.text_font_idx1,
                t.text_font_idx2,
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
            text.text_content.clear();
            text.content_text.clear();
            text.content_items.clear();
            text.total_chars = 0;
            text.visible_chars = 0;
            text.wait_points.clear();
            text.elapsed = 0;
            text.next_wait_index = 0;
            text.pending_wait_ms = 0;
            text.pending_special_wait = false;
            text.reveal_carry = 0;
            text.applied_visible_chars = 0;
            text.layout_dirty = false;
            text.clear_sync_print_wait();
            // TextClear must upload the cleared texture immediately so any following alpha/move
            // motion acts on the already submitted blank surface, not on stale glyphs.
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

        // Reverse-engineered font_set_buff() reset state.
        text.text_content.clear();
        text.content_text.clear();
        text.content_items.clear();
        text.text_font_idx1 = -2;
        text.text_font_idx2 = -2;
        text.text_size1 = 16;
        text.text_size2 = 12;
        text.outline_size1 = 0;
        text.outline_size2 = 0;
        text.shadow_distance = 0;
        text.special_unit_mode = 0;
        text.ruby_text_mode = 0;
        text.wait_control_mode = 0;
        text.line_gap_y = 0;
        text.main_gap_x = 0;
        text.line_start_x = 0;
        text.wrap_reserve_right = 0;
        text.ruby_gap_x = 0;
        text.ruby_extra_y = 0;
        text.skip_mode = 0;
        text.is_suspended = false;
        text.x = 0;
        text.y = 0;
        text.speed = 0;
        text.total_chars = 0;
        text.visible_chars = 0;
        text.wait_points.clear();
        text.next_wait_index = 0;
        text.pending_wait_ms = 0;
        text.pending_special_wait = false;
        text.reveal_carry = 0;
        text.applied_visible_chars = 0;
        text.reveal_queue.clear();
        text.full_buffer.fill(0);
        text.layout_dirty = true;
        text.line_head_forbidden_chars.clear();
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

    pub fn set_text_font_idx1(&mut self, id: i32, font_id: i32) {
        self.items[id as usize].set_text_font_idx1(font_id);
    }

    pub fn set_text_font_idx2(&mut self, id: i32, font_id: i32) {
        self.items[id as usize].set_text_font_idx2(font_id);
    }

    pub fn set_text_size1(&mut self, id: i32, size: u8) {
        self.items[id as usize].set_text_size1(size);
    }

    pub fn set_text_size2(&mut self, id: i32, size: u8) {
        self.items[id as usize].set_text_size2(size);
    }

    pub fn set_text_outline1(&mut self, id: i32, outline: u8) {
        self.items[id as usize].set_outline_size1(outline);
    }

    pub fn set_text_outline2(&mut self, id: i32, outline: u8) {
        self.items[id as usize].set_outline_size2(outline);
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

    pub fn apply_text_format(
        &mut self,
        id: i32,
        line_gap_y: Option<i16>,
        main_gap_x: Option<i16>,
        wrap_reserve_right: Option<u16>,
        line_start_x: Option<u16>,
        ruby_gap_x: Option<i16>,
        ruby_extra_y: Option<i16>,
    ) {
        let t = &mut self.items[id as usize];
        if let Some(v) = line_gap_y {
            t.line_gap_y = v;
        }
        if let Some(v) = main_gap_x {
            t.main_gap_x = v;
        }
        if let Some(v) = wrap_reserve_right {
            t.wrap_reserve_right = v;
        }
        if let Some(v) = line_start_x {
            t.line_start_x = v;
            if t.x < v {
                t.x = v;
            }
        }
        if let Some(v) = ruby_gap_x {
            t.ruby_gap_x = v;
        }
        if let Some(v) = ruby_extra_y {
            t.ruby_extra_y = v;
        }
        t.dirty = true;
    }

    pub fn set_text_pos_x(&mut self, id: i32, x: u16) {
        self.items[id as usize].set_text_pos_x(x);
    }

    pub fn set_text_pos_y(&mut self, id: i32, y: u16) {
        self.items[id as usize].set_text_pos_y(y);
    }

    pub fn set_line_head_forbidden_chars(&mut self, id: i32, chrs: &str) {
        self.items[id as usize].set_line_head_forbidden_chars(chrs);
    }

    pub fn set_text_speed(&mut self, id: i32, speed: i32) {
        self.items[id as usize].set_speed(speed);
    }

    pub fn set_text_content(&mut self, id: i32, content_text: &str) {
        self.items[id as usize].parse_content_text(content_text);
    }

    pub fn set_text_special_unit_mode(&mut self, id: i32, func: u8) {
        let t = &mut self.items[id as usize];
        t.set_special_unit_mode(func);
        let content = t.content_text.clone();
        t.parse_content_text(&content);
    }

    pub fn set_text_ruby_mode(&mut self, id: i32, func: u8) {
        let t = &mut self.items[id as usize];
        t.set_ruby_text_mode(func);
        let content = t.content_text.clone();
        t.parse_content_text(&content);
    }

    pub fn set_text_wait_mode(&mut self, id: i32, func: u8) {
        let t = &mut self.items[id as usize];
        t.set_wait_control_mode(func);
        let content = t.content_text.clone();
        t.parse_content_text(&content);
    }

    pub fn set_text_suspend(&mut self, id: i32, suspend: bool) {
        self.items[id as usize].set_suspend(suspend);
    }
    pub fn set_text_suspend_chr(&mut self, id: i32, chrs: &str) {
        self.items[id as usize].set_line_head_forbidden_chars(chrs);
    }


    pub fn get_text_suspend(&self, id: i32) -> bool {
        self.items[id as usize].get_suspend()
    }

    pub fn set_text_skip(&mut self, id: i32, skip: u8) {
        self.items[id as usize].set_text_skip(skip);
    }

    pub fn set_text_line_gap_y(&mut self, id: i32, space: i16) {
        self.items[id as usize].set_vertical_space(space);
    }

    pub fn set_text_main_gap_x(&mut self, id: i32, space: i16) {
        self.items[id as usize].set_horizon_space(space);
    }

    /// Render a slot when needed and return its size. Pixels stay owned by the text item.
    pub fn rasterize_slot_if_needed(
        &mut self,
        id: i32,
        fonts: &FontEnumerator,
        gaiji: &GaijiManager,
        force: bool,
    ) -> Result<Option<(u32, u32)>> {
        if !(0..32).contains(&id) {
            return Ok(None);
        }
        let t = &mut self.items[id as usize];
        if !t.loaded {
            return Ok(None);
        }
        let w = t.w as u32;
        let h = t.h as u32;
        let expected = (w as usize)
            .checked_mul(h as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or_else(|| anyhow!("rasterize_slot_if_needed: size overflow"))?;
        if t.pixel_buffer.len() != expected {
            bail!("rasterize_slot_if_needed: invalid buffer length");
        }
        let needs_raster = t.layout_dirty || t.full_buffer.len() != expected;
        let needs_upload = force || t.dirty || needs_raster;
        if !needs_upload {
            return Ok(None);
        }
        if needs_raster {
            t.rasterize_full(fonts, gaiji)?;
        }
        t.dirty = false;
        Ok(Some((w, h)))
    }

    pub fn slot_rgba_bytes(&self, id: i32) -> Option<&[u8]> {
        if !(0..32).contains(&id) {
            return None;
        }
        let t = &self.items[id as usize];
        if !t.loaded {
            return None;
        }
        Some(&t.pixel_buffer)
    }
}

// ----------------------------
// Save/Load snapshots
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItemSnapshotV1 {
    pub offset_x: u16,
    pub offset_y: u16,
    pub line_head_forbidden_chars: Vec<char>,

    pub text_content: String,
    pub content_text: String,

    pub text_font_idx1: i32,
    pub text_font_idx2: i32,
    pub text_size1: u8,
    pub text_size2: u8,
    pub outline_size1: u8,
    pub outline_size2: u8,
    pub shadow_distance: u8,
    pub color1: ColorItem,
    pub color2: ColorItem,
    pub color3: ColorItem,
    pub special_unit_mode: u8,
    pub ruby_text_mode: u8,
    pub wait_control_mode: u8,
    pub line_gap_y: i16,
    pub main_gap_x: i16,
    pub line_start_x: u16,
    pub wrap_reserve_right: u16,
    pub ruby_extra_y: i16,
    pub ruby_gap_x: i16,

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
    pub wait_points: Vec<(usize, i32)>,
    pub next_wait_index: usize,
    pub pending_wait_ms: u32,
    pub pending_special_wait: bool,
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
            line_head_forbidden_chars: self.line_head_forbidden_chars.clone(),
            text_content: self.text_content.clone(),
            content_text: self.content_text.clone(),
            text_font_idx1: self.text_font_idx1,
            text_font_idx2: self.text_font_idx2,
            text_size1: self.text_size1,
            text_size2: self.text_size2,
            outline_size1: self.outline_size1,
            outline_size2: self.outline_size2,
            shadow_distance: self.shadow_distance,
            color1: self.color1.clone(),
            color2: self.color2.clone(),
            color3: self.color3.clone(),
            special_unit_mode: self.special_unit_mode,
            ruby_text_mode: self.ruby_text_mode,
            wait_control_mode: self.wait_control_mode,
            line_gap_y: self.line_gap_y,
            main_gap_x: self.main_gap_x,
            line_start_x: self.line_start_x,
            wrap_reserve_right: self.wrap_reserve_right,
            ruby_extra_y: self.ruby_extra_y,
            ruby_gap_x: self.ruby_gap_x,
            skip_mode: self.skip_mode,
            is_suspended: self.is_suspended,
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
            speed: self.speed,
            loaded: self.loaded,
            pixel_buffer: Vec::new(),
            dirty: self.dirty,
            elapsed: self.elapsed,
            total_chars: self.total_chars,
            visible_chars: self.visible_chars,
            wait_points: self.wait_points.clone(),
            next_wait_index: self.next_wait_index,
            pending_wait_ms: self.pending_wait_ms,
            pending_special_wait: self.pending_special_wait,
        }
    }

    fn apply_snapshot_v1(&mut self, snap: &TextItemSnapshotV1) {
        self.offset_x = snap.offset_x;
        self.offset_y = snap.offset_y;
        self.line_head_forbidden_chars = snap.line_head_forbidden_chars.clone();

        self.text_content = snap.text_content.clone();
        self.content_text = snap.content_text.clone();

        self.text_font_idx1 = snap.text_font_idx1;
        self.text_font_idx2 = snap.text_font_idx2;
        self.text_size1 = snap.text_size1;
        self.text_size2 = snap.text_size2;
        self.outline_size1 = snap.outline_size1;
        self.outline_size2 = snap.outline_size2;
        self.shadow_distance = snap.shadow_distance;
        self.color1 = snap.color1.clone();
        self.color2 = snap.color2.clone();
        self.color3 = snap.color3.clone();
        self.special_unit_mode = snap.special_unit_mode;
        self.ruby_text_mode = snap.ruby_text_mode;
        self.wait_control_mode = snap.wait_control_mode;
        self.line_gap_y = snap.line_gap_y;
        self.main_gap_x = snap.main_gap_x;
        self.line_start_x = snap.line_start_x;
        self.wrap_reserve_right = snap.wrap_reserve_right;
        self.ruby_extra_y = snap.ruby_extra_y;
        self.ruby_gap_x = snap.ruby_gap_x;

        self.skip_mode = snap.skip_mode;
        self.is_suspended = snap.is_suspended;
        self.x = snap.x;
        self.y = snap.y;
        self.w = snap.w;
        self.h = snap.h;
        self.speed = snap.speed;
        self.loaded = snap.loaded;
        self.pixel_buffer.clear();
        self.dirty = snap.dirty;
        self.elapsed = snap.elapsed;
        self.total_chars = snap.total_chars;
        self.visible_chars = snap.visible_chars;
        self.wait_points = snap.wait_points.clone();
        self.next_wait_index = snap.next_wait_index;
        self.pending_wait_ms = snap.pending_wait_ms;
        self.pending_special_wait = snap.pending_special_wait;
        self.reveal_carry = 0;
        self.full_buffer.clear();
        self.reveal_queue.clear();
        self.applied_visible_chars = 0;
        self.layout_dirty = true;

        // Rebuild derived token list to keep future incremental rendering functional.
        self.content_items = tokenize_content_text(&self.content_text, self.special_unit_mode, self.ruby_text_mode, self.wait_control_mode);
        if self.total_chars > 0 && self.visible_chars > self.total_chars {
            self.visible_chars = self.total_chars;
        }

        // Buffer size must match w/h.
        self.ensure_buffer();

        // Snapshots no longer duplicate the fully rendered RGBA buffer.
        // Re-render lazily when pixels were omitted or inconsistent.
        let expected = (self.w as usize)
            .checked_mul(self.h as usize)
            .and_then(|v| v.checked_mul(4))
            .unwrap_or(0);
        if snap.pixel_buffer.is_empty() || self.pixel_buffer.len() != expected {
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

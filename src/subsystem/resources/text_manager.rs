use std::{fs, os::unix::fs::MetadataExt};

use crate::{
    subsystem::resources::color_manager::ColorItem,
    utils::file::app_base_path,
};
use anyhow::Result;
use atomic_refcell::AtomicRefCell;

// ＭＳ ゴシック
pub const FONTFACE_MS_GOTHIC: i32 = -4;
// ＭＳ 明朝
pub const FONTFACE_MS_MINCHO: i32 = -3;
// ＭＳ Ｐゴシック
pub const FONTFACE_MS_PGOTHIC: i32 = -2;
// ＭＳ Ｐ明朝
pub const FONTFACE_MS_PMINCHO: i32 = -1;

pub enum FontItem {
    Font(char),
    RubyFont(Vec<char>, Vec<char>),
}

pub struct FontEnumerator {
    default_font: AtomicRefCell<fontdue::Font>,
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
        let font = include_bytes!("./fonts/MSGOTHIC.TTF") as &[u8];
        // should never fail
        let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default()).unwrap();

        Self {
            default_font: AtomicRefCell::new(font),
            fonts: Self::enum_font().unwrap_or_default(),
            system_fontface_id: -4,
            current_font_name: "ＭＳ ゴシック".into(),
        }
    }

    fn enum_font() -> Result<Vec<(String, AtomicRefCell<fontdue::Font>)>> {
        let base_path = app_base_path();
        let path = base_path.get_path();
        let mut path = path.join("fonts");
        path.push("*.ttf");

        let matches: Vec<_> = glob::glob(&path.to_string_lossy())?.flatten().collect();
        if matches.is_empty() {
            log::warn!("No fonts found in {:?}", path);
        }

        let mut fonts = vec![];
        for i in 0..10usize {
            if i >= matches.len() {
                log::warn!("we only load 10 fonts at most");
                break;
            }
            let font_path = &matches[i];

            // avoid font is too large
            let meta_data = match fs::metadata(font_path) {
                Ok(fs) => fs,
                Err(e) => {
                    log::warn!("Failed to get metadata for font: {}", e);
                    continue;
                }
            };

            if meta_data.size() > 30 * 1024 * 1024 {
                log::warn!("Font file is too large: {}", font_path.display());
                continue;
            }

            let font = std::fs::read(font_path)?;
            let font = match fontdue::Font::from_bytes(font, fontdue::FontSettings::default()) {
                Ok(font) => font,
                Err(e) => {
                    log::warn!("Failed to load font: {}", e);
                    continue;
                }
            };
            let font_name = match font.name() {
                Some(name) => name,
                _ => {
                    log::warn!("font name is empty");
                    continue;
                }
            };

            fonts.push((font_name.to_string(), AtomicRefCell::new(font)));
        }

        Ok(fonts)
    }

    pub fn get_font(&self, id: i32) -> AtomicRefCell<fontdue::Font> {
        if self.fonts.is_empty() || id >= self.fonts.len() as i32 {
            self.default_font.clone()
        } else {
            self.fonts[id as usize - 1].1.clone()
        }
    }

    pub fn get_font_name(&self, id: i32) -> Option<String> {
        if self.fonts.is_empty() || id >= self.fonts.len() as i32 {
            None
        } else {
            Some(self.fonts[id as usize - 1].0.clone())
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

    pub fn get_current_font_name(&self) -> &str {
        &self.current_font_name
    }

    pub fn set_current_font_name(&mut self, name: &str) {
        self.current_font_name = name.into();
    }
}

#[derive(Debug, Clone)]
pub struct TextItem {
    offset_x: u16,
    offset_y: u16,
    suspend_chrs: Vec<char>,
    text_content: String,
    content_text: String,
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
    ruby_vertical: u16,
    ruby_horizon: u16,
    skip_mode: u8,
    is_suspended: bool,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    speed: u32,
    loaded: bool,
    pixel_buffer: Vec<u8>,
    elapsed: u32,
}

impl TextItem {
    pub fn new() -> Self {
        Self {
            offset_x: 0,
            offset_y: 0,
            suspend_chrs: vec![],
            text_content: String::new(),
            content_text: String::new(),
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
            skip_mode: 0,
            is_suspended: false,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            speed: 0,
            loaded: false,
            pixel_buffer: vec![],
            elapsed: 0,
        }
    }

    pub fn set_w(&mut self, w: u16) {
        self.w = w;
    }

    pub fn set_h(&mut self, h: u16) {
        self.h = h;
    }

    pub fn set_color1(&mut self, color: &ColorItem) {
        self.color1 = color.clone();
    }

    pub fn set_color2(&mut self, color: &ColorItem) {
        self.color2 = color.clone();
    }

    pub fn set_color3(&mut self, color: &ColorItem) {
        self.color3 = color.clone();
    }

    pub fn set_loaded(&mut self, loaded: bool) {
        self.loaded = loaded;
    }

    pub fn set_font_name(&mut self, id: i32) {
        self.font_name_id = id;
    }

    pub fn set_font_text(&mut self, id: i32) {
        self.font_text_id = id;
    }

    pub fn set_suspend_chrs(&mut self, chrs: &str) {
        self.suspend_chrs = chrs.chars().collect();
    }

    pub fn set_speed(&mut self, speed: u32) {
        self.speed = speed;
    }

    pub fn set_vertical_space(&mut self, space: i16) {
        self.space_vertical = space;
    }

    pub fn set_horizon_space(&mut self, space: i16) {
        self.space_horizon = space;
    }

    pub fn set_text_skip(&mut self, skip: u8) {
        self.skip_mode = skip;
    }

    pub fn set_main_text_size(&mut self, size: u8) {
        self.main_text_size = size;
    }

    pub fn set_ruby_text_size(&mut self, size: u8) {
        self.ruby_text_size = size;
    }

    pub fn set_text_shadow_distance(&mut self, distance: u8) {
        self.distance = distance;
    }

    pub fn set_pos_x(&mut self, x: u16) {
        self.x = x;
    }

    pub fn set_pos_y(&mut self, y: u16) {
        self.y = y;
    }

    pub fn set_suspend(&mut self, suspended: bool) {
        self.is_suspended = suspended;
    }

    pub fn get_suspend(&self) -> bool {
        self.is_suspended
    }

    pub fn set_outline(&mut self, outline: u8) {
        self.main_text_outline = outline;
    }

    pub fn set_ruby_outline(&mut self, outline: u8) {
        self.ruby_text_outline = outline;
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

    pub fn parse_content_text(&mut self, content_text: &str) {
        let content_chrs = content_text.chars().collect::<Vec<_>>();

        let mut items = vec![];
        let mut i = 0;

        // aaa[bbb|ccc]
        // aaa : normal font item
        // bbb : normal font item for the ruby
        // ccc : ruby font item
        while i < content_chrs.len() {
            let chr = content_chrs[i];
            if chr == '[' {
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
                                break;
                            }
                            k += 1;
                        }
                        break;
                    }
                    j += 1;
                }
            } else {
                items.push(FontItem::Font(chr));
                i += 1;
            }
        }
        self.content_text = content_text.to_string();

    }

    pub fn get_loaded(&self) -> bool {
        self.loaded
    }
}

pub struct TextManager {
    pub items: Vec<TextItem>,
    pub readed_text: Vec<u8>,
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
            readed_text: vec![0; 0x100000],
        }
    }

    pub fn set_text_clear(&mut self, id: i32) {
        let text = &mut self.items[id as usize];
        if text.get_loaded() {
            if !text.pixel_buffer.is_empty() {
                // zero out the pixel buffer
                text.pixel_buffer.fill(0);
            }
            text.x = text.text_start_horizon;
            text.y = 0;
            // text.gap5D5_2 = 0;
            // text.gap5D5[1] = 0;
            text.elapsed = 0;
        }
    }

    pub fn set_text_buff(&mut self, id: i32, w: i32, h: i32) {
        self.items[id as usize].set_w(w as u16);
        self.items[id as usize].set_h(h as u16);
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

    pub fn set_font_name(&mut self, id: i32, font_name_id: i32) {
        self.items[id as usize].set_font_name(font_name_id);
    }

    pub fn set_font_text(&mut self, id: i32, font_text_id: i32) {
        self.items[id as usize].set_font_text(font_text_id);
    }

    pub fn test_readed_text(&self, addr: u32) -> bool {
        if addr < 0x800000 {
            let bits = addr % 32;
            let slot = &self.readed_text[4 * (addr / 32) as usize];
            ((1 << bits) & *slot) != 0
        } else {
            false
        }
    }

    pub fn set_readed_text(&mut self, addr: u32) {
        if addr < 0x800000 {
            let bits = addr % 32;
            let slot = &mut self.readed_text[4 * (addr / 32) as usize];
            *slot |= 1 << bits;
        }
    }

    pub fn set_text_suspend_chr(&mut self, id: i32, chrs: &str) {
        self.items[id as usize].set_suspend_chrs(chrs);
    }

    pub fn set_text_speed(&mut self, id: i32, speed: i32) {
        self.items[id as usize].set_speed(speed as u32);
    }

    pub fn set_text_space_vertical(&mut self, id: i32, space: i32) {
        self.items[id as usize].set_vertical_space(space as i16);
    }

    pub fn set_text_space_horizon(&mut self, id: i32, space: i32) {
        self.items[id as usize].set_horizon_space(space as i16);
    }

    pub fn set_text_skip(&mut self, id: i32, skip: i32) {
        self.items[id as usize].set_text_skip(skip as u8);
    }

    pub fn set_text_main_text_size(&mut self, id: i32, size: i32) {
        self.items[id as usize].set_main_text_size(size as u8);
    }

    pub fn set_text_ruby_text_size(&mut self, id: i32, size: i32) {
        self.items[id as usize].set_ruby_text_size(size as u8);
    }

    pub fn set_text_shadow_distance(&mut self, id: i32, distance: i32) {
        self.items[id as usize].set_text_shadow_distance(distance as u8);
    }

    pub fn set_text_pos_x(&mut self, id: i32, x: i32) {
        self.items[id as usize].set_pos_x(x as u16);
    }

    pub fn set_text_pos_y(&mut self, id: i32, y: i32) {
        self.items[id as usize].set_pos_y(y as u16);
    }

    pub fn set_text_suspend(&mut self, id: i32, suspended: bool) {
        self.items[id as usize].set_suspend(suspended);
    }

    pub fn get_text_suspend(&self, id: i32) -> bool {
        self.items[id as usize].get_suspend()
    }

    pub fn set_text_outline(&mut self, id: i32, outline: i32) {
        self.items[id as usize].set_outline(outline as u8);
    }

    pub fn set_text_ruby_outline(&mut self, id: i32, outline: i32) {
        self.items[id as usize].set_ruby_outline(outline as u8);
    }

    pub fn set_text_function1(&mut self, id: i32, func: i32) {
        self.items[id as usize].set_function1(func as u8);
    }

    pub fn set_text_function2(&mut self, id: i32, func: i32) {
        self.items[id as usize].set_function2(func as u8);
    }

    pub fn set_text_function3(&mut self, id: i32, func: i32) {
        self.items[id as usize].set_function3(func as u8);
    }

    pub fn set_text_content(&mut self, id: i32, content_text: &str) {
        self.items[id as usize].parse_content_text(content_text);
    }
    
}

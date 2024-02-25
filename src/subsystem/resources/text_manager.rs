use crate::subsystem::{components::color, resources::color_manager::ColorItem};


pub enum FontItem {
    
}

pub struct TextItem {
    offset_x: u16,
    offset_y: u16,
    // char text_buff[1024];
    // FontDraw draw1;
    // FontDraw draw2;
    // _BYTE text_size1;
    font_name_id: i32,
    font_text_id: i32,
    // _BYTE text_size2;
    // _BYTE outline1;
    // _BYTE outline2;
    // _BYTE distance;
    color1: ColorItem,
    color2: ColorItem,
    color3: ColorItem,
    // _BYTE func2;
    // _BYTE func1;
    // _BYTE func3;
    space_vertical: u16,
    space_horizon: u16,
    text_start_horizon: u16,
    text_start_vertical: u16,
    ruby_vertical: u16,
    ruby_horizon: u16,
    // _BYTE gap5B6[2];
    // FontEnum font_enumer2;
    // _BYTE byte5D4;
    // _BYTE gap5D5[2];
    skip_mode: u8,
    // _BYTE gap5D5_2;
    // _BYTE loaded;
    is_suspended: bool,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    speed: u32,
    loaded: bool,
    pixel_buffer: Vec<u8>,
    // WORD unk4;
    // WORD unk5;
    // stl_string str;
    // BYTE unk[12];
    // FontEnum font_enumer3;
    // BYTE unk2[24];
    elapsed: u32,
}

impl TextItem {
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

    pub fn get_loaded(&self) -> bool {
        self.loaded
    }
}

pub struct TextManager {
    pub items: Vec<TextItem>,
}

impl Default for TextManager {
    fn default() -> Self {
        Self::new()
    }
}


impl TextManager {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
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

}
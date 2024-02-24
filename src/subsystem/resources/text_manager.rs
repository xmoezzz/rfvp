use crate::subsystem::{components::color, resources::color_manager::ColorItem};


pub enum FontItem {
    
}

pub struct TextItem {
    offset_x: u16,
    offset_y: u16,
    // char text_buff[1024];
    // _BYTE gap0[280];
    // void *pixel_buffer_rgba;
    // FontDraw draw1;
    // FontDraw draw2;
    // FontEnum font_enumer;
    // _WORD textfont_idx1;
    // _BYTE text_size1;
    // bool textfont_idx2;
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
    // _WORD space_vertical;
    // _WORD space_horizon;
    // _WORD text_start_horizon;
    // _WORD text_start_vertical;
    // _WORD ruby_vertical;
    // __declspec(align(4)) _WORD ruby_horizon;
    // _BYTE gap5B6[2];
    // FontEnum font_enumer2;
    // _BYTE byte5D4;
    // _BYTE gap5D5[2];
    // _BYTE skip_mode;
    // _BYTE gap5D5_2;
    // _BYTE loaded;
    // _BYTE is_suspended;
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    speed: u32,
    // WORD unk4;
    // WORD unk5;
    // stl_string str;
    // BYTE unk[12];
    // FontEnum font_enumer3;
    // BYTE unk2[24];
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

    pub fn set_text_buff(&mut self, id: i32, w: i32, h: i32) {
        if (0..32).contains(&id) {
            self.items[id as usize].set_w(w as u16);
            self.items[id as usize].set_h(h as u16);
        }
    }
    
    pub fn set_text_color1(&mut self, id: i32, color: &ColorItem) {
        if (0..32).contains(&id) {
            self.items[id as usize].set_color1(color);
        }
    }

    pub fn set_text_color2(&mut self, id: i32, color: &ColorItem) {
        if (0..32).contains(&id) {
            self.items[id as usize].set_color2(color);
        }
    }

    pub fn set_text_color3(&mut self, id: i32, color: &ColorItem) {
        if (0..32).contains(&id) {
            self.items[id as usize].set_color3(color);
        }
    }
}
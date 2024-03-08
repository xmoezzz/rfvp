use image::{DynamicImage, GenericImageView};
use anyhow::{Result, anyhow};

#[derive(Debug, Clone)]
pub struct GraphBuff {
    pub textures: Vec<Option<DynamicImage>>,
    pub r_value: u8,
    pub g_value: u8,
    pub b_value: u8,
    pub texture_ready: bool,
    pub texture_path: String,
    pub offset_x: u16,
    pub offset_y: u16,
    pub width: u16,
    pub height: u16,
}

impl GraphBuff {
    pub fn new() -> Self {
        Self {
            textures: vec![None; 16],
            r_value: 0,
            g_value: 0,
            b_value: 0,
            texture_ready: false,
            texture_path: String::new(),
            offset_x: 0,
            offset_y: 0,
            width: 0,
            height: 0,
        }
    }

    pub fn get_r_value(&self) -> u8 {
        self.r_value
    }

    pub fn get_g_value(&self) -> u8 {
        self.g_value
    }

    pub fn get_b_value(&self) -> u8 {
        self.b_value
    }

    pub fn get_texture_ready(&self) -> bool {
        self.texture_ready
    }

    pub fn get_texture_path(&self) -> &str {
        &self.texture_path
    }

    pub fn get_offset_x(&self) -> u16 {
        self.offset_x
    }

    pub fn get_offset_y(&self) -> u16 {
        self.offset_y
    }

    pub fn get_width(&self) -> u16 {
        self.width
    }

    pub fn get_height(&self) -> u16 {
        self.height
    }

    pub fn get_textures_mut(&mut self) -> &mut Vec<Option<DynamicImage>> {
        &mut self.textures
    }
}


pub fn copy_rect(
    src: &DynamicImage,
    src_x: u32,
    src_y: u32,
    src_w: u32,
    src_h: u32,
    dest: &mut DynamicImage,
    dest_x: u32,
    dest_y: u32,
) -> Result<()> {
    let src = src.view(src_x, src_y, src_w, src_h);
    let dest = match dest.as_mut_rgba8() {
        Some(dest) => dest,
        None => return Err(anyhow!("copy_rect: dest image is not in RGBA8 format")),
    };
    for y in 0..src_h {
        for x in 0..src_w {
            let src_pixel = src.get_pixel(x + src_x, y + src_y);
            let dest_pixel = dest.get_pixel_mut(x + dest_x, y + dest_y);
            *dest_pixel = src_pixel;
        }
    }
    
    Ok(())
}
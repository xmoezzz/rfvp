use image::{DynamicImage, GenericImageView};
use anyhow::{Result, anyhow};

use super::texture::NvsgTexture;

#[derive(Debug, Clone)]
pub struct GraphBuff {
    pub texture: Option<DynamicImage>,
    pub r_value: u8,
    pub g_value: u8,
    pub b_value: u8,
    pub texture_ready: bool,
    pub texture_path: String,
    pub offset_x: u16,
    pub offset_y: u16,
    pub width: u16,
    pub height: u16,
    pub u: u16,
    pub v: u16,
}

impl GraphBuff {
    pub fn new() -> Self {
        Self {
            texture: None,
            r_value: 0,
            g_value: 0,
            b_value: 0,
            texture_ready: false,
            texture_path: String::new(),
            offset_x: 0,
            offset_y: 0,
            width: 0,
            height: 0,
            u: 0,
            v: 0,
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

    pub fn get_u(&self) -> u16 {
        self.u
    }

    pub fn get_v(&self) -> u16 {
        self.v
    }

    pub fn get_width(&self) -> u16 {
        self.width
    }

    pub fn get_height(&self) -> u16 {
        self.height
    }

    pub fn get_texture_mut(&mut self) -> &mut Option<DynamicImage> {
        &mut self.texture
    }

    pub fn get_texture(&self) -> &Option<DynamicImage> {
        &self.texture
    }

    pub fn unload(&mut self) {
        self.texture = None;
        self.texture_ready = false;
        self.texture_path = String::new();
        self.offset_x = 0;
        self.offset_y = 0;
        self.width = 0;
        self.height = 0;
    }

    pub fn load_texture(&mut self, file_name: &str, buff: Vec<u8>) -> Result<()> {
        let mut nvsg_texture = NvsgTexture::new();
        nvsg_texture.read_texture(&buff, |typ| {
            typ == super::texture::TextureType::Single24Bit ||
            typ == super::texture::TextureType::Single32Bit
        })?;

        self.unload();
        // we don't need to split the texture into multiple 256x256 textures
        self.texture = Some(nvsg_texture.get_texture(0)?);
        self.r_value = 100;
        self.g_value = 100;
        self.b_value = 100;
        self.texture_ready = true;
        self.offset_x = nvsg_texture.get_offset_x();
        self.offset_y = nvsg_texture.get_offset_y();
        self.width = nvsg_texture.get_width();
        self.height = nvsg_texture.get_height();
        self.u = nvsg_texture.get_u();
        self.v = nvsg_texture.get_v();
        self.texture_path = file_name.to_string();
    
        Ok(())
    }

    pub fn load_gaiji_fontface_glyph(&mut self, file_name: &str, buff: Vec<u8>) -> Result<()> {
        let mut nvsg_texture = NvsgTexture::new();
        nvsg_texture.read_texture(&buff, |typ| {
            typ == super::texture::TextureType::Single1Bit
        })?;

        self.unload();
        self.texture = Some(nvsg_texture.get_texture(0)?);
        self.r_value = 100;
        self.g_value = 100;
        self.b_value = 100;
        self.texture_ready = true;
        self.offset_x = nvsg_texture.get_offset_x();
        self.offset_y = nvsg_texture.get_offset_y();
        self.width = nvsg_texture.get_width();
        self.height = nvsg_texture.get_height();
        self.u = nvsg_texture.get_u();
        self.v = nvsg_texture.get_v();
        self.texture_path = file_name.to_string();
    
        Ok(())
    }

    pub fn load_mask(&mut self, file_name: &str, buff: Vec<u8>) -> Result<()> {
        let mut nvsg_texture = NvsgTexture::new();
        nvsg_texture.read_texture(&buff, |typ| {
            typ == super::texture::TextureType::Single8Bit
        })?;

        self.unload();
        self.texture = Some(nvsg_texture.get_texture(0)?);
        self.r_value = 100;
        self.g_value = 100;
        self.b_value = 100;
        self.texture_ready = true;
        self.offset_x = nvsg_texture.get_offset_x();
        self.offset_y = nvsg_texture.get_offset_y();
        self.width = nvsg_texture.get_width();
        self.height = nvsg_texture.get_height();
        self.u = nvsg_texture.get_u();
        self.v = nvsg_texture.get_v();
        self.texture_path = file_name.to_string();
    
        Ok(())
    }

    pub fn set_color_tone(
        &mut self,
        red_value: i32,
        green_value: i32,
        blue_value: i32
    ) {
        if let Some(texture) = &mut self.texture {
            if let Some(texture) = texture.as_mut_rgba8() {
                for y in 0..texture.height() {
                    for x in 0..texture.width() {
                        let pixel = texture.get_pixel_mut(x, y);
                        let mut data = pixel.0;
                        let r = data[0] as i32;
                        let g = data[1] as i32;
                        let b = data[2] as i32;
                        let a = data[3] as i32;
            
                        let r = if red_value >= 100 {
                            if red_value > 100 {
                                let green = g;
                                let adjusted_red =
                                    r.saturating_add(green.saturating_mul(red_value - 100) / 0xFF);
                                if adjusted_red > green {
                                    green
                                } else {
                                    adjusted_red
                                }
                            } else {
                                red_value * r / 100
                            }
                        } else {
                            r
                        };
            
                        let b = if green_value >= 100 {
                            if green_value > 100 {
                                let blue = b;
                                let adjusted_green =
                                    b.saturating_add(blue.saturating_mul(green_value - 100) / 0xFF);
                                if adjusted_green > blue {
                                    blue
                                } else {
                                    adjusted_green
                                }
                            } else {
                                green_value * a / 100
                            }
                        } else {
                            b
                        };
            
                        let a = if blue_value < 100 {
                            blue_value * a / 100
                        } else if blue_value > 100 {
                            let blue_value = b;
                            let adjusted_blue =
                                a.saturating_add(blue_value.saturating_mul(blue_value - 100) / 0xFF);
                            if adjusted_blue > blue_value {
                                blue_value
                            } else {
                                adjusted_blue
                            }
                        } else {
                            a
                        };
            
                        data[0] = r as u8;
                        data[1] = g as u8;
                        data[2] = b as u8;
                        data[3] = a as u8;
                        *pixel = image::Rgba(data);
                    }
                }
            }
        }
        self.r_value = red_value as u8;
        self.g_value = green_value as u8;
        self.b_value = blue_value as u8;
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
use image::{DynamicImage, GenericImageView, ImageBuffer};
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

    /// Monotonically increasing generation counter.
    ///
    /// Any CPU-side pixel mutation (including replacing the texture) must bump this counter so the
    /// GPU cache can refresh the corresponding texture.
    pub generation: u64,
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
            generation: 0,
        }
    }

    #[inline]
    pub fn get_generation(&self) -> u64 {
        self.generation
    }

    #[inline]
    pub fn mark_dirty(&mut self) {
        self.generation = self.generation.wrapping_add(1);
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
        self.u = 0;
        self.v = 0;
        self.mark_dirty();
    }

    pub fn load_texture(&mut self, file_name: &str, buff: Vec<u8>) -> Result<()> {
        let mut nvsg_texture = NvsgTexture::new(file_name);
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
        self.mark_dirty();
    
        Ok(())
    }

    pub fn load_gaiji_fontface_glyph(&mut self, file_name: &str, buff: Vec<u8>) -> Result<()> {
        let mut nvsg_texture = NvsgTexture::new(file_name);
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
        self.mark_dirty();
    
        Ok(())
    }

    pub fn load_mask(&mut self, file_name: &str, buff: Vec<u8>) -> Result<()> {
        let mut nvsg_texture = NvsgTexture::new(file_name);
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
        self.mark_dirty();
    
        Ok(())
    }

    pub fn load_from_buff(&mut self, buff: Vec<u8>, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(anyhow!("load_from_buff: invalid size {}x{}", width, height));
        }

        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or_else(|| anyhow!("load_from_buff: size overflow ({}x{}x4)", width, height))?;

        if buff.len() != expected {
            return Err(anyhow!(
                "load_from_buff: invalid buffer length: got {}, expected {} ({}x{}x4)",
                buff.len(),
                expected,
                width,
                height
            ));
        }

        // Buffers provided through this interface are treated as RGBA8.
        let img = image::RgbaImage::from_raw(width, height, buff)
            .ok_or_else(|| anyhow!("load_from_buff: RgbaImage::from_raw failed ({}x{})", width, height))?;

        self.texture = Some(DynamicImage::ImageRgba8(img));
        self.texture_ready = true;

        self.texture_path.clear();
        self.offset_x = 0;
        self.offset_y = 0;

        if width > u16::MAX as u32 || height > u16::MAX as u32 {
            return Err(anyhow!(
                "load_from_buff: size too large for u16: {}x{}",
                width,
                height
            ));
        }
        self.width = width as u16;
        self.height = height as u16;

        self.u = 0;
        self.v = 0;

        self.mark_dirty();
        Ok(())
    }

    pub fn set_color_tone(
        &mut self,
        red_value: i32,
        green_value: i32,
        blue_value: i32
    ) {
        // IDA: GraphRGB -> color_tone_texture -> apply_color_tone.
        // apply_color_tone operates on premultiplied-alpha BGRA pixels and clamps channels to alpha.
        // For RGBA8 in Rust, we apply the same math per channel, clamping to A.

        // Match the original engine: no-op if the texture is not ready.
        if !self.texture_ready {
            return;
        };

        let Some(texture) = &mut self.texture else {
            return;
        };
        let Some(texture) = texture.as_mut_rgba8() else {
            return;
        };

        // In the original engine, (100,100,100) is a no-op on pixels.
        // Avoid bumping generation in this specific case to prevent misleading debug noise.
        if red_value == 100 && green_value == 100 && blue_value == 100 {
            self.r_value = 100;
            self.g_value = 100;
            self.b_value = 100;
            return;
        }

        // Clamp inputs to the script contract (0..=200). This matches the syscall layer.
        let r_adj = red_value.clamp(0, 200) as u32;
        let g_adj = green_value.clamp(0, 200) as u32;
        let b_adj = blue_value.clamp(0, 200) as u32;

        #[inline]
        fn apply_one(chan: u8, alpha: u8, adj: u32) -> u8 {
            if adj == 100 {
                return chan;
            }
            let c = chan as u32;
            let a = alpha as u32;
            if adj < 100 {
                // Darken: C = adj * C / 100
                ((adj * c) / 100).min(255) as u8
            } else {
                // Brighten: C = min(A, C + A * (adj - 100) / 255)
                let inc = (a * (adj - 100)) / 255;
                let out = c + inc;
                out.min(a).min(255) as u8
            }
        }

        for px in texture.pixels_mut() {
            let a = px.0[3];
            // Clamp each channel to alpha to preserve premultiplied-alpha invariant.
            px.0[0] = apply_one(px.0[0], a, r_adj).min(a);
            px.0[1] = apply_one(px.0[1], a, g_adj).min(a);
            px.0[2] = apply_one(px.0[2], a, b_adj).min(a);
        }

        self.r_value = r_adj as u8;
        self.g_value = g_adj as u8;
        self.b_value = b_adj as u8;
        self.mark_dirty();
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
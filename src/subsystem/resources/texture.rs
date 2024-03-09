use anyhow::{bail, Result};
use flate2::read::ZlibDecoder;
use std::io::Read;
use std::path::Path;

use image::{GrayAlphaImage, ImageBuffer, DynamicImage};

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureType {
    #[default]
    /// for background, etc...
    Single24Bit = 0,
    /// for character, etc...
    Single32Bit = 1,
    /// for parts
    Multi32Bit = 2,
    /// for mask
    Single8Bit = 3,
    /// for gaiji
    Single1Bit = 4,
}

impl TryFrom<u16> for TextureType {
    type Error = anyhow::Error;

    fn try_from(value: u16) -> Result<Self> {
        match value {
            0 => Ok(Self::Single24Bit),
            1 => Ok(Self::Single32Bit),
            2 => Ok(Self::Multi32Bit),
            3 => Ok(Self::Single8Bit),
            4 => Ok(Self::Single1Bit),
            _ => bail!("Invalid texture type: {}", value),
        }
    }
}

const HZC1_SIGNATURE: [u8; 4] = [b'h', b'z', b'c', b'1'];
const NVSG_SIGNATURE: [u8; 4] = [b'N', b'V', b'S', b'G'];

#[derive(Debug, Clone, Default)]
pub struct NvsgTexture {
    unknown1: u16,
    typ: TextureType,
    width: u16,
    height: u16,
    offset_x: u16,
    offset_y: u16,
    u: u16,
    v: u16,
    entry_count: u32,
    unknown3: u32,
    unknown4: u32,
    slices: Vec<Vec<u8>>,
}

impl NvsgTexture {
    pub fn new() -> Self {
        Self {
            unknown1: 0,
            typ: TextureType::Single24Bit,
            width: 0,
            height: 0,
            offset_x: 0,
            offset_y: 0,
            u: 0,
            v: 0,
            entry_count: 0,
            unknown3: 0,
            unknown4: 0,
            slices: vec![],
        }
    }

    fn read_u16le(&self, buff: &[u8], offset: usize) -> Result<u16> {
        if buff.len() < offset + 2 {
            bail!("buffer too small for u16");
        }
        Ok(u16::from_le_bytes([buff[offset], buff[offset + 1]]))
    }

    fn read_u32le(&self, buff: &[u8], offset: usize) -> Result<u32> {
        if buff.len() < offset + 4 {
            bail!("buffer too small for u32");
        }
        Ok(u32::from_le_bytes([
            buff[offset],
            buff[offset + 1],
            buff[offset + 2],
            buff[offset + 3],
        ]))
    }

    pub fn get_type(&self) -> TextureType {
        self.typ
    }

    pub fn get_width(&self) -> u16 {
        self.width
    }

    pub fn get_height(&self) -> u16 {
        self.height
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

    pub fn get_entry_count(&self) -> u32 {
        self.entry_count
    }

    pub fn read_texture<F: FnOnce(TextureType) -> bool >(&mut self, buff: &[u8], type_callback: F) -> Result<()> {
        if buff.len() < 4 || buff[..4] != HZC1_SIGNATURE {
            bail!("Invalid HZC1 header");
        }

        if buff.len() < std::mem::size_of::<HZC1HDR>() {
            bail!("buffer too small for HZC1 header");
        }

        let hzc1hdr = HZC1HDR {
            signature: [buff[0], buff[1], buff[2], buff[3]],
            original_length: self.read_u32le(buff, 4)?,
            header_length: self.read_u32le(buff, 8)?,
        };

        let data_len = buff.len() - std::mem::size_of::<HZC1HDR>();
        let data_buff = &buff[std::mem::size_of::<HZC1HDR>()..];

        if data_len < 32 {
            bail!("buffer too small for NVSG header");
        }

        let signature = [data_buff[0], data_buff[1], data_buff[2], data_buff[3]];
        self.unknown1 = self.read_u16le(data_buff, 4)?;

        let typ = self.read_u16le(data_buff, 6)?;
        self.typ = typ.try_into()?;

        self.width = self.read_u16le(data_buff, 8)?;
        self.height = self.read_u16le(data_buff, 10)?;
        self.offset_x = self.read_u16le(data_buff, 12)?;
        self.offset_y = self.read_u16le(data_buff, 14)?;
        self.u = self.read_u16le(data_buff, 16)?;
        self.v = self.read_u16le(data_buff, 18)?;
        self.entry_count = self.read_u32le(data_buff, 20)?;
        self.unknown3 = self.read_u32le(data_buff, 24)?;
        self.unknown4 = self.read_u32le(data_buff, 28)?;

        if signature != NVSG_SIGNATURE {
            bail!("Invalid NVSG header: {:?}", &signature);
        }

        if self.entry_count == 0 {
            self.entry_count = 1;
        }

        let data_buff = &data_buff[hzc1hdr.header_length as usize..];

        let depth = match self.typ {
            TextureType::Single24Bit => 3,
            TextureType::Single32Bit | TextureType::Multi32Bit => 4,
            TextureType::Single8Bit | TextureType::Single1Bit => 1,
            _ => bail!("Invalid NVSG type: {:?}", self.typ),
        };

        if !type_callback(self.typ) {
            bail!("Unexpected texture type: {:?}", self.typ);
        }

        let out_len = hzc1hdr.original_length as usize;
        let mut out_buff = vec![0; out_len];
        let mut decoder = ZlibDecoder::new(data_buff);
        decoder.read_exact(&mut out_buff)?;

        if self.typ == TextureType::Single1Bit {
            for byte in &mut out_buff {
                if *byte == 1 {
                    *byte = 0xFF;
                }
            }
        }

        let frame_len = self.width as u64 * self.height as u64 * depth;

        for i in 0..self.entry_count as u64 {
            let frame = out_buff
                .get(i as usize * frame_len as usize..(i as usize + 1) * frame_len as usize);

            if let Some(frame) = frame {
                self.slices.push(frame.to_vec());
            }
        }

        Ok(())
    }

    pub fn texture_color_tone_32(
        &mut self,
        index: usize,
        red_value: i32,
        green_value: i32,
        blue_value: i32,
    ) -> Result<()> {
        if index >= self.slices.len() {
            bail!("Invalid index: {}", index);
        }

        if self.typ != TextureType::Single32Bit && self.typ != TextureType::Multi32Bit {
            bail!("Invalid texture type: {:?}", self.typ);
        }

        let texture = &mut self.slices[index];
        let pixel_count = texture.len() / 4;
        for i in 0..pixel_count {
            let index = i * 4;
            let r = texture[index] as i32;
            let g = texture[index + 1] as i32;
            let b = texture[index + 2] as i32;
            let a = texture[index + 3] as i32;

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

            texture[index] = r as u8;
            texture[index + 1] = g as u8;
            texture[index + 2] = b as u8;
            texture[index + 3] = a as u8;
        }

        Ok(())
    }

    fn extract_8bit_texture(&self, index: usize, out_path: impl AsRef<Path>) -> Result<()> {
        let slice = &self.slices[index];
        let mut img = GrayAlphaImage::new(self.width as u32, self.height as u32);

        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let index = (y * self.width as u32 + x) as usize;
            let alpha_value = slice[index];
            *pixel = image::LumaA([alpha_value, 0xff]);
        }

        img.save(out_path)?;
        Ok(())
    }

    fn as_8bit_texture(&self, index: usize) -> Result<GrayAlphaImage> {
        let slice = &self.slices[index];
        let mut img = GrayAlphaImage::new(self.width as u32, self.height as u32);

        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let index = (y * self.width as u32 + x) as usize;
            let alpha_value = slice[index];
            *pixel = image::LumaA([alpha_value, 0xff]);
        }

        Ok(img)
    }

    fn extract_24bit_texture(&self, index: usize, out_path: impl AsRef<Path>) -> Result<()> {
        let slice = &self.slices[index];
        let mut img = ImageBuffer::new(self.width as u32, self.height as u32);

        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let index = (y * self.width as u32 + x) as usize * 3;
            let r = slice[index + 2];
            let g = slice[index + 1];
            let b = slice[index + 0];
            *pixel = image::Rgb([r, g, b]);
        }

        img.save(out_path)?;
        Ok(())
    }

    fn as_24bit_texture(&self, index: usize) -> Result<ImageBuffer<image::Rgb<u8>, Vec<u8>>> {
        let slice = &self.slices[index];
        let mut img = ImageBuffer::new(self.width as u32, self.height as u32);

        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let index = (y * self.width as u32 + x) as usize * 3;
            let r = slice[index + 2];
            let g = slice[index + 1];
            let b = slice[index + 0];
            *pixel = image::Rgb([r, g, b]);
        }

        Ok(img)
    }

    fn as_24bit_to_32bit_texture(&self, index: usize) -> Result<ImageBuffer<image::Rgba<u8>, Vec<u8>>> {
        let slice = &self.slices[index];
        let mut img = ImageBuffer::new(self.width as u32, self.height as u32);

        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let index = (y * self.width as u32 + x) as usize * 3;
            let r = slice[index + 2];
            let g = slice[index + 1];
            let b = slice[index + 0];
            *pixel = image::Rgba([r, g, b, 0xff]);
        }

        Ok(img)
    }

    fn extract_32bit_texture(&self, index: usize, out_path: impl AsRef<Path>) -> Result<()> {
        let slice = &self.slices[index];
        let mut img = ImageBuffer::new(self.width as u32, self.height as u32);

        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let index = (y * self.width as u32 + x) as usize * 4;
            let r = slice[index + 2];
            let g = slice[index + 1];
            let b = slice[index + 0];
            let a = slice[index + 3];
            *pixel = image::Rgba([r, g, b, a]);
        }

        img.save(out_path)?;
        Ok(())
    }

    fn as_32bit_texture(&self, index: usize) -> Result<ImageBuffer<image::Rgba<u8>, Vec<u8>>> {
        let slice = &self.slices[index];
        let mut img = ImageBuffer::new(self.width as u32, self.height as u32);

        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let index = (y * self.width as u32 + x) as usize * 4;
            let r = slice[index + 2];
            let g = slice[index + 1];
            let b = slice[index + 0];
            let a = slice[index + 3];
            *pixel = image::Rgba([r, g, b, a]);
        }

        Ok(img)
    }

    pub fn extract_textures(&self, output_dir: impl AsRef<Path>) -> Result<()> {
        let output_dir = output_dir.as_ref();
        if !output_dir.exists() {
            std::fs::create_dir(output_dir)?;
        }

        for i in 0..self.entry_count as usize {
            let output = output_dir.join(format!("texture_{}.bmp", i));
            match self.typ {
                TextureType::Single8Bit | TextureType::Single1Bit => {
                    self.extract_8bit_texture(i, output)?
                }
                TextureType::Single24Bit => self.extract_24bit_texture(i, output)?,
                TextureType::Single32Bit | TextureType::Multi32Bit => {
                    self.extract_32bit_texture(i, output)?
                }
                _ => bail!("Invalid texture type: {:?}", self.typ),
            }
        }

        Ok(())
    }

    pub fn get_texture(&self, index: usize) -> Result<DynamicImage> {
        if index >= self.slices.len() {
            bail!("Invalid index: {}", index);
        }

        let slice = &self.slices[index];
        let img = match self.typ {
            TextureType::Single8Bit | TextureType::Single1Bit => {
                DynamicImage::ImageLumaA8(self.as_8bit_texture(index)?)
            }
            TextureType::Single24Bit => {
                DynamicImage::ImageRgba8(self.as_24bit_to_32bit_texture(index)?)
            }
            TextureType::Single32Bit | TextureType::Multi32Bit => {
                DynamicImage::ImageRgba8(self.as_32bit_texture(index)?)
            }
            _ => bail!("Invalid texture type: {:?}", self.typ),
        };

        Ok(img)
    }

}

#[repr(C, packed)]
#[derive(Default)]
struct HZC1HDR {
    signature: [u8; 4],
    original_length: u32,
    header_length: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_read_texture() {
        let filepath = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase/BGS016b"));
        let mut file = std::fs::File::open(filepath).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();

        let mut container = NvsgTexture::new();
        container.read_texture(&buffer, |typ: TextureType| {true}).unwrap();
        let output = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase"));
        container.extract_textures(output).unwrap();
    }

    #[test]
    fn test_read_texture_2() {
        let filepath = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/BGS016a_parts"
        ));
        let mut file = std::fs::File::open(filepath).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        let mut container = NvsgTexture::new();
        container.read_texture(&buffer, |typ: TextureType| {true}).unwrap();
        let output = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/BGS016a_parts.dir"
        ));
        container.extract_textures(output).unwrap();
    }

    #[test]
    fn test_read_texture_3() {
        let filepath = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/gaiji_shiru156"
        ));
        let mut file = std::fs::File::open(filepath).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        let mut container = NvsgTexture::new();
        container.read_texture(&buffer, |typ: TextureType| {true}).unwrap();
        assert!(!container.slices.is_empty());

        let output = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/gaiji_shiru156.dir"
        ));
        container.extract_textures(output).unwrap();
    }


    #[test]
    fn test_read_texture_3_2() {
        let filepath = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/gaiji_shiru156"
        ));
        let mut file = std::fs::File::open(filepath).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        let mut container = NvsgTexture::new();
        container.read_texture(&buffer, |typ: TextureType| {true}).unwrap();
        assert!(!container.slices.is_empty());

        // print all metadata
        println!("type: {:?}", container.typ);
        println!("width: {}", container.width);
        println!("height: {}", container.height);
        println!("offset_x: {}", container.offset_x);
        println!("offset_y: {}", container.offset_y);
        println!("u: {}", container.u);
        println!("v: {}", container.v);
        println!("entry_count: {}", container.entry_count);
        println!("unknown3: {}", container.unknown3);
        println!("unknown4: {}", container.unknown4);
    }

    #[test]
    fn test_read_texture_4() {
        let filepath = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/sd_302_70"
        ));
        let mut file = std::fs::File::open(filepath).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        let mut container = NvsgTexture::new();
        container.read_texture(&buffer, |typ: TextureType| {true}).unwrap();
        let output = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/sd_302_70.dir"
        ));
        container.extract_textures(output).unwrap();
    }

    #[test]
    fn test_read_texture_5() {
        let filepath = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/sd_302_70"
        ));
        let mut file = std::fs::File::open(filepath).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        let mut container = NvsgTexture::new();
        container.read_texture(&buffer, |typ: TextureType| {true}).unwrap();
        let output = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/sd_302_70_tone"
        ));

        container.texture_color_tone_32(0, 50, 50, 50).unwrap();
        container.extract_textures(output).unwrap();
    }
}

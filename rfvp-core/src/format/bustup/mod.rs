use anyhow::{bail, Result};
use image::RgbaImage;

use crate::format::{
    pic::NvsgTexture,
    text::ZeroString,
};


pub struct Bustup {
    pub base_image: RgbaImage,
    pub origin: (u16, u16),
    pub face_chunks: Vec<RgbaImage>,
    pub current_face: usize,
}

pub fn read_bustup(source: &[u8], base_image: RgbaImage) -> Result<Bustup> {
    let mut container = NvsgTexture::new();
    container.read_texture(source, |_type| { true })?;
    let mut chunks = vec![];
    for i in 0..container.get_entry_count() {
        let chunk = container.get_texture(i as usize)?;
        if let Some(chunk) = chunk.as_rgba8() {
            chunks.push(chunk.to_owned());
        }
        else {
            bail!("Cannot convert chunk to RGBA8");
        }
    }

    let bustup = Bustup {
        base_image,
        origin: (container.get_offset_x(), container.get_offset_y()),
        face_chunks: chunks,
        current_face: 0,
    };
    
    Ok(bustup)
}

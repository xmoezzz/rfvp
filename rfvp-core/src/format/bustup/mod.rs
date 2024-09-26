//! Support for BUP files, storing the character bustup sprites.
//!
//! The BUP format is re-using the machinery from the picture format, but it has some additions on top.
//!
//! The character sprite is composed of three layers:
//! - the base image, which is the character's body
//! - the expression, which displays the character's facial expression
//! - the mouth, which displays the character's mouth
//!
//! The layers are separate because one base image can have multiple facial expressions layered on top, using storage more efficiently.
//!
//! The mouth is also separate because it is usually animated, storing multiple versions with varying openness.

use std::collections::HashMap;

use anyhow::{bail, Result};
use binrw::{BinRead, BinWrite};
use bitvec::{bitbox, vec};
use image::RgbaImage;
use rfvp_tasks::ParallelSlice;

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

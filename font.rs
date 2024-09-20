//! Support for FNT format, storing bitmap fonts with 4 mip-map levels.

use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    io,
    io::{Read, Seek, SeekFrom},
};

use anyhow::anyhow;
use binrw::{BinRead, BinResult, BinWrite, Endian, VecArgs};
use glam::{vec2, Vec2};
use image::GrayImage;
use strum::EnumIter;

use crate::format::lz77;

#[derive(BinRead, BinWrite, Debug)]
#[brw(little, magic = b"FNT4")]
#[br(assert(version == 0x01))]
#[bw(assert(*version == 0x01))]
struct FontHeader {
    pub version: u32,
    pub size: u32,
    pub ascent: u16,
    pub descent: u16,
}

#[derive(BinRead, BinWrite, Debug)]
#[brw(little)]
#[br(assert(unused == 0u8))]
#[bw(assert(*unused == 0u8))]
struct GlyphHeader {
    // terms are roughly based on https://freetype.org/freetype2/docs/glyphs/glyphs-3.html
    /// Distance between the current position of the pen and left of the glyph bitmap
    pub bearing_x: i8,
    /// Distance between the baseline and the top of the glyph bitmap
    pub bearing_y: i8,
    /// Width, without padding (glyph bitmap are padded to be a power of 2)
    pub actual_width: u8,
    /// Height, without padding (glyph bitmap are padded to be a power of 2)
    pub actual_height: u8,
    /// Amount of horizontal pen movements after drawing the glyph
    pub advance_width: u8,
    // might have been advance_height, but it's always 0
    // it's not like the engine can render text vertically, right?
    pub unused: u8,
    /// Width of the texture (should be a power of 2)
    pub texture_width: u8,
    /// Height of the texture (should be a power of 2)
    pub texture_height: u8,
    pub compressed_size: u16,
}

#[derive(Debug, Copy, Clone)]
pub struct GlyphInfo {
    /// Distance between the current position of the pen and left of the glyph bitmap
    pub bearing_x: i8,
    /// Distance between the baseline and the top of the glyph  bitmap
    pub bearing_y: i8,
    /// Amount of horizontal pen movements after drawing the glyph
    pub advance_width: u8,
    /// Width of the glyph bitmap (w/o padding)
    pub actual_width: u8,
    /// Height of the glyph bitmap (w/o padding)
    pub actual_height: u8,
    /// Width of the texture (should be a power of 2)
    pub texture_width: u8,
    /// Height of the texture (should be a power of 2)
    pub texture_height: u8,
}

impl GlyphInfo {
    pub fn actual_size(&self) -> (u32, u32) {
        (self.actual_width as u32, self.actual_height as u32)
    }

    pub fn texture_size(&self) -> (u32, u32) {
        (self.texture_width as u32, self.texture_height as u32)
    }

    pub fn actual_size_relative(&self) -> Vec2 {
        vec2(
            self.actual_width as f32 / self.texture_width as f32,
            self.actual_height as f32 / self.texture_height as f32,
        )
    }
}

impl From<GlyphHeader> for GlyphInfo {
    fn from(header: GlyphHeader) -> Self {
        Self {
            bearing_x: header.bearing_x,
            bearing_y: header.bearing_y,
            advance_width: header.advance_width,
            actual_width: header.actual_width,
            actual_height: header.actual_height,
            texture_width: header.texture_width,
            texture_height: header.texture_height,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, EnumIter)]
pub enum GlyphMipLevel {
    Level0 = 0,
    Level1 = 1,
    Level2 = 2,
    Level3 = 3,
}

/// A newtype representing an ID of the glyph within the FNT file
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlyphId(pub u32);

enum GlyphData {
    Raw(Vec<u8>),
    Compressed(Vec<u8>),
}

/// Glyph that has not been decompressed yet
///
/// Useful for on-demand decompression, as most of the glyphs are not needed right away by the game
pub struct LazyGlyph {
    info: GlyphInfo,
    texture_size: (u8, u8),
    data: GlyphData,
}

impl LazyGlyph {
    fn data(&self) -> Cow<[u8]> {
        match &self.data {
            GlyphData::Raw(data) => Cow::Borrowed(data),
            GlyphData::Compressed(data) => Cow::Owned({
                let mut result = Vec::new();
                lz77::decompress::<10>(data, &mut result);
                result
            }),
        }
    }

    pub fn decompress(&self) -> Glyph {
        let data = self.data();
        let mut data = io::Cursor::new(data);

        fn read_texture(
            width: u8,
            height: u8,
            data: &mut io::Cursor<impl AsRef<[u8]>>,
        ) -> GrayImage {
            let mut image_data = vec![0u8; width as usize * height as usize];
            data.read_exact(&mut image_data)
                .expect("Failed to read glyph texture");

            GrayImage::from_raw(width as u32, height as u32, image_data).unwrap()
        }

        let mip_level_0 = read_texture(self.texture_size.0, self.texture_size.1, &mut data);
        let mip_level_1 = read_texture(self.texture_size.0 / 2, self.texture_size.1 / 2, &mut data);
        let mip_level_2 = read_texture(self.texture_size.0 / 4, self.texture_size.1 / 4, &mut data);
        let mip_level_3 = read_texture(self.texture_size.0 / 8, self.texture_size.1 / 8, &mut data);

        Glyph {
            info: self.info,
            mip_level_0,
            mip_level_1,
            mip_level_2,
            mip_level_3,
        }
    }
}

impl BinRead for LazyGlyph {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        endian: Endian,
        _: Self::Args<'_>,
    ) -> BinResult<Self> {
        let header = GlyphHeader::read_options(reader, endian, ())?;
        let texture_size = (header.texture_width, header.texture_height);
        let compressed_size = header.compressed_size as usize;
        let uncompressed_size = header.texture_width as usize * header.texture_height as usize;
        let info = header.into();

        let data = if compressed_size == 0 {
            GlyphData::Raw(Vec::<u8>::read_options(
                reader,
                endian,
                VecArgs {
                    count: uncompressed_size,
                    inner: (),
                },
            )?)
        } else {
            GlyphData::Compressed(Vec::<u8>::read_options(
                reader,
                endian,
                VecArgs {
                    count: compressed_size,
                    inner: (),
                },
            )?)
        };

        Ok(Self {
            info,
            texture_size,
            data,
        })
    }
}

/// Glyph that has been decompressed
pub struct Glyph {
    info: GlyphInfo,
    mip_level_0: GrayImage,
    mip_level_1: GrayImage,
    mip_level_2: GrayImage,
    mip_level_3: GrayImage,
}

impl Glyph {
    pub fn get_image(&self, mip_level: GlyphMipLevel) -> &GrayImage {
        match mip_level {
            GlyphMipLevel::Level0 => &self.mip_level_0,
            GlyphMipLevel::Level1 => &self.mip_level_1,
            GlyphMipLevel::Level2 => &self.mip_level_2,
            GlyphMipLevel::Level3 => &self.mip_level_3,
        }
    }
}

impl BinRead for Glyph {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        endian: Endian,
        _: Self::Args<'_>,
    ) -> BinResult<Self> {
        let glyph = LazyGlyph::read_options(reader, endian, ())?;
        Ok(glyph.decompress())
    }
}

pub trait GlyphTrait: for<'a> BinRead<Args<'a> = ()> {
    fn get_info(&self) -> GlyphInfo;
}
impl GlyphTrait for Glyph {
    fn get_info(&self) -> GlyphInfo {
        self.info
    }
}
impl GlyphTrait for LazyGlyph {
    fn get_info(&self) -> GlyphInfo {
        self.info
    }
}

pub struct Font<G: GlyphTrait = Glyph> {
    /// Distance between the baseline and the top of the font
    ascent: u16,
    /// Distance between the baseline and the bottom of the font
    descent: u16,
    characters: Box<[GlyphId; 0x10000]>,
    glyphs: HashMap<GlyphId, G>,
}

pub type LazyFont = Font<LazyGlyph>;

impl<G: GlyphTrait> Font<G> {
    /// Get the sum of the ascent and descent, giving the total height of the font
    pub fn get_line_height(&self) -> u16 {
        self.ascent + self.descent
    }

    /// Get the distance between the baseline and the top of the font
    pub fn get_descent(&self) -> u16 {
        self.descent
    }

    /// Get the distance between the baseline and the bottom of the font
    pub fn get_ascent(&self) -> u16 {
        self.ascent
    }

    pub fn get_glyph_for_character(&self, character: u16) -> &G {
        self.glyphs
            .get(&self.characters[character as usize])
            .unwrap()
    }

    pub fn get_glyph(&self, glyph_id: GlyphId) -> Option<&G> {
        self.glyphs.get(&glyph_id)
    }

    pub fn get_character_mapping(&self) -> &[GlyphId; 0x10000] {
        &self.characters
    }

    pub fn get_glyphs(&self) -> &HashMap<GlyphId, G> {
        &self.glyphs
    }
}

fn stream_size(reader: &mut impl Seek) -> BinResult<u64> {
    let pos = reader.stream_position()?;
    let size = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(pos))?;
    Ok(size)
}

/// A macro similar to `vec![$elem; $size]` which returns a boxed array.
///
/// ```rustc
///     let _: Box<[u8; 1024]> = box_array![0; 1024];
/// ```
macro_rules! box_array {
    ($val:expr ; $len:expr) => {{
        // Use a generic function so that the pointer cast remains type-safe
        fn vec_to_boxed_array<T>(vec: Vec<T>) -> Box<[T; $len]> {
            let boxed_slice = vec.into_boxed_slice();

            let ptr = ::std::boxed::Box::into_raw(boxed_slice) as *mut [T; $len];

            unsafe { Box::from_raw(ptr) }
        }

        vec_to_boxed_array(vec![$val; $len])
    }};
}

impl<G: GlyphTrait> BinRead for Font<G> {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        endian: Endian,
        _: Self::Args<'_>,
    ) -> BinResult<Self> {
        let stream_position = reader.stream_position()?;

        let header = FontHeader::read_options(reader, endian, ())?;

        let size = stream_size(reader)?;
        if header.size != size as u32 {
            return Err(binrw::Error::Custom {
                err: Box::new(anyhow!(
                    "Font size in header does not match actual stream size"
                )),
                pos: stream_position,
            });
        }

        let mut character_table = box_array![0u32; 0x10000];
        for c in character_table.iter_mut() {
            *c = u32::read_options(reader, endian, ())?;
        }

        let mut known_glyph_offsets = HashMap::new();
        let mut characters = box_array![GlyphId(0); 0x10000];
        let mut glyphs = HashMap::new();

        for (character_index, &glyph_offset) in character_table.iter().enumerate() {
            let next_glyph_id = GlyphId(known_glyph_offsets.len() as u32);
            let glyph_id = *known_glyph_offsets
                .entry(glyph_offset)
                .or_insert(next_glyph_id);
            characters[character_index] = glyph_id;

            match glyphs.entry(glyph_id) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(entry) => {
                    reader.seek(SeekFrom::Start(glyph_offset as u64))?;
                    entry.insert(G::read_options(reader, endian, Default::default())?);
                }
            }
        }

        Ok(Font {
            ascent: header.ascent,
            descent: header.descent,
            characters,
            glyphs,
        })
    }
}

pub fn read_font<R: Read + Seek>(reader: &mut R) -> BinResult<Font> {
    Font::read_le(reader)
}

pub fn read_lazy_font<R: Read + Seek>(reader: &mut R) -> BinResult<LazyFont> {
    Font::read_le(reader)
}

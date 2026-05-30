#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "generator")]
extern crate std;

#[cfg(feature = "alloc")]
use alloc::vec::Vec;
#[cfg(feature = "generator")]
use std::{collections::BTreeSet, vec};

#[cfg(feature = "generator")]
use encoding_rs::SHIFT_JIS;
#[cfg(feature = "generator")]
use fontdue::{Font, FontSettings};

pub const MAGIC: &[u8; 8] = b"RFVPTMAP";
pub const VERSION: u16 = 1;
pub const GLYPH_WIDTH: u16 = 16;
pub const GLYPH_HEIGHT: u16 = 16;
pub const BPP: u16 = 4;
pub const PAGE_DIR_COUNT: usize = 0x1100;
pub const MISSING_PAGE_RECORD: u16 = 0xffff;
pub const GLYPH_BYTES: usize = 128;

const HEADER_SIZE: usize = 76;
const PAGE_RECORD_SIZE: usize = 36;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapFontError {
    FileTooSmall,
    InvalidMagic,
    UnsupportedVersion,
    UnsupportedGlyphFormat,
    InvalidPageDirectory,
    InvalidPageRecords,
    InvalidGlyphData,
    InvalidFallback,
}

impl core::fmt::Display for BitmapFontError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FileTooSmall => f.write_str("bitmap font file is too small"),
            Self::InvalidMagic => f.write_str("invalid bitmap font magic"),
            Self::UnsupportedVersion => f.write_str("unsupported bitmap font version"),
            Self::UnsupportedGlyphFormat => f.write_str("unsupported bitmap glyph format"),
            Self::InvalidPageDirectory => f.write_str("invalid bitmap font page directory"),
            Self::InvalidPageRecords => f.write_str("invalid bitmap font page records"),
            Self::InvalidGlyphData => f.write_str("invalid bitmap font glyph data"),
            Self::InvalidFallback => f.write_str("invalid bitmap font fallback glyph"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BitmapFontError {}

#[derive(Clone, Copy, Debug)]
pub struct GlyphBitmap<'a> {
    pub width: u16,
    pub height: u16,
    pub bpp: u16,
    pub data: &'a [u8],
}

#[derive(Clone, Copy, Debug)]
pub struct BitmapFont<'a> {
    data: &'a [u8],
    glyph_width: u16,
    glyph_height: u16,
    bpp: u16,
    page_dir_count: u32,
    page_dir_offset: u32,
    page_record_count: u32,
    page_record_offset: u32,
    glyph_count: u32,
    glyph_data_offset: u32,
    fallback_index: u32,
}

impl<'a> BitmapFont<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, BitmapFontError> {
        if data.len() < HEADER_SIZE {
            return Err(BitmapFontError::FileTooSmall);
        }
        if &data[0..8] != MAGIC {
            return Err(BitmapFontError::InvalidMagic);
        }
        let version = read_u16(data, 8).ok_or(BitmapFontError::FileTooSmall)?;
        if version != VERSION {
            return Err(BitmapFontError::UnsupportedVersion);
        }
        let glyph_width = read_u16(data, 10).ok_or(BitmapFontError::FileTooSmall)?;
        let glyph_height = read_u16(data, 12).ok_or(BitmapFontError::FileTooSmall)?;
        let bpp = read_u16(data, 14).ok_or(BitmapFontError::FileTooSmall)?;
        if glyph_width != GLYPH_WIDTH || glyph_height != GLYPH_HEIGHT || bpp != BPP {
            return Err(BitmapFontError::UnsupportedGlyphFormat);
        }

        let page_dir_count = read_u32(data, 16).ok_or(BitmapFontError::FileTooSmall)?;
        let page_dir_offset = read_u32(data, 20).ok_or(BitmapFontError::FileTooSmall)?;
        let page_record_count = read_u32(data, 24).ok_or(BitmapFontError::FileTooSmall)?;
        let page_record_offset = read_u32(data, 28).ok_or(BitmapFontError::FileTooSmall)?;
        let glyph_count = read_u32(data, 32).ok_or(BitmapFontError::FileTooSmall)?;
        let glyph_data_offset = read_u32(data, 36).ok_or(BitmapFontError::FileTooSmall)?;
        let fallback_index = read_u32(data, 40).ok_or(BitmapFontError::FileTooSmall)?;

        if page_dir_count as usize != PAGE_DIR_COUNT {
            return Err(BitmapFontError::InvalidPageDirectory);
        }
        checked_range(data.len(), page_dir_offset, page_dir_count, 2)
            .ok_or(BitmapFontError::InvalidPageDirectory)?;
        checked_range(
            data.len(),
            page_record_offset,
            page_record_count,
            PAGE_RECORD_SIZE as u32,
        )
        .ok_or(BitmapFontError::InvalidPageRecords)?;
        checked_range(
            data.len(),
            glyph_data_offset,
            glyph_count,
            GLYPH_BYTES as u32,
        )
        .ok_or(BitmapFontError::InvalidGlyphData)?;
        if glyph_count == 0 || fallback_index >= glyph_count {
            return Err(BitmapFontError::InvalidFallback);
        }

        Ok(Self {
            data,
            glyph_width,
            glyph_height,
            bpp,
            page_dir_count,
            page_dir_offset,
            page_record_count,
            page_record_offset,
            glyph_count,
            glyph_data_offset,
            fallback_index,
        })
    }

    pub fn lookup_glyph_index(&self, codepoint: u32) -> u32 {
        if codepoint > 0x10ffff || (0xd800..=0xdfff).contains(&codepoint) {
            return self.fallback_index;
        }
        let page = codepoint >> 8;
        if page >= self.page_dir_count {
            return self.fallback_index;
        }
        let low = (codepoint & 0xff) as u8;
        let dir_offset = self.page_dir_offset as usize + page as usize * 2;
        let Some(page_record_index) = read_u16(self.data, dir_offset) else {
            return self.fallback_index;
        };
        if page_record_index == MISSING_PAGE_RECORD {
            return self.fallback_index;
        }
        if page_record_index as u32 >= self.page_record_count {
            return self.fallback_index;
        }
        let record_offset =
            self.page_record_offset as usize + page_record_index as usize * PAGE_RECORD_SIZE;
        let Some(base_glyph_index) = read_u32(self.data, record_offset) else {
            return self.fallback_index;
        };
        let bitset_offset = record_offset + 4;
        let Some(bitset) = self.data.get(bitset_offset..bitset_offset + 32) else {
            return self.fallback_index;
        };
        if !bit_present(bitset, low) {
            return self.fallback_index;
        }
        let glyph_index = base_glyph_index + rank_before(bitset, low);
        if glyph_index >= self.glyph_count {
            return self.fallback_index;
        }
        glyph_index
    }

    pub fn glyph_data(&self, glyph_index: u32) -> Option<&'a [u8]> {
        if glyph_index >= self.glyph_count {
            return None;
        }
        let offset = self.glyph_data_offset as usize + glyph_index as usize * GLYPH_BYTES;
        self.data.get(offset..offset + GLYPH_BYTES)
    }

    pub fn lookup_glyph(&self, codepoint: u32) -> GlyphBitmap<'a> {
        let glyph_index = self.lookup_glyph_index(codepoint);
        let data = self
            .glyph_data(glyph_index)
            .or_else(|| self.glyph_data(self.fallback_index))
            .unwrap_or(&[]);
        GlyphBitmap {
            width: self.glyph_width,
            height: self.glyph_height,
            bpp: self.bpp,
            data,
        }
    }

    pub fn fallback_index(&self) -> u32 {
        self.fallback_index
    }
}

#[cfg(feature = "alloc")]
#[derive(Clone, Debug)]
pub struct OwnedBitmapFont {
    bytes: Vec<u8>,
}

#[cfg(feature = "alloc")]
impl OwnedBitmapFont {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, BitmapFontError> {
        BitmapFont::parse(&bytes)?;
        Ok(Self { bytes })
    }

    pub fn as_font(&self) -> Result<BitmapFont<'_>, BitmapFontError> {
        BitmapFont::parse(&self.bytes)
    }

    pub fn lookup_glyph_index(&self, codepoint: u32) -> u32 {
        self.as_font()
            .map(|font| font.lookup_glyph_index(codepoint))
            .unwrap_or(0)
    }

    pub fn lookup_glyph(&self, codepoint: u32) -> GlyphBitmap<'_> {
        self.as_font()
            .map(|font| font.lookup_glyph(codepoint))
            .unwrap_or(GlyphBitmap {
                width: GLYPH_WIDTH,
                height: GLYPH_HEIGHT,
                bpp: BPP,
                data: &[],
            })
    }

    pub fn fallback_index(&self) -> u32 {
        self.as_font()
            .map(|font| font.fallback_index())
            .unwrap_or(0)
    }
}

fn checked_range(
    file_len: usize,
    offset: u32,
    count: u32,
    item_size: u32,
) -> Option<(usize, usize)> {
    let offset = offset as usize;
    let len = (count as usize).checked_mul(item_size as usize)?;
    let end = offset.checked_add(len)?;
    if end <= file_len {
        Some((offset, end))
    } else {
        None
    }
}

fn bit_present(bitset: &[u8], low: u8) -> bool {
    let byte_index = (low / 8) as usize;
    let bit_index = low % 8;
    bitset
        .get(byte_index)
        .map(|byte| (byte & (1 << bit_index)) != 0)
        .unwrap_or(false)
}

fn rank_before(bitset: &[u8], low: u8) -> u32 {
    let full_bytes = (low / 8) as usize;
    let partial_bits = low % 8;
    let mut count = 0u32;
    for byte in bitset.iter().take(full_bytes) {
        count += byte.count_ones();
    }
    if partial_bits > 0 {
        let mask = (1u8 << partial_bits) - 1;
        count += (bitset[full_bytes] & mask).count_ones();
    }
    count
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(feature = "generator")]
#[derive(Clone, Debug)]
pub struct BitmapFontBuildOptions {
    pub glyph_width: u16,
    pub glyph_height: u16,
    pub bpp: u16,
    pub oversample: u32,
    pub fallback_codepoint: u32,
}

#[cfg(feature = "generator")]
impl Default for BitmapFontBuildOptions {
    fn default() -> Self {
        Self {
            glyph_width: GLYPH_WIDTH,
            glyph_height: GLYPH_HEIGHT,
            bpp: BPP,
            oversample: 4,
            fallback_codepoint: 0x25a1,
        }
    }
}

#[cfg(feature = "generator")]
#[derive(Debug)]
pub enum BitmapFontBuildError {
    InvalidOptions,
    InvalidFont,
    InvalidCodepoint(u32),
    MissingFallback,
    PackInputLen,
}

#[cfg(feature = "generator")]
impl core::fmt::Display for BitmapFontBuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidOptions => f.write_str("invalid bitmap font build options"),
            Self::InvalidFont => f.write_str("invalid TTF font"),
            Self::InvalidCodepoint(codepoint) => write!(f, "invalid Unicode codepoint {codepoint}"),
            Self::MissingFallback => f.write_str("fallback codepoint is not in the glyph set"),
            Self::PackInputLen => f.write_str("4bpp packing input length must be even"),
        }
    }
}

#[cfg(feature = "generator")]
impl std::error::Error for BitmapFontBuildError {}

#[cfg(feature = "generator")]
pub fn build_japanese_bitmap_font_from_ttf(
    ttf_bytes: &[u8],
    options: &BitmapFontBuildOptions,
) -> Result<Vec<u8>, BitmapFontBuildError> {
    let mut set = BTreeSet::new();
    for range in [
        0x20..=0x7e,
        0x3040..=0x309f,
        0x30a0..=0x30ff,
        0xff61..=0xff9f,
        0x3000..=0x303f,
        0xff01..=0xff5e,
    ] {
        set.extend(range);
    }
    add_cp932_double_byte_codepoints(&mut set);
    set.insert(options.fallback_codepoint);
    let codepoints: Vec<u32> = set.into_iter().collect();
    build_bitmap_font_from_codepoints(ttf_bytes, &codepoints, options)
}

#[cfg(feature = "generator")]
pub fn build_bitmap_font_from_codepoints(
    ttf_bytes: &[u8],
    codepoints: &[u32],
    options: &BitmapFontBuildOptions,
) -> Result<Vec<u8>, BitmapFontBuildError> {
    validate_options(options)?;
    let font = Font::from_bytes(ttf_bytes, FontSettings::default())
        .map_err(|_| BitmapFontBuildError::InvalidFont)?;
    let mut set = BTreeSet::new();
    for &codepoint in codepoints {
        if char::from_u32(codepoint).is_none() {
            return Err(BitmapFontBuildError::InvalidCodepoint(codepoint));
        }
        set.insert(codepoint);
    }
    set.insert(options.fallback_codepoint);
    if !set.contains(&options.fallback_codepoint) {
        return Err(BitmapFontBuildError::MissingFallback);
    }

    let mut page_records = Vec::new();
    let mut page_dir = vec![MISSING_PAGE_RECORD; PAGE_DIR_COUNT];
    let mut glyph_data = Vec::new();
    let mut glyph_index = 0u32;
    let mut fallback_index = None;
    let fallback_char = char::from_u32(options.fallback_codepoint).ok_or(
        BitmapFontBuildError::InvalidCodepoint(options.fallback_codepoint),
    )?;
    let mut missing_count = 0usize;

    for page in 0..PAGE_DIR_COUNT {
        let page_start = (page as u32) << 8;
        let lows: Vec<u8> = set
            .range(page_start..=page_start + 0xff)
            .map(|codepoint| (codepoint & 0xff) as u8)
            .collect();
        if lows.is_empty() {
            continue;
        }
        let page_record_index = page_records.len() as u16;
        page_dir[page] = page_record_index;
        let base_glyph_index = glyph_index;
        let mut bitset = [0u8; 32];
        for low in lows {
            bitset[(low / 8) as usize] |= 1 << (low % 8);
            let codepoint = page_start + low as u32;
            if codepoint == options.fallback_codepoint {
                fallback_index = Some(glyph_index);
            }
            let ch = char::from_u32(codepoint)
                .ok_or(BitmapFontBuildError::InvalidCodepoint(codepoint))?;
            let render_codepoint = if codepoint != options.fallback_codepoint && !font.has_glyph(ch)
            {
                missing_count += 1;
                fallback_char as u32
            } else {
                codepoint
            };
            glyph_data.extend_from_slice(&render_glyph_4bpp(&font, render_codepoint, options)?);
            glyph_index += 1;
        }
        page_records.push((base_glyph_index, bitset));
    }

    let fallback_index = fallback_index.ok_or(BitmapFontBuildError::MissingFallback)?;
    if missing_count > 0 {
        eprintln!("rfvp_bitmap: rendered {missing_count} missing glyphs with fallback glyph");
    }
    write_font_file(
        &page_dir,
        &page_records,
        glyph_index,
        fallback_index,
        &glyph_data,
        options,
    )
}

#[cfg(feature = "generator")]
pub fn pack_4bpp_row_or_glyph(alpha_4bit: &[u8]) -> Result<Vec<u8>, BitmapFontBuildError> {
    if alpha_4bit.len() % 2 != 0 {
        return Err(BitmapFontBuildError::PackInputLen);
    }
    let mut out = Vec::with_capacity(alpha_4bit.len() / 2);
    for pair in alpha_4bit.chunks_exact(2) {
        out.push(((pair[0] & 0x0f) << 4) | (pair[1] & 0x0f));
    }
    Ok(out)
}

#[cfg(feature = "generator")]
fn validate_options(options: &BitmapFontBuildOptions) -> Result<(), BitmapFontBuildError> {
    if options.glyph_width != GLYPH_WIDTH
        || options.glyph_height != GLYPH_HEIGHT
        || options.bpp != BPP
        || options.oversample == 0
    {
        return Err(BitmapFontBuildError::InvalidOptions);
    }
    if char::from_u32(options.fallback_codepoint).is_none() {
        return Err(BitmapFontBuildError::InvalidCodepoint(
            options.fallback_codepoint,
        ));
    }
    Ok(())
}

#[cfg(feature = "generator")]
fn add_cp932_double_byte_codepoints(set: &mut BTreeSet<u32>) {
    for lead in 0x81u8..=0xfcu8 {
        if !(0x81..=0x9f).contains(&lead) && !(0xe0..=0xfc).contains(&lead) {
            continue;
        }
        for trail in 0x40u8..=0xfcu8 {
            if trail == 0x7f {
                continue;
            }
            let bytes = [lead, trail];
            let (text, _, had_errors) = SHIFT_JIS.decode(&bytes);
            if had_errors {
                continue;
            }
            let mut chars = text.chars();
            let Some(ch) = chars.next() else {
                continue;
            };
            if chars.next().is_none() && ch != '\u{fffd}' {
                set.insert(ch as u32);
            }
        }
    }
}

#[cfg(feature = "generator")]
fn render_glyph_4bpp(
    font: &Font,
    codepoint: u32,
    options: &BitmapFontBuildOptions,
) -> Result<[u8; GLYPH_BYTES], BitmapFontBuildError> {
    let ch = char::from_u32(codepoint).ok_or(BitmapFontBuildError::InvalidCodepoint(codepoint))?;
    let scale = (options.glyph_height as f32 * options.oversample as f32 * 0.92).max(1.0);
    let (metrics, bitmap) = font.rasterize(ch, scale);
    let large_w = options.glyph_width as usize * options.oversample as usize;
    let large_h = options.glyph_height as usize * options.oversample as usize;
    let mut canvas = vec![0u8; large_w * large_h];
    let x_offset = ((large_w as i32 - metrics.width as i32) / 2).max(0) as usize;
    let y_offset = ((large_h as i32 - metrics.height as i32) / 2).max(0) as usize;
    for y in 0..metrics.height.min(large_h) {
        for x in 0..metrics.width.min(large_w) {
            let dst_x = x + x_offset;
            let dst_y = y + y_offset;
            if dst_x < large_w && dst_y < large_h {
                canvas[dst_y * large_w + dst_x] = bitmap[y * metrics.width + x];
            }
        }
    }

    let mut alpha_4bit =
        Vec::with_capacity(options.glyph_width as usize * options.glyph_height as usize);
    let os = options.oversample as usize;
    for y in 0..options.glyph_height as usize {
        for x in 0..options.glyph_width as usize {
            let mut sum = 0u32;
            for oy in 0..os {
                for ox in 0..os {
                    sum += canvas[(y * os + oy) * large_w + (x * os + ox)] as u32;
                }
            }
            let alpha = sum / (options.oversample * options.oversample);
            alpha_4bit.push(((alpha * 15 + 127) / 255) as u8);
        }
    }

    let packed = pack_4bpp_row_or_glyph(&alpha_4bit)?;
    let mut out = [0u8; GLYPH_BYTES];
    out.copy_from_slice(&packed);
    Ok(out)
}

#[cfg(feature = "generator")]
fn write_font_file(
    page_dir: &[u16],
    page_records: &[(u32, [u8; 32])],
    glyph_count: u32,
    fallback_index: u32,
    glyph_data: &[u8],
    options: &BitmapFontBuildOptions,
) -> Result<Vec<u8>, BitmapFontBuildError> {
    let page_dir_offset = HEADER_SIZE as u32;
    let page_record_offset = page_dir_offset + (PAGE_DIR_COUNT as u32 * 2);
    let glyph_data_offset =
        page_record_offset + (page_records.len() as u32 * PAGE_RECORD_SIZE as u32);
    let mut out = Vec::with_capacity(glyph_data_offset as usize + glyph_data.len());
    out.extend_from_slice(MAGIC);
    push_u16(&mut out, VERSION);
    push_u16(&mut out, options.glyph_width);
    push_u16(&mut out, options.glyph_height);
    push_u16(&mut out, options.bpp);
    push_u32(&mut out, PAGE_DIR_COUNT as u32);
    push_u32(&mut out, page_dir_offset);
    push_u32(&mut out, page_records.len() as u32);
    push_u32(&mut out, page_record_offset);
    push_u32(&mut out, glyph_count);
    push_u32(&mut out, glyph_data_offset);
    push_u32(&mut out, fallback_index);
    out.extend_from_slice(&[0; 32]);
    for &entry in page_dir {
        push_u16(&mut out, entry);
    }
    for (base, bitset) in page_records {
        push_u32(&mut out, *base);
        out.extend_from_slice(bitset);
    }
    out.extend_from_slice(glyph_data);
    Ok(out)
}

#[cfg(feature = "generator")]
fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(feature = "generator")]
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_font() -> Vec<u8> {
        let mut page_dir = vec![MISSING_PAGE_RECORD; PAGE_DIR_COUNT];
        page_dir[0] = 0;
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&GLYPH_WIDTH.to_le_bytes());
        out.extend_from_slice(&GLYPH_HEIGHT.to_le_bytes());
        out.extend_from_slice(&BPP.to_le_bytes());
        out.extend_from_slice(&(PAGE_DIR_COUNT as u32).to_le_bytes());
        out.extend_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&((HEADER_SIZE + PAGE_DIR_COUNT * 2) as u32).to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(
            &((HEADER_SIZE + PAGE_DIR_COUNT * 2 + PAGE_RECORD_SIZE) as u32).to_le_bytes(),
        );
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&[0; 32]);
        for entry in page_dir {
            out.extend_from_slice(&entry.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        let mut bitset = [0u8; 32];
        bitset[0x3f / 8] |= 1 << (0x3f % 8);
        bitset[0x41 / 8] |= 1 << (0x41 % 8);
        out.extend_from_slice(&bitset);
        out.extend_from_slice(&[0x11; GLYPH_BYTES]);
        out.extend_from_slice(&[0x22; GLYPH_BYTES]);
        out
    }

    #[test]
    fn header_magic_and_version_parse() {
        let bytes = minimal_font();
        let font = BitmapFont::parse(&bytes).unwrap();
        assert_eq!(font.fallback_index(), 0);
    }

    #[test]
    fn page_dir_count_is_full_unicode_page_count() {
        assert_eq!(PAGE_DIR_COUNT, 0x1100);
        let bytes = minimal_font();
        let font = BitmapFont::parse(&bytes).unwrap();
        assert_eq!(font.page_dir_count, 0x1100);
    }

    #[test]
    fn missing_page_returns_fallback() {
        let bytes = minimal_font();
        let font = BitmapFont::parse(&bytes).unwrap();
        assert_eq!(font.lookup_glyph_index(0x3042), font.fallback_index());
    }

    #[test]
    fn present_glyph_lookup_returns_expected_index() {
        let bytes = minimal_font();
        let font = BitmapFont::parse(&bytes).unwrap();
        assert_eq!(font.lookup_glyph_index(0x3f), 0);
        assert_eq!(font.lookup_glyph_index(0x41), 1);
    }

    #[test]
    fn rank_before_counts_bits_before_low() {
        let mut bitset = [0u8; 32];
        bitset[0] = 0b1011;
        assert_eq!(rank_before(&bitset, 0), 0);
        assert_eq!(rank_before(&bitset, 1), 1);
        assert_eq!(rank_before(&bitset, 3), 2);
        assert_eq!(rank_before(&bitset, 4), 3);
    }

    #[cfg(feature = "generator")]
    #[test]
    fn nibble_packing_orders_high_low() {
        let packed = pack_4bpp_row_or_glyph(&[0, 15, 15, 0]).unwrap();
        assert_eq!(packed, vec![0x0f, 0xf0]);
    }

    #[test]
    fn glyph_data_checks_bounds() {
        let bytes = minimal_font();
        let font = BitmapFont::parse(&bytes).unwrap();
        assert_eq!(font.glyph_data(0).unwrap().len(), GLYPH_BYTES);
        assert!(font.glyph_data(2).is_none());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn owned_font_from_bytes_and_lookup() {
        let font = OwnedBitmapFont::from_bytes(minimal_font()).unwrap();
        assert_eq!(font.lookup_glyph_index(0x41), 1);
        assert_eq!(font.lookup_glyph_index(0x3042), font.fallback_index());
        assert_eq!(font.lookup_glyph(0x41).data.len(), GLYPH_BYTES);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn owned_font_rejects_invalid_bytes() {
        assert!(OwnedBitmapFont::from_bytes(Vec::new()).is_err());
    }

    #[cfg(feature = "generator")]
    #[test]
    fn generated_font_parses_and_looks_up_ascii() {
        let ttf = include_bytes!("../../rfvp-rebuilder/assets/fonts/BIZUDGothic-Regular.ttf");
        let options = BitmapFontBuildOptions {
            fallback_codepoint: 0x3f,
            ..Default::default()
        };
        let bytes = build_bitmap_font_from_codepoints(ttf, &[0x3f, 0x41], &options).unwrap();
        let font = BitmapFont::parse(&bytes).unwrap();
        assert_ne!(font.lookup_glyph_index(0x41), font.fallback_index());
        assert_eq!(font.lookup_glyph_index(0x3042), font.fallback_index());
    }
}

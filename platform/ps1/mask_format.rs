//! PS1-only, offline Bayer coverage and a versioned NVSG mask trailer.
//! The original Type 3 zlib alpha plane remains byte-for-byte readable.

pub const NATIVE_FORMAT: u16 = 0x1303;
pub const HEADER_SIZE: usize = 16;
pub const CLUT_HEADER_SIZE: usize = 516;
pub const FOOTER_MAGIC: &[u8; 4] = b"M8PS";
pub const HEADER_MAGIC: &[u8; 4] = b"PSM8";
pub const PALETTE: [u16; 3] = [0x0000, 0xffff, 0x7fff];

const BAYER: [[u8; 8]; 8] = [
    [0, 48, 12, 60, 3, 51, 15, 63],
    [32, 16, 44, 28, 35, 19, 47, 31],
    [8, 56, 4, 52, 11, 59, 7, 55],
    [40, 24, 36, 20, 43, 27, 39, 23],
    [2, 50, 14, 62, 1, 49, 13, 61],
    [34, 18, 46, 30, 33, 17, 45, 29],
    [10, 58, 6, 54, 9, 57, 5, 53],
    [42, 26, 38, 22, 41, 25, 37, 21],
];

/// Quantize coverage to 0, 1/2 or 1. Called only by the offline converter.
pub fn alpha_state(alpha: u8, x: usize, y: usize) -> u8 {
    let scaled = u32::from(alpha) * 2;
    let base = scaled / 255;
    let remainder = scaled % 255;
    let threshold = u32::from(BAYER[y & 7][x & 7]) * 2 + 1;
    (base + u32::from(remainder * 128 > threshold * 255)) as u8
}

/// Offline coverage threshold. Index zero is reserved for transparent DMA padding.
/// The default palette (phase 255) reproduces alpha_state exactly. Retaining the
/// threshold also allows dissolves to advance using palette changes alone.
pub fn coverage_index(alpha: u8, x: usize, y: usize) -> u8 {
    let rank = u32::from(BAYER[y & 7][x & 7]);
    let numerator = u32::from(alpha) * 256 + (127 - rank * 2) * 255;
    (1 + numerator * 84 / (255 * 128)) as u8
}

/// A mask reveal ramps effective alpha from 0 through the source alpha to 255.
/// Only this 256-entry CLUT is changed at runtime, never the pixel plane.
pub fn palette_state(index: u8, phase: u16) -> u8 {
    if index == 0 {
        return 0;
    }
    let phase = i32::from(phase.min(510));
    let ceil_div = |value: i32| {
        if value > 0 {
            (value + 254) / 255
        } else {
            value / 255
        }
    };
    let low = ceil_div(255 + 84 * (765 - 2 * phase));
    let high = ceil_div(255 + 84 * (1020 - 2 * phase));
    if i32::from(index) < low {
        0
    } else if i32::from(index) < high {
        1
    } else {
        2
    }
}

pub struct MaskPlane<'a> {
    pub width: u16,
    pub height: u16,
    pub frames: u32,
    pub stride: u16,
    data: &'a [u8],
    frame_bytes: usize,
}

impl<'a> MaskPlane<'a> {
    pub fn frame(&self, index: usize) -> Option<&'a [u8]> {
        if index >= self.frames as usize {
            return None;
        }
        self.data
            .get(index * self.frame_bytes..(index + 1) * self.frame_bytes)
    }
}

/// Legacy masks have no trailer and retain their old rendering path.
pub fn read(bytes: &[u8]) -> Result<Option<MaskPlane<'_>>, &'static str> {
    if bytes.len() < 44
        || &bytes[..4] != b"hzc1"
        || &bytes[12..16] != b"NVSG"
        || bytes[18..20] != [3, 0]
        || !bytes.ends_with(FOOTER_MAGIC)
    {
        return Ok(None);
    }
    let u16_at = |data: &[u8], at| u16::from_le_bytes([data[at], data[at + 1]]);
    let u32_at = |data: &[u8], at| u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
    let length = u32_at(bytes, bytes.len() - 8) as usize;
    let start = bytes
        .len()
        .checked_sub(8)
        .and_then(|end| end.checked_sub(length))
        .ok_or("invalid PS1 mask trailer length")?;
    let nvsg_header = u32_at(bytes, 8) as usize;
    if nvsg_header < 32
        || start
            < 12usize
                .checked_add(nvsg_header)
                .ok_or("invalid NVSG header")?
    {
        return Err("PS1 mask trailer overlaps NVSG data");
    }
    let payload = &bytes[start..bytes.len() - 8];
    if payload.len() < HEADER_SIZE || &payload[..4] != HEADER_MAGIC || payload[14..16] != [0, 0] {
        return Err("unsupported PS1 mask trailer");
    }
    let width = u16_at(payload, 4);
    let height = u16_at(payload, 6);
    let frames = u32_at(payload, 8);
    let stride = u16_at(payload, 12);
    if width == 0
        || height == 0
        || width != u16_at(bytes, 20)
        || height != u16_at(bytes, 22)
        || frames != u32_at(bytes, 32).max(1)
        || u32::from(stride) != ((u32::from(width) + 1) & !1)
    {
        return Err("PS1 mask dimensions do not match NVSG");
    }
    let frame_bytes = usize::from(stride)
        .checked_mul(usize::from(height))
        .and_then(|n| n.checked_add(CLUT_HEADER_SIZE))
        .ok_or("PS1 mask frame too large")?;
    let data = &payload[HEADER_SIZE..];
    if frame_bytes.checked_mul(frames as usize) != Some(data.len()) {
        return Err("invalid PS1 mask frame length");
    }
    for frame in data.chunks_exact(frame_bytes) {
        if u16_at(frame, 0) != 256 || u16_at(frame, 2) != stride {
            return Err("invalid PS1 mask CLUT header");
        }
        for i in 0..256 {
            if u16_at(frame, 4 + i * 2) != PALETTE[palette_state(i as u8, 255) as usize] {
                return Err("invalid PS1 mask palette");
            }
        }
        if frame[CLUT_HEADER_SIZE..].iter().any(|b| *b > 252) {
            return Err("invalid PS1 mask coverage threshold");
        }
    }
    Ok(Some(MaskPlane {
        width,
        height,
        frames,
        stride,
        data,
        frame_bytes,
    }))
}

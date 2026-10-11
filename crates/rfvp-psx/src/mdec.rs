//! MDEC frames: pictures coded as the PlayStation's STR movies carry them (bitstream version 2).
//!
//! A frame is 16x16 macroblocks in columns (top to bottom, then left to right), each six 8x8
//! blocks: Cr, Cb, then the four luma blocks. A block is its DCT, quantized with the MDEC's
//! table (MPEG-1's intra table with 2 for DC) and the frame's scale, in zigzag order: the DC
//! value as 10 raw bits, then run/level codes (MPEG-1's AC table, escape `000001` + run + level,
//! end of block `10`). Bits are read most significant first from 16-bit little-endian words.
//! The frame starts with an 8-byte header: u16 MDEC code count / 2, u16 0x3800, u16 scale,
//! u16 version (2); a DC of 0x1ff ends it.
//!
//! The run/level codes are taken from PSn00bSDK's decoder table, so the two cannot disagree.

use std::sync::OnceLock;

/// The MDEC quantization table, in zigzag order (PSn00bSDK's default).
const QUANT: [u32; 64] = [
    2, 16, 16, 19, 16, 19, 22, 22, 22, 22, 22, 22, 26, 24, 26, 27, 27, 27, 26, 26, 26, 26, 27, 27, 27,
    29, 29, 29, 34, 34, 34, 29, 29, 29, 27, 27, 29, 29, 32, 32, 34, 34, 37, 38, 37, 35, 35, 34, 35,
    38, 38, 40, 40, 40, 48, 48, 46, 46, 56, 56, 58, 69, 69, 83,
];

/// Natural (row-major) position of each zigzag index.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// PSn00bSDK's VLC lookup table (libpsn00b/psxpress/vlc2.c), run-length packed: the repeat
/// count minus one in the top 11 bits, then (code length << 16 | MDEC value).
const PACKED_TABLE: [u32; 226] = [
    0x03e00000, 0x000d000b, 0x000d03f5, 0x000d2002, 0x000d23fe, 0x000d1003, 0x000d13fd, 0x000d000a,
    0x000d03f6, 0x000d0804, 0x000d0bfc, 0x000d1c02, 0x000d1ffe, 0x000d5402, 0x000d57fe, 0x000d5001,
    0x000d53ff, 0x000d0009, 0x000d03f7, 0x000d4c01, 0x000d4fff, 0x000d4801, 0x000d4bff, 0x000d0405,
    0x000d07fb, 0x000d0c03, 0x000d0ffd, 0x000d0008, 0x000d03f8, 0x000d1802, 0x000d1bfe, 0x000d4401,
    0x000d47ff, 0x006b4001, 0x006b43ff, 0x006b1402, 0x006b17fe, 0x006b0007, 0x006b03f9, 0x006b0803,
    0x006b0bfd, 0x006b0404, 0x006b07fc, 0x006b3c01, 0x006b3fff, 0x006b3801, 0x006b3bff, 0x006b1002,
    0x006b13fe, 0x0fe00000, 0x03e80802, 0x03e80bfe, 0x03e82401, 0x03e827ff, 0x03e80004, 0x03e803fc,
    0x03e82001, 0x03e823ff, 0x07e71c01, 0x07e71fff, 0x07e71801, 0x07e71bff, 0x07e70402, 0x07e707fe,
    0x07e71401, 0x07e717ff, 0x01e93401, 0x01e937ff, 0x01e90006, 0x01e903fa, 0x01e93001, 0x01e933ff,
    0x01e92c01, 0x01e92fff, 0x01e90c02, 0x01e90ffe, 0x01e90403, 0x01e907fd, 0x01e90005, 0x01e903fb,
    0x01e92801, 0x01e92bff, 0x0fe60003, 0x0fe603fd, 0x0fe61001, 0x0fe613ff, 0x0fe60c01, 0x0fe60fff,
    0x1fe50002, 0x1fe503fe, 0x1fe50801, 0x1fe50bff, 0x3fe40401, 0x3fe407ff, 0xffe2fe00, 0x7fe30001,
    0x7fe303ff, 0x03e00000, 0x00110412, 0x001107ee, 0x00110411, 0x001107ef, 0x00110410, 0x001107f0,
    0x0011040f, 0x001107f1, 0x00111803, 0x00111bfd, 0x00114002, 0x001143fe, 0x00113c02, 0x00113ffe,
    0x00113802, 0x00113bfe, 0x00113402, 0x001137fe, 0x00113002, 0x001133fe, 0x00112c02, 0x00112ffe,
    0x00117c01, 0x00117fff, 0x00117801, 0x00117bff, 0x00117401, 0x001177ff, 0x00117001, 0x001173ff,
    0x00116c01, 0x00116fff, 0x00300028, 0x003003d8, 0x00300027, 0x003003d9, 0x00300026, 0x003003da,
    0x00300025, 0x003003db, 0x00300024, 0x003003dc, 0x00300023, 0x003003dd, 0x00300022, 0x003003de,
    0x00300021, 0x003003df, 0x00300020, 0x003003e0, 0x0030040e, 0x003007f2, 0x0030040d, 0x003007f3,
    0x0030040c, 0x003007f4, 0x0030040b, 0x003007f5, 0x0030040a, 0x003007f6, 0x00300409, 0x003007f7,
    0x00300408, 0x003007f8, 0x006f001f, 0x006f03e1, 0x006f001e, 0x006f03e2, 0x006f001d, 0x006f03e3,
    0x006f001c, 0x006f03e4, 0x006f001b, 0x006f03e5, 0x006f001a, 0x006f03e6, 0x006f0019, 0x006f03e7,
    0x006f0018, 0x006f03e8, 0x006f0017, 0x006f03e9, 0x006f0016, 0x006f03ea, 0x006f0015, 0x006f03eb,
    0x006f0014, 0x006f03ec, 0x006f0013, 0x006f03ed, 0x006f0012, 0x006f03ee, 0x006f0011, 0x006f03ef,
    0x006f0010, 0x006f03f0, 0x00ee2802, 0x00ee2bfe, 0x00ee2402, 0x00ee27fe, 0x00ee1403, 0x00ee17fd,
    0x00ee0c04, 0x00ee0ffc, 0x00ee0805, 0x00ee0bfb, 0x00ee0407, 0x00ee07f9, 0x00ee0406, 0x00ee07fa,
    0x00ee000f, 0x00ee03f1, 0x00ee000e, 0x00ee03f2, 0x00ee000d, 0x00ee03f3, 0x00ee000c, 0x00ee03f4,
    0x00ee6801, 0x00ee6bff, 0x00ee6401, 0x00ee67ff, 0x00ee6001, 0x00ee63ff, 0x00ee5c01, 0x00ee5fff,
    0x00ee5801, 0x00ee5bff,
];

/// (run, level) for level 1..=40 -> (code bits, length), sign bit (0) included.
fn codes() -> &'static std::collections::HashMap<(u32, u32), (u32, u32)> {
    static CODES: OnceLock<std::collections::HashMap<(u32, u32), (u32, u32)>> = OnceLock::new();
    CODES.get_or_init(|| {
        let mut expanded = Vec::with_capacity(8192 + 512);
        for &packed in &PACKED_TABLE {
            for _ in 0..=(packed >> 21) {
                expanded.push(packed & 0x001f_ffff);
            }
        }
        assert_eq!(expanded.len(), 8192 + 512, "VLC table size");
        let mut map = std::collections::HashMap::new();
        // The first table is indexed by 13 bits (codes not starting with eight zeros), the
        // second by 17 (codes that do).
        for (table, bits) in [(&expanded[..8192], 13u32), (&expanded[8192..], 17u32)] {
            for (index, &value) in table.iter().enumerate() {
                let len = value >> 16;
                let mdec = value & 0xffff;
                if len == 0 || len > bits || mdec == 0xfe00 {
                    continue;
                }
                let level = (mdec & 0x3ff) as i32;
                let level = if level >= 0x200 { level - 0x400 } else { level };
                if level <= 0 {
                    continue;
                }
                let code = index as u32 >> (bits - len);
                map.entry((mdec >> 10, level as u32)).or_insert((code, len));
            }
        }
        map
    })
}

struct BitWriter {
    words: Vec<u16>,
    acc: u32,
    used: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self { words: Vec::new(), acc: 0, used: 0 }
    }

    fn put(&mut self, value: u32, len: u32) {
        for i in (0..len).rev() {
            self.acc = (self.acc << 1) | ((value >> i) & 1);
            self.used += 1;
            if self.used == 16 {
                self.words.push(self.acc as u16);
                self.acc = 0;
                self.used = 0;
            }
        }
    }

    fn finish(mut self) -> Vec<u16> {
        if self.used > 0 {
            self.words.push((self.acc << (16 - self.used)) as u16);
        }
        self.words
    }
}

/// The 8x8 DCT (orthonormal, as MPEG's) of a block of signed samples.
fn dct(block: &[f32; 64]) -> [f32; 64] {
    static COS: OnceLock<[[f32; 8]; 8]> = OnceLock::new();
    let cos = COS.get_or_init(|| {
        let mut t = [[0f32; 8]; 8];
        for (u, row) in t.iter_mut().enumerate() {
            let c = if u == 0 { (0.125f32).sqrt() } else { 0.5 };
            for (x, v) in row.iter_mut().enumerate() {
                *v = c * (((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI) / 16.0).cos();
            }
        }
        t
    });
    let mut tmp = [0f32; 64];
    for y in 0..8 {
        for u in 0..8 {
            tmp[y * 8 + u] = (0..8).map(|x| cos[u][x] * block[y * 8 + x]).sum();
        }
    }
    let mut out = [0f32; 64];
    for v in 0..8 {
        for u in 0..8 {
            out[v * 8 + u] = (0..8).map(|y| cos[v][y] * tmp[y * 8 + u]).sum();
        }
    }
    out
}

/// Quantized coefficients of one block, in zigzag order.
fn quantize(block: &[f32; 64], scale: u32) -> [i32; 64] {
    let coeffs = dct(block);
    let mut out = [0i32; 64];
    // DC: dequantized as level * 2, and the IDCT's DC is 8 times the mean.
    out[0] = (coeffs[0] / 2.0).round().clamp(-512.0, 510.0) as i32;
    for k in 1..64 {
        let q = (QUANT[k] * scale) as f32;
        out[k] = (coeffs[ZIGZAG[k]] * 8.0 / q).round().clamp(-511.0, 511.0) as i32;
    }
    out
}

fn put_block(w: &mut BitWriter, coeffs: &[i32; 64], codes_count: &mut usize) {
    w.put((coeffs[0] & 0x3ff) as u32, 10);
    *codes_count += 1;
    let mut run = 0u32;
    for &level in &coeffs[1..] {
        if level == 0 {
            run += 1;
            continue;
        }
        match codes().get(&(run, level.unsigned_abs())) {
            Some(&(code, len)) => w.put(code | (level < 0) as u32, len),
            None => {
                w.put(0b000001, 6);
                w.put(run, 6);
                w.put((level & 0x3ff) as u32, 10);
            }
        }
        *codes_count += 1;
        run = 0;
    }
    w.put(0b10, 2);
    *codes_count += 1;
}

/// YCbCr planes of a frame: luma minus 128, chroma averaged over 2x2.
pub struct Planes {
    pub width: usize,
    pub height: usize,
    y: Vec<f32>,
    cb: Vec<f32>,
    cr: Vec<f32>,
}

impl Planes {
    pub fn from_rgb(rgb: &[u8], width: usize, height: usize) -> Self {
        let mut y = vec![0f32; width * height];
        let (cw, ch) = (width / 2, height / 2);
        let mut cb = vec![0f32; cw * ch];
        let mut cr = vec![0f32; cw * ch];
        for py in 0..height {
            for px in 0..width {
                let p = &rgb[(py * width + px) * 3..];
                let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
                y[py * width + px] = 0.299 * r + 0.587 * g + 0.114 * b - 128.0;
                let ci = (py / 2) * cw + px / 2;
                cb[ci] += (-0.168_736 * r - 0.331_264 * g + 0.5 * b) / 4.0;
                cr[ci] += (0.5 * r - 0.418_688 * g - 0.081_312 * b) / 4.0;
            }
        }
        Self { width, height, y, cb, cr }
    }

    fn block(plane: &[f32], stride: usize, x0: usize, y0: usize) -> [f32; 64] {
        let mut b = [0f32; 64];
        for y in 0..8 {
            for x in 0..8 {
                b[y * 8 + x] = plane[(y0 + y) * stride + x0 + x];
            }
        }
        b
    }
}

/// Code a frame (its size a multiple of 16) at quantization `scale` (1..=63).
pub fn encode(planes: &Planes, scale: u32) -> Vec<u8> {
    let mut w = BitWriter::new();
    let mut count = 0usize;
    let (mbw, mbh) = (planes.width / 16, planes.height / 16);
    let total = mbw * mbh;
    let mut n = 0;
    for mx in 0..mbw {
        for my in 0..mbh {
            n += 1;
            let cw = planes.width / 2;
            let mut blocks = [
                quantize(&Planes::block(&planes.cr, cw, mx * 8, my * 8), scale),
                quantize(&Planes::block(&planes.cb, cw, mx * 8, my * 8), scale),
                quantize(&Planes::block(&planes.y, planes.width, mx * 16, my * 16), scale),
                quantize(&Planes::block(&planes.y, planes.width, mx * 16 + 8, my * 16), scale),
                quantize(&Planes::block(&planes.y, planes.width, mx * 16, my * 16 + 8), scale),
                quantize(&Planes::block(&planes.y, planes.width, mx * 16 + 8, my * 16 + 8), scale),
            ];
            if n == total {
                // The decoder takes the code count in pairs: make it even with one invisible
                // coefficient at the very end of the last block.
                let used: usize = blocks.iter().map(|b| 2 + b[1..].iter().filter(|&&c| c != 0).count()).sum();
                if (count + used) % 2 == 1 {
                    let last = &mut blocks[5][63];
                    *last = if *last == 0 { 1 } else { *last };
                    let used2: usize =
                        blocks.iter().map(|b| 2 + b[1..].iter().filter(|&&c| c != 0).count()).sum();
                    if (count + used2) % 2 == 1 {
                        blocks[5][62] = if blocks[5][62] == 0 { 1 } else { 0 };
                    }
                }
            }
            for b in &blocks {
                put_block(&mut w, b, &mut count);
            }
        }
    }
    // End of frame.
    w.put(0x1ff, 10);
    let words = w.finish();
    let mut out = Vec::with_capacity(8 + words.len() * 2 + 4);
    out.extend_from_slice(&((count / 2) as u16).to_le_bytes());
    out.extend_from_slice(&0x3800u16.to_le_bytes());
    out.extend_from_slice(&(scale as u16).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    for word in words {
        out.extend_from_slice(&word.to_le_bytes());
    }
    // Whole 32-bit words: the decoder reads them two halves at a time.
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

/// The smallest scale whose frame fits in `budget` bytes (63 if none does).
pub fn encode_within(planes: &Planes, budget: usize) -> Vec<u8> {
    let (mut lo, mut hi) = (1u32, 63u32);
    let mut best = encode(planes, hi);
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let frame = encode(planes, mid);
        if frame.len() <= budget {
            best = frame;
            if mid == 1 {
                break;
            }
            hi = mid - 1;
        } else {
            lo = mid + 1;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_the_common_codes() {
        let c = codes();
        assert_eq!(c.get(&(0, 1)), Some(&(0b110, 3)), "run 0 level 1 is 11s");
        assert_eq!(c.get(&(1, 1)), Some(&(0b0110, 4)), "run 1 level 1 is 011s");
        assert_eq!(c.get(&(0, 2)), Some(&(0b01000, 5)), "run 0 level 2 is 0100s");
    }

    #[test]
    fn flat_frame_is_small_and_even() {
        let rgb = vec![128u8; 32 * 16 * 3];
        let planes = Planes::from_rgb(&rgb, 32, 16);
        let frame = encode(&planes, 1);
        let count = u16::from_le_bytes([frame[0], frame[1]]) as usize * 2;
        // Two macroblocks of six blocks, each DC + EOB (+ the parity fix).
        assert!(count >= 24 && count <= 26, "{count}");
        assert_eq!(&frame[2..4], &0x3800u16.to_le_bytes());
        assert_eq!(frame.len() % 4, 0);
    }
}

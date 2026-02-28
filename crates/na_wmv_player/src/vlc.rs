/// VC-1 / WMV9 VLC Tables
///
/// Tables derived from SMPTE 421M Annex E, matching upstream's vc1data.h.
///
/// Each AC-coefficient VLC entry packs (run, |level|, last_flag) as:
///   bits[13..8] = run,  bits[7..1] = level,  bit[0] = last
///
/// The escape sentinel is -1 (caller then reads fixed-length escape).

use crate::bitreader::BitReader;

// ─── Generic VLC engine ─────────────────────────────────────────────────────

/// Flat decode table built from Huffman codes.
pub struct VlcTable {
    entries:          Vec<(i32, u8)>,  // (symbol, code_len)
    max_bits:         u8,
    /// max_level[run] for last=false/true — used in escape Mode 1.
    max_level_nolast: [u8; 64],
    max_level_last:   [u8; 64],
    /// max_run[level] for last=false/true — used in escape Mode 2.
    max_run_nolast:   [u8; 64],
    max_run_last:     [u8; 64],
}

impl VlcTable {
    /// Build from `(code_bits, code_len, symbol)` triples.
    pub fn build(codes: &[(u32, u8, i32)], max_bits: u8) -> Self {
        let size = 1usize << max_bits;
        let mut entries = vec![(i32::MIN, 0u8); size];
        for &(bits, len, sym) in codes {
            debug_assert!(len <= max_bits);
            let prefix = bits << (max_bits - len);
            let spread = 1usize << (max_bits - len);
            for i in 0..spread {
                entries[prefix as usize + i] = (sym, len);
            }
        }
        // Precompute max_level / max_run for escape Mode 1 & 2.
        let mut max_level_nolast = [0u8; 64];
        let mut max_level_last   = [0u8; 64];
        let mut max_run_nolast   = [0u8; 64];
        let mut max_run_last     = [0u8; 64];
        for &(_bits, _len, sym) in codes {
            if sym == VLC_ESCAPE || sym < 0 { continue; }
            let (run, level, last) = unpack_rl(sym);
            let run   = run   as usize;
            let level = level as usize;
            if last {
                if run   < 64 { max_level_last[run]   = max_level_last[run].max(level as u8); }
                if level < 64 { max_run_last[level]   = max_run_last[level].max(run as u8); }
            } else {
                if run   < 64 { max_level_nolast[run] = max_level_nolast[run].max(level as u8); }
                if level < 64 { max_run_nolast[level] = max_run_nolast[level].max(run as u8); }
            }
        }
        VlcTable {
            entries, max_bits,
            max_level_nolast, max_level_last,
            max_run_nolast,   max_run_last,
        }
    }

    /// Decode one symbol.  Advances the reader by the code length.
    #[inline]
    pub fn decode(&self, br: &mut BitReader<'_>) -> Option<i32> {
        let peek = br.peek_bits(self.max_bits)?;
        let (sym, len) = self.entries[peek as usize];
        if len == 0 { return None; }
        br.skip_bits(len);
        Some(sym)
    }

    pub fn max_bits(&self) -> u8 { self.max_bits }

    /// Max level coded for given (run, last) in this table.
    #[inline] pub fn max_level(&self, run: usize, last: bool) -> u8 {
        let r = run.min(63);
        if last { self.max_level_last[r] } else { self.max_level_nolast[r] }
    }
    /// Max run coded for given (level, last) in this table.
    #[inline] pub fn max_run(&self, level: usize, last: bool) -> u8 {
        let l = level.min(63);
        if last { self.max_run_last[l] } else { self.max_run_nolast[l] }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Escape sentinel returned by VLC decode when fixed-length escape follows.
pub const VLC_ESCAPE: i32 = -1;

/// Pack (run, level_magnitude, last) into VLC symbol.
#[inline(always)]
pub const fn pack_rl(run: u8, level: u8, last: bool) -> i32 {
    ((run as i32) << 8) | ((level as i32) << 1) | (last as i32)
}

/// Unpack VLC symbol into (run, level_magnitude, last).
#[inline(always)]
pub fn unpack_rl(v: i32) -> (u8, u8, bool) {
    (((v >> 8) & 0x3F) as u8,
     ((v >> 1) & 0x7F) as u8,
     v & 1 != 0)
}

// ─── DC size VLC tables ──────────────────────────────────────────────────────
// SMPTE 421M §8.1.4.4 Tables 36-37.
// Symbol = number of bits that follow encoding the DC differential.

pub fn dc_luma_vlc() -> VlcTable {
    // Table 36 – DC luma (Simple/Main)
    const T: &[(u32, u8, i32)] = &[
        (0b1111111,  7,  0),
        (0b00,       2,  1),
        (0b01,       2,  2),
        (0b100,      3,  3),
        (0b1010,     4,  4),
        (0b10110,    5,  5),
        (0b101110,   6,  6),
        (0b1011110,  7,  7),
        (0b11110,    5,  8),
        (0b111110,   6,  9),
        (0b1111100,  7, 10),
        (0b1111101,  7, 11),  // dc_size=11 (rarely used)
    ];
    VlcTable::build(T, 7)
}

pub fn dc_chroma_vlc() -> VlcTable {
    // Table 37 – DC chroma (Simple/Main)
    const T: &[(u32, u8, i32)] = &[
        (0b00,        2,  0),
        (0b01,        2,  1),
        (0b10,        2,  2),
        (0b110,       3,  3),
        (0b1110,      4,  4),
        (0b11110,     5,  5),
        (0b111110,    6,  6),
        (0b1111110,   7,  7),
        (0b11111110,  8,  8),
        (0b11111111,  8, VLC_ESCAPE),
    ];
    VlcTable::build(T, 8)
}

// ─── AC coefficient VLC tables ───────────────────────────────────────────────
// SMPTE 421M Annex E Tables 59-66.
//
// 4 inter tables (TRANSACFRM 0-3) + 4 intra tables (TRANSACFRM2 0-3).
// All codes are canonical Huffman assignments derived from the spec code lengths.
//
// Table selection is done by the sequence header fields:
//   TRANSACFRM  → inter_tcoef_vlc(idx)
//   TRANSACFRM2 → intra_tcoef_vlc(idx)

const INTER_CODES_0: &[(u32, u8, i32)] = &[
    (0b00,  2, pack_rl( 0, 1,false)),
    (0b0100,  4, pack_rl( 0, 2,false)),
    (0b0101,  4, pack_rl( 0, 3,false)),
    (0b0110,  4, pack_rl( 0, 2,true )),
    (0b0111,  4, pack_rl( 0, 3,true )),
    (0b10000,  5, pack_rl( 0, 4,false)),
    (0b10001,  5, pack_rl( 1, 1,false)),
    (0b10010,  5, pack_rl( 0, 1,true )),
    (0b100110,  6, pack_rl( 0, 5,false)),
    (0b100111,  6, pack_rl( 0, 6,false)),
    (0b101000,  6, pack_rl( 0, 7,false)),
    (0b101001,  6, pack_rl( 0, 8,false)),
    (0b101010,  6, pack_rl( 2, 1,false)),
    (0b1010110,  7, pack_rl( 0, 9,false)),
    (0b1010111,  7, pack_rl( 0,10,false)),
    (0b1011000,  7, pack_rl( 0,11,false)),
    (0b1011001,  7, pack_rl( 0,12,false)),
    (0b1011010,  7, pack_rl( 1, 2,false)),
    (0b1011011,  7, pack_rl( 1, 3,false)),
    (0b1011100,  7, pack_rl( 3, 1,false)),
    (0b1011101,  7, pack_rl( 4, 1,false)),
    (0b1011110,  7, pack_rl( 1, 1,true )),
    (0b1011111,  7, pack_rl( 2, 1,true )),
    (0b1100000,  7, pack_rl( 3, 1,true )),
    (0b1100001,  7, pack_rl( 4, 1,true )),
    (0b11000100,  8, pack_rl( 0,13,false)),
    (0b11000101,  8, pack_rl( 0,14,false)),
    (0b11000110,  8, pack_rl( 0,15,false)),
    (0b11000111,  8, pack_rl( 1, 4,false)),
    (0b11001000,  8, pack_rl( 5, 1,false)),
    (0b11001001,  8, pack_rl( 6, 1,false)),
    (0b11001010,  8, pack_rl( 7, 1,false)),
    (0b11001011,  8, pack_rl( 5, 1,true )),
    (0b11001100,  8, pack_rl( 6, 1,true )),
    (0b11001101,  8, pack_rl( 7, 1,true )),
    (0b110011100,  9, pack_rl( 1, 5,false)),
    (0b110011101,  9, pack_rl( 2, 2,false)),
    (0b110011110,  9, VLC_ESCAPE),
];

const INTER_CODES_1: &[(u32, u8, i32)] = &[
    (0b00,  2, pack_rl( 0, 1,false)),
    (0b010,  3, pack_rl( 0, 2,false)),
    (0b0110,  4, pack_rl( 0, 3,false)),
    (0b0111,  4, pack_rl( 0, 1,true )),
    (0b10000,  5, pack_rl( 0, 4,false)),
    (0b10001,  5, pack_rl( 1, 1,false)),
    (0b10010,  5, pack_rl( 0, 2,true )),
    (0b100110,  6, pack_rl( 0, 5,false)),
    (0b100111,  6, pack_rl( 0, 6,false)),
    (0b101000,  6, pack_rl( 0, 7,false)),
    (0b101001,  6, pack_rl( 0, 8,false)),
    (0b101010,  6, pack_rl( 2, 1,false)),
    (0b1010110,  7, pack_rl( 0, 9,false)),
    (0b1010111,  7, pack_rl( 0,10,false)),
    (0b1011000,  7, pack_rl( 0,11,false)),
    (0b1011001,  7, pack_rl( 0,12,false)),
    (0b1011010,  7, pack_rl( 1, 2,false)),
    (0b1011011,  7, pack_rl( 1, 3,false)),
    (0b1011100,  7, pack_rl( 2, 2,false)),
    (0b1011101,  7, pack_rl( 3, 1,false)),
    (0b1011110,  7, pack_rl( 0, 3,true )),
    (0b1011111,  7, pack_rl( 1, 1,true )),
    (0b1100000,  7, pack_rl( 2, 1,true )),
    (0b1100001,  7, pack_rl( 3, 1,true )),
    (0b11000100,  8, pack_rl( 0,13,false)),
    (0b11000101,  8, pack_rl( 0,14,false)),
    (0b11000110,  8, pack_rl( 0,15,false)),
    (0b11000111,  8, pack_rl( 1, 4,false)),
    (0b11001000,  8, pack_rl( 4, 1,false)),
    (0b11001001,  8, pack_rl( 5, 1,false)),
    (0b11001010,  8, pack_rl( 6, 1,false)),
    (0b11001011,  8, pack_rl( 7, 1,false)),
    (0b11001100,  8, pack_rl( 4, 1,true )),
    (0b11001101,  8, pack_rl( 5, 1,true )),
    (0b11001110,  8, pack_rl( 6, 1,true )),
    (0b110011110,  9, pack_rl( 1, 5,false)),
    (0b110011111,  9, VLC_ESCAPE),
];

const INTER_CODES_2: &[(u32, u8, i32)] = &[
    (0b00,  2, pack_rl( 0, 1,false)),
    (0b010,  3, pack_rl( 0, 2,false)),
    (0b0110,  4, pack_rl( 0, 3,false)),
    (0b0111,  4, pack_rl( 0, 2,true )),
    (0b1000,  4, pack_rl( 0, 3,true )),
    (0b10010,  5, pack_rl( 0, 4,false)),
    (0b10011,  5, pack_rl( 1, 1,false)),
    (0b10100,  5, pack_rl( 0, 1,true )),
    (0b101010,  6, pack_rl( 0, 5,false)),
    (0b101011,  6, pack_rl( 0, 6,false)),
    (0b101100,  6, pack_rl( 0, 7,false)),
    (0b101101,  6, pack_rl( 0, 8,false)),
    (0b101110,  6, pack_rl( 2, 1,false)),
    (0b1011110,  7, pack_rl( 0, 9,false)),
    (0b1011111,  7, pack_rl( 0,10,false)),
    (0b1100000,  7, pack_rl( 0,11,false)),
    (0b1100001,  7, pack_rl( 0,12,false)),
    (0b1100010,  7, pack_rl( 1, 2,false)),
    (0b1100011,  7, pack_rl( 1, 3,false)),
    (0b1100100,  7, pack_rl( 3, 1,false)),
    (0b1100101,  7, pack_rl( 4, 1,false)),
    (0b1100110,  7, pack_rl( 1, 1,true )),
    (0b1100111,  7, pack_rl( 2, 1,true )),
    (0b1101000,  7, pack_rl( 3, 1,true )),
    (0b1101001,  7, pack_rl( 4, 1,true )),
    (0b11010100,  8, pack_rl( 0,13,false)),
    (0b11010101,  8, pack_rl( 0,14,false)),
    (0b11010110,  8, pack_rl( 1, 4,false)),
    (0b11010111,  8, pack_rl( 5, 1,false)),
    (0b11011000,  8, pack_rl( 6, 1,false)),
    (0b11011001,  8, pack_rl( 7, 1,false)),
    (0b11011010,  8, pack_rl( 8, 1,false)),
    (0b11011011,  8, pack_rl( 5, 1,true )),
    (0b11011100,  8, pack_rl( 6, 1,true )),
    (0b11011101,  8, pack_rl( 7, 1,true )),
    (0b110111100,  9, pack_rl( 1, 5,false)),
    (0b110111101,  9, pack_rl( 2, 2,false)),
    (0b110111110,  9, VLC_ESCAPE),
];

const INTER_CODES_3: &[(u32, u8, i32)] = &[
    (0b00,  2, pack_rl( 0, 1,false)),
    (0b010,  3, pack_rl( 0, 2,false)),
    (0b0110,  4, pack_rl( 0, 3,false)),
    (0b0111,  4, pack_rl( 0, 4,false)),
    (0b1000,  4, pack_rl( 0, 1,true )),
    (0b1001,  4, pack_rl( 0, 2,true )),
    (0b10100,  5, pack_rl( 0, 5,false)),
    (0b10101,  5, pack_rl( 0, 3,true )),
    (0b101100,  6, pack_rl( 0, 6,false)),
    (0b101101,  6, pack_rl( 0, 7,false)),
    (0b101110,  6, pack_rl( 0, 8,false)),
    (0b101111,  6, pack_rl( 0, 9,false)),
    (0b110000,  6, pack_rl( 1, 1,false)),
    (0b1100010,  7, pack_rl( 0,10,false)),
    (0b1100011,  7, pack_rl( 0,11,false)),
    (0b1100100,  7, pack_rl( 0,12,false)),
    (0b1100101,  7, pack_rl( 0,13,false)),
    (0b1100110,  7, pack_rl( 1, 2,false)),
    (0b1100111,  7, pack_rl( 1, 3,false)),
    (0b1101000,  7, pack_rl( 2, 1,false)),
    (0b1101001,  7, pack_rl( 3, 1,false)),
    (0b1101010,  7, pack_rl( 1, 1,true )),
    (0b1101011,  7, pack_rl( 2, 1,true )),
    (0b1101100,  7, pack_rl( 3, 1,true )),
    (0b1101101,  7, pack_rl( 4, 1,true )),
    (0b11011100,  8, pack_rl( 0,14,false)),
    (0b11011101,  8, pack_rl( 0,15,false)),
    (0b11011110,  8, pack_rl( 1, 4,false)),
    (0b11011111,  8, pack_rl( 4, 1,false)),
    (0b11100000,  8, pack_rl( 5, 1,false)),
    (0b11100001,  8, pack_rl( 6, 1,false)),
    (0b11100010,  8, pack_rl( 7, 1,false)),
    (0b11100011,  8, pack_rl( 5, 1,true )),
    (0b11100100,  8, pack_rl( 6, 1,true )),
    (0b11100101,  8, pack_rl( 7, 1,true )),
    (0b111001100,  9, pack_rl( 1, 5,false)),
    (0b111001101,  9, pack_rl( 2, 2,false)),
    (0b111001110,  9, VLC_ESCAPE),
];

const INTRA_CODES_0: &[(u32, u8, i32)] = &[
    (0b00,  2, pack_rl( 0, 1,false)),
    (0b0100,  4, pack_rl( 0, 2,false)),
    (0b0101,  4, pack_rl( 0, 3,false)),
    (0b0110,  4, pack_rl( 0, 1,true )),
    (0b0111,  4, pack_rl( 0, 2,true )),
    (0b10000,  5, pack_rl( 0, 4,false)),
    (0b10001,  5, pack_rl( 1, 3,false)),
    (0b100100,  6, pack_rl( 0, 5,false)),
    (0b100101,  6, pack_rl( 0, 6,false)),
    (0b100110,  6, pack_rl( 0, 7,false)),
    (0b100111,  6, pack_rl( 2, 1,false)),
    (0b1010000,  7, pack_rl( 0, 8,false)),
    (0b1010001,  7, pack_rl( 0, 9,false)),
    (0b1010010,  7, pack_rl( 0,10,false)),
    (0b1010011,  7, pack_rl( 0,11,false)),
    (0b1010100,  7, pack_rl( 0, 3,true )),
    (0b1010101,  7, pack_rl( 1, 1,true )),
    (0b1010110,  7, pack_rl( 2, 1,true )),
    (0b1010111,  7, pack_rl( 3, 1,true )),
    (0b10110000,  8, pack_rl( 0,12,false)),
    (0b10110001,  8, pack_rl( 1, 1,false)),
    (0b10110010,  8, pack_rl( 1, 2,false)),
    (0b101100110,  9, pack_rl( 2, 2,false)),
    (0b101100111,  9, pack_rl( 3, 1,false)),
    (0b101101000,  9, VLC_ESCAPE),
];

const INTRA_CODES_1: &[(u32, u8, i32)] = &[
    (0b00,  2, pack_rl( 0, 1,false)),
    (0b010,  3, pack_rl( 0, 2,false)),
    (0b0110,  4, pack_rl( 0, 3,false)),
    (0b0111,  4, pack_rl( 0, 1,true )),
    (0b1000,  4, pack_rl( 0, 2,true )),
    (0b10010,  5, pack_rl( 0, 4,false)),
    (0b10011,  5, pack_rl( 2, 1,false)),
    (0b10100,  5, pack_rl( 0, 3,true )),
    (0b101010,  6, pack_rl( 0, 5,false)),
    (0b101011,  6, pack_rl( 0, 6,false)),
    (0b101100,  6, pack_rl( 0, 7,false)),
    (0b101101,  6, pack_rl( 1, 1,false)),
    (0b101110,  6, pack_rl( 3, 1,false)),
    (0b1011110,  7, pack_rl( 0, 8,false)),
    (0b1011111,  7, pack_rl( 0, 9,false)),
    (0b1100000,  7, pack_rl( 0,10,false)),
    (0b1100001,  7, pack_rl( 0,11,false)),
    (0b1100010,  7, pack_rl( 1, 2,false)),
    (0b1100011,  7, pack_rl( 1, 3,false)),
    (0b1100100,  7, pack_rl( 1, 1,true )),
    (0b1100101,  7, pack_rl( 2, 1,true )),
    (0b11001100,  8, pack_rl( 0,12,false)),
    (0b11001101,  8, pack_rl( 1, 4,false)),
    (0b11001110,  8, pack_rl( 3, 1,true )),
    (0b11001111,  8, pack_rl( 4, 1,true )),
    (0b110100000,  9, pack_rl( 2, 2,false)),
    (0b110100001,  9, pack_rl( 4, 1,false)),
    (0b110100010,  9, VLC_ESCAPE),
];

const INTRA_CODES_2: &[(u32, u8, i32)] = &[
    (0b00,  2, pack_rl( 0, 1,false)),
    (0b010,  3, pack_rl( 0, 2,false)),
    (0b0110,  4, pack_rl( 0, 3,false)),
    (0b0111,  4, pack_rl( 0, 2,true )),
    (0b1000,  4, pack_rl( 0, 3,true )),
    (0b10010,  5, pack_rl( 0, 4,false)),
    (0b10011,  5, pack_rl( 2, 1,false)),
    (0b10100,  5, pack_rl( 0, 1,true )),
    (0b101010,  6, pack_rl( 0, 5,false)),
    (0b101011,  6, pack_rl( 0, 6,false)),
    (0b101100,  6, pack_rl( 0, 7,false)),
    (0b101101,  6, pack_rl( 1, 1,false)),
    (0b101110,  6, pack_rl( 3, 1,false)),
    (0b1011110,  7, pack_rl( 0, 8,false)),
    (0b1011111,  7, pack_rl( 0, 9,false)),
    (0b1100000,  7, pack_rl( 0,10,false)),
    (0b1100001,  7, pack_rl( 1, 2,false)),
    (0b1100010,  7, pack_rl( 2, 2,false)),
    (0b1100011,  7, pack_rl( 4, 1,false)),
    (0b1100100,  7, pack_rl( 1, 1,true )),
    (0b1100101,  7, pack_rl( 2, 1,true )),
    (0b1100110,  7, pack_rl( 3, 1,true )),
    (0b1100111,  7, pack_rl( 4, 1,true )),
    (0b11010000,  8, pack_rl( 0,11,false)),
    (0b11010001,  8, pack_rl( 1, 3,false)),
    (0b110100100,  9, VLC_ESCAPE),
];

const INTRA_CODES_3: &[(u32, u8, i32)] = &[
    (0b00,  2, pack_rl( 0, 1,false)),
    (0b010,  3, pack_rl( 0, 2,false)),
    (0b0110,  4, pack_rl( 0, 3,false)),
    (0b0111,  4, pack_rl( 0, 4,false)),
    (0b1000,  4, pack_rl( 0, 1,true )),
    (0b1001,  4, pack_rl( 0, 2,true )),
    (0b10100,  5, pack_rl( 0, 5,false)),
    (0b10101,  5, pack_rl( 2, 1,false)),
    (0b10110,  5, pack_rl( 0, 3,true )),
    (0b101110,  6, pack_rl( 0, 6,false)),
    (0b101111,  6, pack_rl( 0, 7,false)),
    (0b110000,  6, pack_rl( 0, 8,false)),
    (0b110001,  6, pack_rl( 1, 1,false)),
    (0b110010,  6, pack_rl( 2, 2,false)),
    (0b1100110,  7, pack_rl( 0, 9,false)),
    (0b1100111,  7, pack_rl( 0,10,false)),
    (0b1101000,  7, pack_rl( 0,11,false)),
    (0b1101001,  7, pack_rl( 1, 2,false)),
    (0b1101010,  7, pack_rl( 3, 1,false)),
    (0b1101011,  7, pack_rl( 4, 1,false)),
    (0b1101100,  7, pack_rl( 1, 1,true )),
    (0b1101101,  7, pack_rl( 2, 1,true )),
    (0b1101110,  7, pack_rl( 3, 1,true )),
    (0b1101111,  7, pack_rl( 4, 1,true )),
    (0b11100000,  8, pack_rl( 1, 3,false)),
    (0b111000010,  9, VLC_ESCAPE),
];

pub fn inter_tcoef_vlc(idx: usize) -> VlcTable {
    let codes = [INTER_CODES_0, INTER_CODES_1, INTER_CODES_2, INTER_CODES_3];
    VlcTable::build(codes[idx.min(3)], 9)
}

pub fn intra_tcoef_vlc(idx: usize) -> VlcTable {
    let codes = [INTRA_CODES_0, INTRA_CODES_1, INTRA_CODES_2, INTRA_CODES_3];
    VlcTable::build(codes[idx.min(3)], 9)
}

// ─── CBPCY VLC (Coded Block Pattern) ────────────────────────────────────────
// SMPTE 421M §8.1.5 Tables 54-55.
// Symbol = 6-bit CBP (Y0 Y1 Y2 Y3 Cb Cr = MSB..LSB).

// ─── Inter CBP VLC tables (CBPTAB 0-3) ──────────────────────────────────────
// SMPTE 421M §8.1.5 Tables 55a-55b (Simple/Main uses tables 0-1;
// CBPTAB 2-3 fall back to 0-1).
// Canonical Huffman codes derived from spec code lengths.

const CBP_INTER_0_CODES: &[(u32, u8, i32)] = &[
    (0b0,  1, 0),
    (0b1000,  4, 48),
    (0b1001,  4, 49),
    (0b10100,  5, 50),
    (0b10101,  5, 51),
    (0b101100,  6, 52),
    (0b101101,  6, 53),
    (0b101110,  6, 54),
    (0b1011110,  7, 25),
    (0b1011111,  7, 26),
    (0b1100000,  7, 27),
    (0b1100001,  7, 28),
    (0b1100010,  7, 29),
    (0b1100011,  7, 30),
    (0b1100100,  7, 31),
    (0b1100101,  7, 32),
    (0b1100110,  7, 33),
    (0b1100111,  7, 34),
    (0b1101000,  7, 55),
    (0b1101001,  7, 56),
    (0b11010100,  8, 35),
    (0b11010101,  8, 36),
    (0b11010110,  8, 37),
    (0b11010111,  8, 38),
    (0b11011000,  8, 39),
    (0b11011001,  8, 40),
    (0b11011010,  8, 41),
    (0b11011011,  8, 42),
    (0b11011100,  8, 43),
    (0b11011101,  8, 44),
    (0b11011110,  8, 45),
    (0b11011111,  8, 46),
    (0b11100000,  8, 47),
    (0b11100001,  8, 57),
    (0b11100010,  8, 58),
    (0b111000110,  9, 5),
    (0b111000111,  9, 6),
    (0b111001000,  9, 7),
    (0b111001001,  9, 8),
    (0b111001010,  9, 13),
    (0b111001011,  9, 14),
    (0b111001100,  9, 15),
    (0b111001101,  9, 16),
    (0b111001110,  9, 19),
    (0b111001111,  9, 20),
    (0b111010000,  9, 59),
    (0b111010001,  9, 60),
    (0b111010010,  9, 63),
    (0b1110100110, 10, 1),
    (0b1110100111, 10, 2),
    (0b1110101000, 10, 3),
    (0b1110101001, 10, 4),
    (0b1110101010, 10, 9),
    (0b1110101011, 10, 10),
    (0b1110101100, 10, 11),
    (0b1110101101, 10, 12),
    (0b1110101110, 10, 17),
    (0b1110101111, 10, 18),
    (0b1110110000, 10, 21),
    (0b1110110001, 10, 22),
    (0b1110110010, 10, 23),
    (0b1110110011, 10, 24),
    (0b1110110100, 10, 61),
    (0b1110110101, 10, 62),
];

const CBP_INTER_1_CODES: &[(u32, u8, i32)] = &[
    (0b0,  1, 0),
    (0b1000,  4, 48),
    (0b1001,  4, 49),
    (0b10100,  5, 50),
    (0b10101,  5, 51),
    (0b10110,  5, 52),
    (0b10111,  5, 53),
    (0b110000,  6, 54),
    (0b110001,  6, 55),
    (0b110010,  6, 56),
    (0b1100110,  7, 25),
    (0b1100111,  7, 26),
    (0b1101000,  7, 27),
    (0b1101001,  7, 28),
    (0b1101010,  7, 29),
    (0b1101011,  7, 30),
    (0b1101100,  7, 31),
    (0b1101101,  7, 32),
    (0b1101110,  7, 33),
    (0b1101111,  7, 34),
    (0b1110000,  7, 57),
    (0b1110001,  7, 58),
    (0b11100100,  8, 35),
    (0b11100101,  8, 36),
    (0b11100110,  8, 37),
    (0b11100111,  8, 38),
    (0b11101000,  8, 39),
    (0b11101001,  8, 40),
    (0b11101010,  8, 41),
    (0b11101011,  8, 42),
    (0b11101100,  8, 43),
    (0b11101101,  8, 44),
    (0b11101110,  8, 45),
    (0b11101111,  8, 46),
    (0b11110000,  8, 47),
    (0b11110001,  8, 59),
    (0b11110010,  8, 60),
    (0b111100110,  9, 3),
    (0b111100111,  9, 4),
    (0b111101000,  9, 9),
    (0b111101001,  9, 10),
    (0b111101010,  9, 13),
    (0b111101011,  9, 14),
    (0b111101100,  9, 15),
    (0b111101101,  9, 16),
    (0b111101110,  9, 17),
    (0b111101111,  9, 18),
    (0b111110000,  9, 19),
    (0b111110001,  9, 20),
    (0b111110010,  9, 61),
    (0b111110011,  9, 62),
    (0b1111101000, 10, 1),
    (0b1111101001, 10, 2),
    (0b1111101010, 10, 5),
    (0b1111101011, 10, 6),
    (0b1111101100, 10, 7),
    (0b1111101101, 10, 8),
    (0b1111101110, 10, 11),
    (0b1111101111, 10, 12),
    (0b1111110000, 10, 21),
    (0b1111110001, 10, 22),
    (0b1111110010, 10, 23),
    (0b1111110011, 10, 24),
    (0b1111110100, 10, 63),
];

pub fn cbpcy_p_vlc(idx: usize) -> VlcTable {
    // CBPTAB 0-1 use distinct tables; 2-3 reuse 0-1 (Simple/Main only)
    match idx & 1 {
        0 => VlcTable::build(CBP_INTER_0_CODES, 10),
        _ => VlcTable::build(CBP_INTER_1_CODES, 10),
    }
}

pub fn cbpcy_i_vlc() -> VlcTable {
    // Table 54 – Intra CBP (only coded-or-not matters for I-frames)
    const T: &[(u32, u8, i32)] = &[
        (0b0,         1, 63),
        (0b100,       3,  0),
        (0b101,       3, 32),
        (0b1100,      4, 16),
        (0b1101,      4,  8),
        (0b11100,     5,  4),
        (0b11101,     5,  2),
        (0b111100,    6,  1),
        (0b1111010,   7, 48),
        (0b1111011,   7, 40),
        (0b11111000,  8, 36),
        (0b11111001,  8, 24),
        (0b11111010,  8, 20),
        (0b11111011,  8, 12),
        (0b11111100,  8,  6),
        (0b11111101,  8,  5),
        (0b11111110,  8,  3),
        (0b111111110, 9, 60),
        (0b111111111, 9, 56),
    ];
    VlcTable::build(T, 9)
}

// ─── Transform type per MB / block ───────────────────────────────────────────
// SMPTE 421M Tables 56-57.
// 0=8x8, 1=8x4_top, 2=8x4_bot, 3=4x8_left, 4=4x8_right, 5=4x4, 6=per_block

pub fn ttmb_vlc() -> VlcTable {
    const T: &[(u32, u8, i32)] = &[
        (0b1,      1, 0),
        (0b01,     2, 6),
        (0b001,    3, 5),
        (0b0001,   4, 1),
        (0b00001,  5, 2),
        (0b000001, 6, 3),
        (0b000000, 6, 4),
    ];
    VlcTable::build(T, 6)
}

pub fn ttblk_vlc() -> VlcTable {
    const T: &[(u32, u8, i32)] = &[
        (0b1,      1, 0),
        (0b01,     2, 5),
        (0b001,    3, 1),
        (0b0001,   4, 2),
        (0b00001,  5, 3),
        (0b000001, 6, 4),
        (0b000000, 6, 6),
    ];
    VlcTable::build(T, 6)
}

pub fn subblkpat_vlc() -> VlcTable {
    const T: &[(u32, u8, i32)] = &[
        (0b1,    1, 3),
        (0b01,   2, 2),
        (0b001,  3, 1),
        (0b0001, 4, 0),
    ];
    VlcTable::build(T, 4)
}

// ─── MV differential VLC ─────────────────────────────────────────────────────
// SMPTE 421M §8.3.5.4 Table 46 (k=0) and Table 47 (k=1).
// Symbol = signed quarter-pixel MV differential.
// i32::MIN is the escape sentinel (fixed-length follows).

// ─── MV differential VLC tables (MVTAB 0-3) ─────────────────────────────────
// SMPTE 421M §8.3.5.4 Tables 46a-46d.
// Canonical Huffman from spec code lengths.
// i32::MIN = escape sentinel (fixed-length code follows).

const MV_CODES_0: &[(u32, u8, i32)] = &[
    (0b0,  1, 0),
    (0b100,  3, 1),
    (0b101,  3, -1),
    (0b11000,  5, 2),
    (0b11001,  5, -2),
    (0b110100,  6, 3),
    (0b110101,  6, -3),
    (0b1101100,  7, 4),
    (0b1101101,  7, -4),
    (0b11011100,  8, 5),
    (0b11011101,  8, -5),
    (0b110111100,  9, 6),
    (0b110111101,  9, -6),
    (0b1101111100, 10, 7),
    (0b1101111101, 10, -7),
    (0b1101111110, 10, i32::MIN),
];

const MV_CODES_1: &[(u32, u8, i32)] = &[
    (0b0,  1, 0),
    (0b100,  3, 1),
    (0b101,  3, -1),
    (0b11000,  5, 2),
    (0b11001,  5, -2),
    (0b110100,  6, 3),
    (0b110101,  6, -3),
    (0b110110,  6, 4),
    (0b110111,  6, -4),
    (0b1110000,  7, 5),
    (0b1110001,  7, -5),
    (0b11100100,  8, 6),
    (0b11100101,  8, -6),
    (0b111001100,  9, 7),
    (0b111001101,  9, -7),
    (0b111001110,  9, i32::MIN),
];

const MV_CODES_2: &[(u32, u8, i32)] = &[
    (0b0,  1, 0),
    (0b10,  2, 1),
    (0b11,  2, -1),
    // NOTE: these bit patterns are written MSB-first.
    // The original version had code lengths off by one, which made
    // `prefix = bits << (max_bits - len)` overflow the decode table.
    (0b10000,       5, 2),
    (0b10001,       5, -2),
    (0b100100,      6, 3),
    (0b100101,      6, -3),
    (0b1001100,     7, 4),
    (0b1001101,     7, -4),
    (0b10011100,    8, 5),
    (0b10011101,    8, -5),
    (0b100111100,   9, 6),
    (0b100111101,   9, -6),
    (0b1001111100, 10, 7),
    (0b1001111101, 10, -7),
    (0b1001111110, 10, i32::MIN),
];

const MV_CODES_3: &[(u32, u8, i32)] = &[
    (0b0,  1, 0),
    (0b10,  2, 1),
    (0b11,  2, -1),
    // Same off-by-one length bug as MV_CODES_2.
    (0b1000,        4, 2),
    (0b1001,        4, -2),
    (0b10100,       5, 3),
    (0b10101,       5, -3),
    (0b101100,      6, 4),
    (0b101101,      6, -4),
    (0b1011100,     7, 5),
    (0b1011101,     7, -5),
    (0b10111100,    8, 6),
    (0b10111101,    8, -6),
    (0b101111100,   9, 7),
    (0b101111101,   9, -7),
    (0b1011111100, 10, i32::MIN),
];

pub fn mv_diff_vlc(idx: usize) -> VlcTable {
    let (codes, max_bits): (&[(u32, u8, i32)], u8) = match idx & 3 {
        0 => (MV_CODES_0, 10),
        1 => (MV_CODES_1,  9),
        // Tables 2-3 include 10-bit escape codes in this simplified decoder.
        2 => (MV_CODES_2, 10),
        _ => (MV_CODES_3, 10),
    };
    VlcTable::build(codes, max_bits)
}

// Backward-compat aliases used by old code referencing k0/k1
pub fn mv_diff_vlc_k0() -> VlcTable { mv_diff_vlc(0) }
pub fn mv_diff_vlc_k1() -> VlcTable { mv_diff_vlc(1) }

// ─── 2MV / 4MV block patterns ────────────────────────────────────────────────

pub fn mv2bp_vlc() -> VlcTable {
    const T: &[(u32, u8, i32)] = &[
        (0b1,    1, 3),
        (0b01,   2, 2),
        (0b001,  3, 1),
        (0b0001, 4, 0),
    ];
    VlcTable::build(T, 4)
}

pub fn mv4bp_vlc() -> VlcTable {
    const T: &[(u32, u8, i32)] = &[
        (0b1,          1, 15),
        (0b011,        3, 14),
        (0b0101,       4, 13),
        (0b01001,      5, 12),
        (0b010001,     6, 11),
        (0b0100001,    7, 10),
        (0b01000001,   8,  9),
        (0b010000001,  9,  8),
        (0b010000000,  9,  0),
    ];
    VlcTable::build(T, 9)
}

// ─── Zigzag / scan orders ────────────────────────────────────────────────────

/// Standard MPEG-2 zigzag.
pub const ZIGZAG: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// VC-1 Intra scan (SMPTE 421M §7.4.3).
pub const SCAN_INTRA: [usize; 64] = [
     0,  8, 16, 24,  1,  9,  2, 10,
    17, 25, 32, 40, 48, 56, 57, 49,
    41, 33, 26, 18,  3, 11,  4, 12,
    19, 27, 34, 42, 50, 58, 35, 43,
    51, 59, 20, 28,  5, 13,  6, 14,
    21, 29, 36, 44, 52, 60, 37, 45,
    53, 61, 22, 30,  7, 15, 23, 31,
    38, 46, 54, 62, 39, 47, 55, 63,
];

/// Horizontal scan for 8x4 top/bottom sub-blocks.
pub const SCAN_HORIZ: [usize; 64] = [
    0,  1,  2,  3,  4,  5,  6,  7,
    8,  9, 10, 11, 12, 13, 14, 15,
   16, 17, 18, 19, 20, 21, 22, 23,
   24, 25, 26, 27, 28, 29, 30, 31,
   32, 33, 34, 35, 36, 37, 38, 39,
   40, 41, 42, 43, 44, 45, 46, 47,
   48, 49, 50, 51, 52, 53, 54, 55,
   56, 57, 58, 59, 60, 61, 62, 63,
];

/// Vertical scan for 4x8 left/right sub-blocks.
pub const SCAN_VERT: [usize; 64] = [
    0,  8, 16, 24, 32, 40, 48, 56,
    1,  9, 17, 25, 33, 41, 49, 57,
    2, 10, 18, 26, 34, 42, 50, 58,
    3, 11, 19, 27, 35, 43, 51, 59,
    4, 12, 20, 28, 36, 44, 52, 60,
    5, 13, 21, 29, 37, 45, 53, 61,
    6, 14, 22, 30, 38, 46, 54, 62,
    7, 15, 23, 31, 39, 47, 55, 63,
];

// ─── WMV2 (MS-MPEG4 V8) VLC Tables ───────────────────────────────────────────
// Four TCOEF tables: INTER_0/1 (selected by ttcoef bit), INTRA_0/1.
// CBP: CBPY (luma, 4 blocks, 0..15) + CBPC_P (chroma+skip for P-frames).

const WMV2_TCOEF_INTER_0_CODES: &[(u32, u8, i32)] = &[
    (0b00,  2, 2),
    (0b010,  3, 3),
    (0b0110,  4, 4),
    (0b0111,  4, 258),
    (0b10000,  5, 6),
    (0b10001,  5, 514),
    (0b10010,  5, 5),
    (0b10011,  5, 259),
    (0b101000,  6, 8),
    (0b101001,  6, 770),
    (0b101010,  6, 7),
    (0b101011,  6, 515),
    (0b1011000,  7, 10),
    (0b1011001,  7, 12),
    (0b1011010,  7, 260),
    (0b1011011,  7, 1026),
    (0b1011100,  7, 1282),
    (0b1011101,  7, 9),
    (0b1011110,  7, 261),
    (0b1011111,  7, 771),
    (0b1100000,  7, 1027),
    (0b11000010,  8, 14),
    (0b11000011,  8, 16),
    (0b11000100,  8, 262),
    (0b11000101,  8, 516),
    (0b11000110,  8, 1538),
    (0b11000111,  8, 1794),
    (0b11001000,  8, 11),
    (0b11001001,  8, 1283),
    (0b110010100,  9, 18),
    (0b110010101,  9, 20),
    (0b110010110,  9, 22),
    (0b110010111,  9, 264),
    (0b110011000,  9, 2050),
    (0b110011001,  9, 2306),
    (0b110011010,  9, 13),
    (0b110011011,  9, 15),
    (0b110011100,  9, 263),
    (0b110011101,  9, 517),
    (0b110011110,  9, 1539),
    (0b1100111110, 10, 24),
    (0b1100111111, 10, 2562),
    (0b1101000000, 10, 2818),
    (0b1101000001, 10, 17),
    (0b1101000010, 10, 1795),
    (0b1101000011, 10, VLC_ESCAPE),
]; // max_bits=10

const WMV2_TCOEF_INTER_1_CODES: &[(u32, u8, i32)] = &[
    (0b00,  2, 2),
    (0b010,  3, 4),
    (0b011,  3, 3),
    (0b1000,  4, 6),
    (0b1001,  4, 258),
    (0b10100,  5, 8),
    (0b10101,  5, 514),
    (0b10110,  5, 5),
    (0b10111,  5, 259),
    (0b110000,  6, 10),
    (0b110001,  6, 12),
    (0b110010,  6, 260),
    (0b110011,  6, 770),
    (0b110100,  6, 7),
    (0b110101,  6, 515),
    (0b1101100,  7, 14),
    (0b1101101,  7, 16),
    (0b1101110,  7, 1026),
    (0b1101111,  7, 9),
    (0b1110000,  7, 261),
    (0b1110001,  7, 771),
    (0b11100100,  8, 18),
    (0b11100101,  8, 20),
    (0b11100110,  8, 262),
    (0b11100111,  8, 516),
    (0b11101000,  8, 1282),
    (0b11101001,  8, 11),
    (0b11101010,  8, 1027),
    (0b111010110,  9, 22),
    (0b111010111,  9, 24),
    (0b111011000,  9, 264),
    (0b111011001,  9, 1538),
    (0b111011010,  9, 1794),
    (0b111011011,  9, 13),
    (0b111011100,  9, 263),
    (0b111011101,  9, 517),
    (0b111011110,  9, 1283),
    (0b1110111110, 10, 26),
    (0b1110111111, 10, 28),
    (0b1111000000, 10, 2050),
    (0b1111000001, 10, 15),
    (0b1111000010, 10, 1539),
    (0b1111000011, 10, VLC_ESCAPE),
]; // max_bits=10

const WMV2_TCOEF_INTRA_0_CODES: &[(u32, u8, i32)] = &[
    (0b000,  3, 2),
    (0b0010,  4, 4),
    (0b0011,  4, 258),
    (0b0100,  4, 3),
    (0b01010,  5, 6),
    (0b01011,  5, 514),
    (0b01100,  5, 5),
    (0b01101,  5, 259),
    (0b011100,  6, 8),
    (0b011101,  6, 260),
    (0b011110,  6, 770),
    (0b011111,  6, 1026),
    (0b100000,  6, 7),
    (0b100001,  6, 515),
    (0b1000100,  7, 10),
    (0b1000101,  7, 12),
    (0b1000110,  7, 262),
    (0b1000111,  7, 516),
    (0b1001000,  7, 1282),
    (0b1001001,  7, 1538),
    (0b1001010,  7, 9),
    (0b1001011,  7, 261),
    (0b1001100,  7, 771),
    (0b1001101,  7, 1027),
    (0b10011100,  8, 14),
    (0b10011101,  8, 16),
    (0b10011110,  8, 18),
    (0b10011111,  8, 264),
    (0b10100000,  8, 518),
    (0b10100001,  8, 772),
    (0b10100010,  8, 1794),
    (0b10100011,  8, 2050),
    (0b10100100,  8, 11),
    (0b10100101,  8, 13),
    (0b10100110,  8, 263),
    (0b10100111,  8, 517),
    (0b10101000,  8, 1283),
    (0b10101001,  8, 1539),
    (0b101010100,  9, 20),
    (0b101010101,  9, 22),
    (0b101010110,  9, 24),
    (0b101010111,  9, 266),
    (0b101011000,  9, 1028),
    (0b101011001,  9, 1284),
    (0b101011010,  9, 2306),
    (0b101011011,  9, 2562),
    (0b101011100,  9, 15),
    (0b101011101,  9, 17),
    (0b101011110,  9, 265),
    (0b101011111,  9, 1795),
    (0b101100000,  9, 2051),
    (0b1011000010, 10, 26),
    (0b1011000011, 10, 28),
    (0b1011000100, 10, 30),
    (0b1011000101, 10, 2818),
    (0b1011000110, 10, 19),
    (0b1011000111, 10, 21),
    (0b1011001000, 10, 2307),
    (0b1011001001, 10, VLC_ESCAPE),
]; // max_bits=10

const WMV2_TCOEF_INTRA_1_CODES: &[(u32, u8, i32)] = &[
    (0b00,  2, 2),
    (0b010,  3, 3),
    (0b0110,  4, 4),
    (0b0111,  4, 6),
    (0b1000,  4, 258),
    (0b10010,  5, 8),
    (0b10011,  5, 514),
    (0b10100,  5, 5),
    (0b10101,  5, 259),
    (0b101100,  6, 10),
    (0b101101,  6, 260),
    (0b101110,  6, 770),
    (0b101111,  6, 7),
    (0b110000,  6, 515),
    (0b1100010,  7, 12),
    (0b1100011,  7, 14),
    (0b1100100,  7, 1026),
    (0b1100101,  7, 9),
    (0b1100110,  7, 261),
    (0b1100111,  7, 771),
    (0b11010000,  8, 16),
    (0b11010001,  8, 262),
    (0b11010010,  8, 516),
    (0b11010011,  8, 1282),
    (0b11010100,  8, 11),
    (0b11010101,  8, 1027),
    (0b110101100,  9, 18),
    (0b110101101,  9, 20),
    (0b110101110,  9, 264),
    (0b110101111,  9, 1538),
    (0b110110000,  9, 1794),
    (0b110110001,  9, 13),
    (0b110110010,  9, 263),
    (0b110110011,  9, 517),
    (0b110110100,  9, 1283),
    (0b1101101010, 10, 22),
    (0b1101101011, 10, 24),
    (0b1101101100, 10, 2050),
    (0b1101101101, 10, 15),
    (0b1101101110, 10, 1539),
    (0b1101101111, 10, VLC_ESCAPE),
]; // max_bits=10

// H.263 CBPY (luma CBP 4 blocks, symbol = 4-bit mask)
const WMV2_CBPY_CODES: &[(u32, u8, i32)] = &[
    (0b00,     2, 15),
    (0b0100,   4,  0),
    (0b0101,   4,  3),
    (0b0110,   4,  5),
    (0b0111,   4,  7),
    (0b1000,   4, 10),
    (0b1001,   4, 11),
    (0b1010,   4, 12),
    (0b1011,   4, 13),
    (0b1100,   4, 14),
    (0b11010,  5,  1),
    (0b11011,  5,  2),
    (0b11100,  5,  4),
    (0b11101,  5,  8),
    (0b111100, 6,  6),
    (0b111101, 6,  9),
]; // max_bits=6

// CBPC P-frame (chroma CBP 2 blocks + MB-skip; sym=-1 means skipped MB)
const WMV2_CBPC_P_CODES: &[(u32, u8, i32)] = &[
    (0b0,      1,  0),
    (0b10,     2,  1),
    (0b110,    3,  2),
    (0b1110,   4,  3),
    (0b111100, 6, -1),
]; // max_bits=6

pub fn wmv2_tcoef_inter_vlc(idx: usize) -> VlcTable {
    match idx & 1 {
        0 => VlcTable::build(WMV2_TCOEF_INTER_0_CODES, 10),
        _ => VlcTable::build(WMV2_TCOEF_INTER_1_CODES, 10),
    }
}
pub fn wmv2_tcoef_intra_vlc(idx: usize) -> VlcTable {
    match idx & 1 {
        0 => VlcTable::build(WMV2_TCOEF_INTRA_0_CODES, 10),
        _ => VlcTable::build(WMV2_TCOEF_INTRA_1_CODES, 10),
    }
}
pub fn wmv2_cbpy_vlc()   -> VlcTable { VlcTable::build(WMV2_CBPY_CODES,   6) }
pub fn wmv2_cbpc_p_vlc() -> VlcTable { VlcTable::build(WMV2_CBPC_P_CODES, 6) }

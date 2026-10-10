//! Exact VC-1 Simple/Main VLC tables used by the native WMV3 decoder.
//!
//! Values are transcribed from FFmpeg `libavcodec/vc1_vlc_data.h`.

use crate::na_rl_tables::{FF_RL_BASES, RlBase};
use crate::vlc::VlcTable;
use crate::vlc_tree::VlcTree;

const CBPCY_CODES: [[u32; 64]; 4] = [
    [
        0, 6, 15, 13, 13, 11, 3, 13, 5, 8, 49, 10, 12, 114, 102, 119, 1, 54, 96, 8, 10, 111, 5, 15,
        12, 10, 2, 12, 13, 115, 53, 63, 1, 7, 1, 7, 14, 12, 4, 14, 1, 9, 97, 11, 7, 58, 52, 62, 4,
        103, 1, 9, 11, 56, 101, 118, 4, 110, 100, 30, 2, 5, 4, 3,
    ],
    [
        0, 9, 1, 18, 5, 14, 237, 26, 3, 121, 3, 22, 13, 16, 6, 30, 2, 10, 1, 20, 12, 241, 5, 28,
        16, 12, 3, 24, 28, 124, 239, 247, 1, 240, 1, 19, 18, 15, 4, 27, 1, 122, 2, 23, 1, 17, 7,
        31, 1, 11, 2, 21, 19, 246, 238, 29, 17, 13, 236, 25, 58, 63, 8, 125,
    ],
    [
        0, 201, 25, 231, 5, 221, 1, 3, 2, 414, 2, 241, 16, 225, 195, 492, 2, 412, 1, 240, 7, 224,
        98, 245, 1, 220, 96, 5, 9, 230, 101, 247, 1, 102, 1, 415, 24, 3, 2, 244, 3, 54, 3, 484, 17,
        114, 200, 493, 3, 413, 1, 4, 13, 113, 99, 485, 4, 111, 194, 243, 5, 29, 26, 31,
    ],
    [
        0, 28, 12, 44, 3, 36, 20, 52, 2, 32, 16, 48, 8, 40, 24, 28, 1, 30, 14, 46, 6, 38, 22, 54,
        3, 34, 18, 50, 10, 42, 26, 30, 1, 29, 13, 45, 5, 37, 21, 53, 2, 33, 17, 49, 9, 41, 25, 29,
        1, 31, 15, 47, 7, 39, 23, 55, 4, 35, 19, 51, 11, 43, 27, 31,
    ],
];

const CBPCY_BITS: [[u8; 64]; 4] = [
    [
        13, 13, 7, 13, 7, 13, 13, 12, 6, 13, 7, 12, 6, 8, 8, 8, 5, 7, 8, 12, 6, 8, 13, 12, 7, 13,
        13, 12, 6, 8, 7, 7, 6, 13, 8, 12, 7, 13, 13, 12, 7, 13, 8, 12, 5, 7, 7, 7, 6, 8, 13, 12, 6,
        7, 8, 8, 5, 8, 8, 6, 3, 3, 3, 2,
    ],
    [
        14, 13, 8, 13, 3, 13, 8, 13, 3, 7, 8, 13, 4, 13, 13, 13, 3, 13, 13, 13, 4, 8, 13, 13, 5,
        13, 13, 13, 5, 7, 8, 8, 3, 8, 14, 13, 5, 13, 13, 13, 4, 7, 13, 13, 6, 13, 13, 13, 5, 13, 8,
        13, 5, 8, 8, 13, 5, 13, 8, 13, 6, 6, 13, 7,
    ],
    [
        13, 8, 6, 8, 4, 8, 13, 12, 4, 9, 8, 8, 5, 8, 8, 9, 5, 9, 10, 8, 4, 8, 7, 8, 6, 8, 7, 13, 4,
        8, 7, 8, 5, 7, 8, 9, 6, 13, 13, 8, 4, 6, 8, 9, 5, 7, 8, 9, 5, 9, 9, 13, 5, 7, 7, 9, 4, 7,
        8, 8, 3, 5, 5, 5,
    ],
    [
        9, 9, 9, 9, 2, 9, 9, 9, 2, 9, 9, 9, 9, 9, 9, 8, 3, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
        9, 8, 2, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 8, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
        9, 9, 9, 8,
    ],
];

pub fn cbpcy_vlcs() -> [VlcTable; 4] {
    std::array::from_fn(|tab| {
        let mut entries = Vec::with_capacity(64);
        for sym in 0..64 {
            entries.push((CBPCY_CODES[tab][sym], CBPCY_BITS[tab][sym], sym as i32));
        }
        VlcTable::build(&entries, CBPCY_BITS[tab].iter().copied().max().unwrap_or(1))
    })
}

const MVDATA_CODES: [[u32; 73]; 4] = [
    [
        0, 2, 3, 8, 576, 3, 2, 6, 5, 577, 578, 7, 8, 9, 40, 19, 37, 82, 21, 22, 23, 579, 580, 166,
        96, 167, 49, 194, 195, 581, 582, 583, 292, 293, 294, 13, 2, 7, 24, 50, 102, 295, 13, 7, 8,
        18, 50, 103, 38, 20, 21, 22, 39, 204, 103, 23, 24, 25, 104, 410, 105, 106, 107, 108, 109,
        220, 411, 442, 222, 443, 446, 447, 7,
    ],
    [
        0, 4, 5, 3, 4, 3, 4, 5, 20, 6, 21, 44, 45, 46, 3008, 95, 112, 113, 57, 3009, 3010, 116,
        117, 3011, 118, 3012, 3013, 3014, 3015, 3016, 3017, 3018, 3019, 3020, 3021, 3022, 1, 4, 15,
        160, 161, 41, 6, 11, 42, 162, 43, 119, 56, 57, 58, 163, 236, 237, 3023, 119, 120, 242, 122,
        486, 1512, 487, 246, 494, 1513, 495, 1514, 1515, 1516, 1517, 1518, 1519, 31,
    ],
    [
        0, 512, 513, 514, 515, 2, 3, 258, 259, 260, 261, 262, 263, 264, 265, 266, 267, 268, 269,
        270, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 282, 283, 284, 285, 286, 1, 5,
        287, 288, 289, 290, 6, 7, 291, 292, 293, 294, 295, 296, 297, 298, 299, 300, 301, 302, 303,
        304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 317, 318, 319,
    ],
    [
        0, 1, 1, 2, 3, 4, 1, 5, 4, 3, 5, 8, 6, 9, 10, 11, 12, 7, 104, 14, 105, 4, 10, 15, 11, 6,
        14, 8, 106, 107, 108, 15, 109, 9, 55, 10, 1, 2, 1, 2, 3, 12, 6, 2, 6, 7, 28, 7, 15, 8, 5,
        18, 29, 152, 77, 24, 25, 26, 39, 108, 13, 109, 55, 56, 57, 116, 11, 153, 234, 235, 118,
        119, 15,
    ],
];

const MVDATA_BITS: [[u8; 73]; 4] = [
    [
        6, 7, 7, 8, 14, 6, 5, 6, 7, 14, 14, 6, 6, 6, 8, 9, 10, 9, 7, 7, 7, 14, 14, 10, 9, 10, 8,
        10, 10, 14, 14, 14, 13, 13, 13, 6, 3, 5, 6, 8, 9, 13, 5, 4, 4, 5, 7, 9, 6, 5, 5, 5, 6, 9,
        8, 5, 5, 5, 7, 10, 7, 7, 7, 7, 7, 8, 10, 9, 8, 9, 9, 9, 3,
    ],
    [
        5, 7, 7, 6, 6, 5, 5, 6, 7, 5, 7, 8, 8, 8, 14, 9, 9, 9, 8, 14, 14, 9, 9, 14, 9, 14, 14, 14,
        14, 14, 14, 14, 14, 14, 14, 14, 2, 3, 6, 8, 8, 6, 3, 4, 6, 8, 6, 9, 6, 6, 6, 8, 8, 8, 14,
        7, 7, 8, 7, 9, 13, 9, 8, 9, 13, 9, 13, 13, 13, 13, 13, 13, 5,
    ],
    [
        3, 12, 12, 12, 12, 3, 4, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11,
        11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 1, 5, 11, 11, 11, 11, 4, 4, 11, 11, 11,
        11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11,
        11, 11, 11,
    ],
    [
        15, 11, 15, 15, 15, 15, 12, 15, 12, 11, 12, 12, 15, 12, 12, 12, 12, 15, 15, 12, 15, 10, 11,
        12, 11, 10, 11, 10, 15, 15, 15, 11, 15, 10, 14, 10, 4, 4, 5, 7, 8, 9, 5, 3, 4, 5, 6, 8, 5,
        4, 3, 5, 6, 8, 7, 5, 5, 5, 6, 7, 9, 7, 6, 6, 6, 7, 10, 8, 8, 8, 7, 7, 4,
    ],
];

pub fn mvdata_vlcs() -> [VlcTable; 4] {
    std::array::from_fn(|tab| {
        let mut entries = Vec::with_capacity(73);
        for sym in 0..73 {
            entries.push((MVDATA_CODES[tab][sym], MVDATA_BITS[tab][sym], sym as i32));
        }
        VlcTable::build(
            &entries,
            MVDATA_BITS[tab].iter().copied().max().unwrap_or(1),
        )
    })
}

// VC-1 transform-type enumeration, identical to FFmpeg's TransformTypes.
pub const TT_8X8: u8 = 0;
pub const TT_8X4_BOTTOM: u8 = 1;
pub const TT_8X4_TOP: u8 = 2;
pub const TT_8X4: u8 = 3;
pub const TT_4X8_RIGHT: u8 = 4;
pub const TT_4X8_LEFT: u8 = 5;
pub const TT_4X8: u8 = 6;
pub const TT_4X4: u8 = 7;

const TTMB_CODES: [[u32; 16]; 3] = [
    [
        0x0003, 0x002E, 0x005F, 0x0000, 0x0016, 0x0015, 0x0001, 0x0004, 0x0014, 0x02F1, 0x0179,
        0x017B, 0x0BC0, 0x0BC1, 0x05E1, 0x017A,
    ],
    [
        0x0006, 0x0006, 0x0003, 0x0007, 0x000F, 0x000E, 0x0000, 0x0002, 0x0002, 0x0014, 0x0011,
        0x000B, 0x0009, 0x0021, 0x0015, 0x0020,
    ],
    [
        0x0006, 0x0000, 0x000E, 0x0005, 0x0002, 0x0003, 0x0003, 0x000F, 0x0002, 0x0081, 0x0021,
        0x0009, 0x0101, 0x0041, 0x0011, 0x0100,
    ],
];
const TTMB_BITS: [[u8; 16]; 3] = [
    [2, 6, 7, 2, 5, 5, 2, 3, 5, 10, 9, 9, 12, 12, 11, 9],
    [3, 4, 4, 4, 4, 4, 3, 3, 2, 7, 7, 6, 6, 8, 7, 8],
    [3, 3, 4, 5, 3, 3, 4, 4, 2, 10, 8, 6, 11, 9, 7, 11],
];
const TTBLK_CODES: [[u32; 8]; 3] = [
    [0, 1, 3, 5, 16, 17, 18, 19],
    [3, 0, 1, 2, 3, 5, 8, 9],
    [1, 0, 1, 4, 6, 7, 10, 11],
];
const TTBLK_BITS: [[u8; 8]; 3] = [
    [2, 2, 2, 3, 5, 5, 5, 5],
    [2, 3, 3, 3, 3, 3, 4, 4],
    [2, 3, 3, 3, 3, 3, 4, 4],
];
const SUBBLKPAT_CODES: [[u32; 15]; 3] = [
    [14, 12, 7, 11, 9, 26, 2, 10, 27, 8, 0, 6, 1, 15, 1],
    [14, 0, 8, 15, 10, 4, 23, 13, 5, 9, 25, 3, 24, 22, 1],
    [5, 6, 2, 2, 8, 0, 28, 3, 1, 3, 29, 1, 19, 18, 15],
];
const SUBBLKPAT_BITS: [[u8; 15]; 3] = [
    [5, 5, 5, 5, 5, 6, 4, 5, 6, 5, 4, 5, 4, 5, 1],
    [4, 3, 4, 4, 4, 5, 5, 4, 5, 4, 5, 4, 5, 5, 2],
    [3, 3, 4, 3, 4, 5, 5, 3, 5, 4, 5, 4, 5, 5, 4],
];

pub const VC1_ZZ_4X4: [usize; 16] = [0, 8, 16, 1, 9, 24, 17, 2, 10, 18, 25, 3, 11, 26, 19, 27];
pub const VC1_ZZ_8X4: [usize; 32] = [
    0, 8, 1, 16, 2, 9, 10, 3, 24, 17, 4, 11, 18, 12, 5, 19, 25, 13, 20, 26, 27, 6, 21, 28, 14, 22,
    29, 7, 30, 15, 23, 31,
];
pub const VC1_ZZ_4X8: [usize; 32] = [
    0, 1, 8, 2, 9, 16, 17, 24, 10, 32, 25, 18, 40, 3, 33, 26, 48, 11, 56, 41, 34, 49, 57, 42, 19,
    50, 27, 58, 35, 43, 51, 59,
];

pub const TTBLK_TO_TT: [[u8; 8]; 3] = [
    [
        TT_8X4,
        TT_4X8,
        TT_8X8,
        TT_4X4,
        TT_8X4_TOP,
        TT_8X4_BOTTOM,
        TT_4X8_RIGHT,
        TT_4X8_LEFT,
    ],
    [
        TT_8X8,
        TT_4X8_RIGHT,
        TT_4X8_LEFT,
        TT_4X4,
        TT_8X4,
        TT_4X8,
        TT_8X4_BOTTOM,
        TT_8X4_TOP,
    ],
    [
        TT_8X8,
        TT_4X8,
        TT_4X4,
        TT_8X4_BOTTOM,
        TT_4X8_RIGHT,
        TT_4X8_LEFT,
        TT_8X4,
        TT_8X4_TOP,
    ],
];

pub fn ttmb_vlcs() -> [VlcTable; 3] {
    std::array::from_fn(|tab| {
        let entries: Vec<(u32, u8, i32)> = (0..16)
            .map(|sym| (TTMB_CODES[tab][sym], TTMB_BITS[tab][sym], sym as i32))
            .collect();
        VlcTable::build(&entries, TTMB_BITS[tab].iter().copied().max().unwrap_or(1))
    })
}

pub fn ttblk_vlcs() -> [VlcTable; 3] {
    std::array::from_fn(|tab| {
        let entries: Vec<(u32, u8, i32)> = (0..8)
            .map(|sym| (TTBLK_CODES[tab][sym], TTBLK_BITS[tab][sym], sym as i32))
            .collect();
        VlcTable::build(&entries, TTBLK_BITS[tab].iter().copied().max().unwrap_or(1))
    })
}

pub fn subblkpat_vlcs() -> [VlcTable; 3] {
    std::array::from_fn(|tab| {
        let entries: Vec<(u32, u8, i32)> = (0..15)
            .map(|sym| {
                (
                    SUBBLKPAT_CODES[tab][sym],
                    SUBBLKPAT_BITS[tab][sym],
                    sym as i32,
                )
            })
            .collect();
        VlcTable::build(
            &entries,
            SUBBLKPAT_BITS[tab].iter().copied().max().unwrap_or(1),
        )
    })
}

const HIGH_RATE_INTRA_VLC: [(u32, u8); 163] = [
    (0x0, 2),
    (0x3, 3),
    (0xD, 4),
    (0x5, 4),
    (0x1C, 5),
    (0x16, 5),
    (0x3F, 6),
    (0x3A, 6),
    (0x2E, 6),
    (0x22, 6),
    (0x7B, 7),
    (0x67, 7),
    (0x5F, 7),
    (0x47, 7),
    (0x26, 7),
    (0xEF, 8),
    (0xCD, 8),
    (0xC1, 8),
    (0xA9, 8),
    (0x4F, 8),
    (0x1F2, 9),
    (0x1DD, 9),
    (0x199, 9),
    (0x185, 9),
    (0x15D, 9),
    (0x11B, 9),
    (0x3EF, 10),
    (0x3E1, 10),
    (0x3C8, 10),
    (0x331, 10),
    (0x303, 10),
    (0x2F1, 10),
    (0x2A0, 10),
    (0x233, 10),
    (0x126, 10),
    (0x7C0, 11),
    (0x76F, 11),
    (0x76C, 11),
    (0x661, 11),
    (0x604, 11),
    (0x572, 11),
    (0x551, 11),
    (0x46A, 11),
    (0x274, 11),
    (0xF27, 12),
    (0xF24, 12),
    (0xEDB, 12),
    (0xC8E, 12),
    (0xC0B, 12),
    (0xC0A, 12),
    (0xAE3, 12),
    (0x8D6, 12),
    (0x490, 12),
    (0x495, 12),
    (0x1F19, 13),
    (0x1DB5, 13),
    (0x9, 4),
    (0x10, 5),
    (0x29, 6),
    (0x62, 7),
    (0xF3, 8),
    (0xAD, 8),
    (0x1E5, 9),
    (0x179, 9),
    (0x9C, 9),
    (0x3B1, 10),
    (0x2AE, 10),
    (0x127, 10),
    (0x76E, 11),
    (0x570, 11),
    (0x275, 11),
    (0xF25, 12),
    (0xEC0, 12),
    (0xAA0, 12),
    (0x8D7, 12),
    (0x1E4C, 13),
    (0x8, 5),
    (0x63, 7),
    (0xAF, 8),
    (0x17B, 9),
    (0x3B3, 10),
    (0x7DD, 11),
    (0x640, 11),
    (0xF8D, 12),
    (0xBC1, 12),
    (0x491, 12),
    (0x28, 6),
    (0xC3, 8),
    (0x151, 9),
    (0x2A1, 10),
    (0x573, 11),
    (0xEC3, 12),
    (0x1F35, 13),
    (0x65, 7),
    (0x1DA, 9),
    (0x2AF, 10),
    (0x277, 11),
    (0x8C9, 12),
    (0x1781, 13),
    (0x25, 7),
    (0x118, 9),
    (0x646, 11),
    (0xAA6, 12),
    (0x1780, 13),
    (0xC9, 8),
    (0x321, 10),
    (0xF9B, 12),
    (0x191E, 13),
    (0x48, 8),
    (0x7CC, 11),
    (0xAA1, 12),
    (0x180, 9),
    (0x465, 11),
    (0x1905, 13),
    (0x3E2, 10),
    (0xEC1, 12),
    (0x3C9B, 14),
    (0x2F4, 10),
    (0x8C8, 12),
    (0x7C1, 11),
    (0x928, 13),
    (0x5E1, 11),
    (0x320D, 14),
    (0xEC2, 12),
    (0x6418, 15),
    (0x1F34, 13),
    (0x78, 7),
    (0x155, 9),
    (0x552, 11),
    (0x191F, 13),
    (0xFA, 8),
    (0x7DC, 11),
    (0x1907, 13),
    (0xAC, 8),
    (0x249, 11),
    (0x13B1, 14),
    (0x1F6, 9),
    (0xAE2, 12),
    (0x1DC, 9),
    (0x4ED, 12),
    (0x184, 9),
    (0x1904, 13),
    (0x156, 9),
    (0x9D9, 13),
    (0x3E7, 10),
    (0x929, 13),
    (0x3B2, 10),
    (0x3B68, 14),
    (0x2F5, 10),
    (0x13B0, 14),
    (0x322, 10),
    (0x3B69, 14),
    (0x234, 10),
    (0x7935, 15),
    (0x7C7, 11),
    (0xC833, 16),
    (0x660, 11),
    (0x7934, 15),
    (0x24B, 11),
    (0xC832, 16),
    (0xAA7, 12),
    (0x1F18, 13),
    (0x7A, 7),
];
const HIGH_RATE_INTER_VLC: [(u32, u8); 175] = [
    (0x2, 2),
    (0x0, 3),
    (0x1E, 5),
    (0x4, 5),
    (0x12, 6),
    (0x70, 7),
    (0x1A, 7),
    (0x5F, 8),
    (0x47, 8),
    (0x1D3, 9),
    (0xB5, 9),
    (0x57, 9),
    (0x3B5, 10),
    (0x16D, 10),
    (0x162, 10),
    (0x7CE, 11),
    (0x719, 11),
    (0x691, 11),
    (0x2C6, 11),
    (0x156, 11),
    (0xF92, 12),
    (0xD2E, 12),
    (0xD20, 12),
    (0x59E, 12),
    (0x468, 12),
    (0x2A6, 12),
    (0x1DA2, 13),
    (0x1C60, 13),
    (0x1A43, 13),
    (0xB1D, 13),
    (0x8C0, 13),
    (0x55D, 13),
    (0x3, 3),
    (0xA, 5),
    (0x77, 7),
    (0xE5, 8),
    (0x1D9, 9),
    (0x3E5, 10),
    (0x166, 10),
    (0x694, 11),
    (0x152, 11),
    (0x59F, 12),
    (0x1F3C, 13),
    (0x1A4B, 13),
    (0x55E, 13),
    (0xC, 4),
    (0x7D, 7),
    (0x44, 8),
    (0x3E0, 10),
    (0x769, 11),
    (0xE31, 12),
    (0x1F26, 13),
    (0x55C, 13),
    (0x1B, 5),
    (0xE2, 8),
    (0x3A5, 10),
    (0x2C9, 11),
    (0x1F23, 13),
    (0x3B47, 14),
    (0x7, 5),
    (0x1D8, 9),
    (0x2D8, 11),
    (0x1F27, 13),
    (0x3494, 14),
    (0x35, 6),
    (0x3E1, 10),
    (0x59C, 12),
    (0x38C3, 14),
    (0xC, 6),
    (0x165, 10),
    (0x1D23, 13),
    (0x1638, 14),
    (0x68, 7),
    (0x693, 11),
    (0x3A45, 14),
    (0x20, 7),
    (0xF90, 12),
    (0x7CF6, 15),
    (0xE8, 8),
    (0x58F, 12),
    (0x2CEF, 15),
    (0x45, 8),
    (0xB3A, 13),
    (0x1F1, 9),
    (0x3B46, 14),
    (0x1A7, 9),
    (0x1676, 14),
    (0x56, 9),
    (0x692A, 15),
    (0x38D, 10),
    (0xE309, 16),
    (0xAA, 10),
    (0x1C611, 17),
    (0x2DF, 11),
    (0xB3B9, 17),
    (0x2C8, 11),
    (0x38C20, 18),
    (0x1B0, 11),
    (0x16390, 18),
    (0xF9F, 12),
    (0x16771, 18),
    (0xED0, 12),
    (0x71843, 19),
    (0xD2A, 12),
    (0xF9E8C, 20),
    (0x461, 12),
    (0xF9E8E, 20),
    (0xB67, 13),
    (0x55F, 13),
    (0x3F, 6),
    (0x6D, 9),
    (0xE90, 12),
    (0x54E, 13),
    (0x13, 6),
    (0x119, 10),
    (0xB66, 13),
    (0xB, 6),
    (0x235, 11),
    (0x7CF5, 15),
    (0x75, 7),
    (0xD24, 12),
    (0xF9E9, 16),
    (0x2E, 7),
    (0x1F22, 13),
    (0x21, 7),
    (0x54F, 13),
    (0x14, 7),
    (0x3A44, 14),
    (0xE4, 8),
    (0x7CF7, 15),
    (0x5E, 8),
    (0x7185, 15),
    (0x37, 8),
    (0x2C73, 15),
    (0x1DB, 9),
    (0x59DD, 16),
    (0x1C7, 9),
    (0x692B, 15),
    (0x1A6, 9),
    (0x58E5, 16),
    (0xB4, 9),
    (0x1F3D0, 17),
    (0xB0, 9),
    (0xB1C9, 17),
    (0x3E6, 10),
    (0x16770, 18),
    (0x16E, 10),
    (0x3E7A2, 18),
    (0x11B, 10),
    (0xF9E8D, 20),
    (0xD9, 10),
    (0xF9E8F, 20),
    (0xA8, 10),
    (0x2C723, 19),
    (0x749, 11),
    (0xE3084, 20),
    (0x696, 11),
    (0x58E45, 20),
    (0x2DE, 11),
    (0xB1C88, 21),
    (0x231, 11),
    (0x1C610A, 21),
    (0x1B1, 11),
    (0x71842D, 23),
    (0xD2B, 12),
    (0x38C217, 22),
    (0xD2F, 12),
    (0x163913, 22),
    (0x5B2, 12),
    (0x163912, 22),
    (0x469, 12),
    (0x71842C, 23),
    (0x1A42, 13),
    (0x8C1, 13),
    (0x73, 7),
];

const HIGH_RATE_INTRA_NOLAST_MAX_LEVEL: &[u8] = &[56, 20, 10, 7, 6, 5, 4, 3, 3, 3, 2, 2, 2, 2, 1];
const HIGH_RATE_INTRA_LAST_MAX_LEVEL: &[u8] = &[4, 3, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1];
const HIGH_RATE_INTER_NOLAST_MAX_LEVEL: &[u8] = &[
    32, 13, 8, 6, 5, 4, 4, 3, 3, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1,
];
const HIGH_RATE_INTER_LAST_MAX_LEVEL: &[u8] = &[
    4, 3, 3, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1,
];

#[derive(Clone)]
pub struct Vc1AcTable {
    n: usize,
    last: usize,
    vlc: VlcTree,
    run: Vec<u8>,
    level: Vec<u8>,
    max_level: [[u8; 65]; 2],
    max_run: [[u8; 65]; 2],
}

impl Vc1AcTable {
    fn finish(n: usize, last: usize, vlc: VlcTree, run: Vec<u8>, level: Vec<u8>) -> Self {
        let mut max_level = [[0u8; 65]; 2];
        let mut max_run = [[0u8; 65]; 2];
        for last_flag in 0..2usize {
            let (start, end) = if last_flag == 0 { (0, last) } else { (last, n) };
            for i in start..end {
                let r = run[i] as usize;
                let l = level[i] as usize;
                if r < 65 && l < 65 {
                    max_level[last_flag][r] = max_level[last_flag][r].max(level[i]);
                    max_run[last_flag][l] = max_run[last_flag][l].max(run[i]);
                }
            }
        }
        Self {
            n,
            last,
            vlc,
            run,
            level,
            max_level,
            max_run,
        }
    }

    fn from_base(base: &RlBase) -> Self {
        let mut vlc = VlcTree::new();
        for (idx, (code, len)) in base.vlc.iter().enumerate() {
            if *len != 0 {
                vlc.insert(*code, *len, idx as i32);
            }
        }
        Self::finish(
            base.n,
            base.last,
            vlc,
            base.run.to_vec(),
            base.level.to_vec(),
        )
    }

    fn from_high(codes: &[(u32, u8)], last: usize, no_last_max: &[u8], last_max: &[u8]) -> Self {
        let n = codes.len() - 1; // final VLC entry is ESCAPE
        let mut run = Vec::with_capacity(n);
        let mut level = Vec::with_capacity(n);
        for (r, &max_l) in no_last_max.iter().enumerate() {
            for l in 1..=max_l {
                run.push(r as u8);
                level.push(l);
            }
        }
        assert_eq!(run.len(), last);
        for (r, &max_l) in last_max.iter().enumerate() {
            for l in 1..=max_l {
                run.push(r as u8);
                level.push(l);
            }
        }
        assert_eq!(run.len(), n);
        let mut vlc = VlcTree::new();
        for (idx, &(code, len)) in codes.iter().enumerate() {
            vlc.insert(code, len, idx as i32);
        }
        Self::finish(n, last, vlc, run, level)
    }

    #[inline]
    pub fn decode_index(&self, br: &mut crate::bitreader::BitReader<'_>) -> Option<usize> {
        self.vlc
            .decode(br)
            .and_then(|v| if v >= 0 { Some(v as usize) } else { None })
    }
    #[inline]
    pub fn is_escape(&self, idx: usize) -> bool {
        idx == self.n
    }
    #[inline]
    pub fn is_last(&self, idx: usize) -> bool {
        idx >= self.last && idx < self.n
    }
    #[inline]
    pub fn run_level(&self, idx: usize) -> Option<(u8, u8)> {
        if idx >= self.n {
            None
        } else {
            Some((self.run[idx], self.level[idx]))
        }
    }
    #[inline]
    pub fn max_level(&self, last: bool, run: usize) -> u8 {
        self.max_level[last as usize][run.min(64)]
    }
    #[inline]
    pub fn max_run(&self, last: bool, level: usize) -> u8 {
        self.max_run[last as usize][level.min(64)]
    }
}

/// Eight VC-1 AC coding sets in the order used by FFmpeg / SMPTE 421M:
/// high-motion intra/inter, low-motion intra/inter, mid-rate intra/inter,
/// high-rate intra/inter.
pub fn ac_tables() -> [Vc1AcTable; 8] {
    [
        Vc1AcTable::from_base(&FF_RL_BASES[1]), // 186, last=119
        Vc1AcTable::from_base(&FF_RL_BASES[4]), // 169, last=99
        Vc1AcTable::from_base(&FF_RL_BASES[0]), // 133, last=85
        Vc1AcTable::from_base(&FF_RL_BASES[3]), // 149, last=81
        Vc1AcTable::from_base(&FF_RL_BASES[2]), // 103, last=67
        Vc1AcTable::from_base(&FF_RL_BASES[5]), // 103, last=58
        Vc1AcTable::from_high(
            &HIGH_RATE_INTRA_VLC,
            126,
            HIGH_RATE_INTRA_NOLAST_MAX_LEVEL,
            HIGH_RATE_INTRA_LAST_MAX_LEVEL,
        ),
        Vc1AcTable::from_high(
            &HIGH_RATE_INTER_VLC,
            109,
            HIGH_RATE_INTER_NOLAST_MAX_LEVEL,
            HIGH_RATE_INTER_LAST_MAX_LEVEL,
        ),
    ]
}

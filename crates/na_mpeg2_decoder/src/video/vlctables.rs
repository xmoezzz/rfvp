use std::sync::OnceLock;

use super::vlc::{Vlc, VlcElem, RlVlcElem};
use super::tables::*;

pub const DC_VLC_BITS: i32 = 9;
pub const MV_VLC_BITS: i32 = 8;
pub const TEX_VLC_BITS: i32 = 9;

pub const MBINCR_VLC_BITS: i32 = 9;
pub const MB_PAT_VLC_BITS: i32 = 9;
pub const MB_PTYPE_VLC_BITS: i32 = 6;
pub const MB_BTYPE_VLC_BITS: i32 = 6;

pub const MPEG12_RL_NB_ELEMS: usize = 111;

// MB_TYPE flags (subset used by MPEG-1/2 decode paths).
// Values follow the upstream bit layout (int16_t tables in mpeg12.c).
pub const MB_TYPE_INTRA: i16 = 0x0001;
pub const MB_TYPE_CBP: i16 = 0x0002;
pub const MB_TYPE_FORWARD_MV: i16 = 0x0004;
pub const MB_TYPE_BACKWARD_MV: i16 = 0x0008;
pub const MB_TYPE_BIDIR_MV: i16 = 0x0010;
pub const MB_TYPE_QUANT: i16 = 0x0020;

pub const MB_TYPE_16x16: i16 = 0x0040;
pub const MB_TYPE_16x8: i16 = 0x0080;
pub const MB_TYPE_INTERLACED: i16 = 0x0100;
pub const MB_TYPE_SKIP: i16 = 0x0200;

pub const MB_TYPE_ZERO_MV: i16 = 0x0400;

#[inline]
pub fn is_intra(mb_type: i16) -> bool { (mb_type & MB_TYPE_INTRA) != 0 }
#[inline]
pub fn has_cbp(mb_type: i16) -> bool { (mb_type & MB_TYPE_CBP) != 0 }
#[inline]
pub fn is_quant(mb_type: i16) -> bool { (mb_type & MB_TYPE_QUANT) != 0 }

#[inline]
pub fn mb_type_mv_2_mv_dir(mb_type: i16) -> i32 {
    // Equivalent to `MB_TYPE_MV_2_MV_DIR` for MPEG-1/2 subset.
    // Forward/back/bidir flags map to MV_DIR_FORWARD/MV_DIR_BACKWARD.
    let mut dir = 0;
    if (mb_type & MB_TYPE_FORWARD_MV) != 0 { dir |= 1; }
    if (mb_type & MB_TYPE_BACKWARD_MV) != 0 { dir |= 2; }
    if (mb_type & MB_TYPE_BIDIR_MV) != 0 { dir |= 3; }
    dir
}

pub const MV_DIR_FORWARD: i32 = 1;
pub const MV_DIR_BACKWARD: i32 = 2;

#[derive(Clone, Debug)]
pub struct Mpeg12Vlcs {
    pub dc_lum: Vlc,
    pub dc_chroma: Vlc,
    pub mv: Vlc,
    pub mbincr: Vlc,
    pub mb_pat: Vlc,
    pub mb_ptype: Vlc,
    pub mb_btype: Vlc,
    pub rl_mpeg1: Vec<RlVlcElem>,
    pub rl_mpeg2: Vec<RlVlcElem>,
}

static VLC_CACHE: OnceLock<Mpeg12Vlcs> = OnceLock::new();

fn init_2d_vlc_rl(table_vlc: &[[u16; 2]; 113]) -> Vec<RlVlcElem> {
    // Build base VLC table into VLCElem and then postprocess into RL_VLC_ELEM.
    let mut bits: Vec<u8> = Vec::with_capacity(113);
    let mut codes: Vec<u16> = Vec::with_capacity(113);
    for [code, len] in table_vlc.iter() {
        codes.push(*code);
        bits.push(*len as u8);
    }

    let base = Vlc::init_sparse(TEX_VLC_BITS, &bits, &codes, None);
    // Copy/convert.
    let mut out = vec![RlVlcElem::default(); base.table.len()];
    for (i, e) in base.table.iter().enumerate() {
        let idx = e.sym as i32;
        let len = e.len as i32;
        let (level, run) = if len == 0 {
            (i16::MAX, 65u8)
        } else if len < 0 {
            (idx as i16, 0u8)
        } else {
            if idx as usize == MPEG12_RL_NB_ELEMS {
                (0, 65)
            } else if idx as usize == MPEG12_RL_NB_ELEMS + 1 {
                (127, 0)
            } else {
                let r = FF_MPEG12_RUN[idx as usize] as i32 + 1;
                let l = FF_MPEG12_LEVEL[idx as usize] as i32;
                (l as i16, r as u8)
            }
        };
        out[i] = RlVlcElem { level, len8: len as i8, run };
    }
    out
}

fn build() -> Mpeg12Vlcs {
    // DC tables.
    let dc_lum = Vlc::init_sparse(DC_VLC_BITS, &FF_MPEG12_VLC_DC_LUM_BITS, &FF_MPEG12_VLC_DC_LUM_CODE, None);
    let dc_chroma = Vlc::init_sparse(DC_VLC_BITS, &FF_MPEG12_VLC_DC_CHROMA_BITS, &FF_MPEG12_VLC_DC_CHROMA_CODE, None);

    // MV table.
    let mut mv_bits = [0u8; 17];
    let mut mv_codes = [0u16; 17];
    for (i, [c, b]) in FF_MPEG12_MBMOTIONVECTORTABLE.iter().enumerate() {
        mv_codes[i] = *c as u16;
        mv_bits[i] = *b;
    }
    let mv = Vlc::init_sparse(MV_VLC_BITS, &mv_bits, &mv_codes, None);

    // MB incr.
    let mut mbincr_bits = [0u8; 36];
    let mut mbincr_codes = [0u16; 36];
    for (i, [c, b]) in FF_MPEG12_MBADDRINCRTABLE.iter().enumerate() {
        mbincr_codes[i] = *c as u16;
        mbincr_bits[i] = *b;
    }
    let mbincr = Vlc::init_sparse(MBINCR_VLC_BITS, &mbincr_bits, &mbincr_codes, None);

    // MB pattern.
    let mut mbpat_bits = [0u8; 64];
    let mut mbpat_codes = [0u16; 64];
    for (i, [c, b]) in FF_MPEG12_MBPATTABLE.iter().enumerate() {
        mbpat_codes[i] = *c as u16;
        mbpat_bits[i] = *b;
    }
    let mb_pat = Vlc::init_sparse(MB_PAT_VLC_BITS, &mbpat_bits, &mbpat_codes, None);

    // P-type and B-type.
    // From mpeg12.c `table_mb_ptype` and `ptype2mb_type`.
    const TABLE_MB_PTYPE: [[u8; 2]; 7] = [
        [3, 5],
        [1, 2],
        [1, 3],
        [1, 1],
        [1, 6],
        [1, 5],
        [2, 5],
    ];

    const PTYPE2MB_TYPE: [i16; 7] = [
        MB_TYPE_INTRA,
        MB_TYPE_FORWARD_MV | MB_TYPE_CBP | MB_TYPE_ZERO_MV | MB_TYPE_16x16,
        MB_TYPE_FORWARD_MV,
        MB_TYPE_FORWARD_MV | MB_TYPE_CBP,
        MB_TYPE_QUANT | MB_TYPE_INTRA,
        MB_TYPE_QUANT | MB_TYPE_FORWARD_MV | MB_TYPE_CBP | MB_TYPE_ZERO_MV | MB_TYPE_16x16,
        MB_TYPE_QUANT | MB_TYPE_FORWARD_MV | MB_TYPE_CBP,
    ];

    let mut ptype_bits = [0u8; 7];
    let mut ptype_codes = [0u16; 7];
    let mut ptype_syms = [0i16; 7];
    for i in 0..7 {
        ptype_codes[i] = TABLE_MB_PTYPE[i][0] as u16;
        ptype_bits[i] = TABLE_MB_PTYPE[i][1];
        ptype_syms[i] = PTYPE2MB_TYPE[i];
    }
    let mb_ptype = Vlc::init_sparse(MB_PTYPE_VLC_BITS, &ptype_bits, &ptype_codes, Some(&ptype_syms));

    const TABLE_MB_BTYPE: [[u8; 2]; 11] = [
        [3, 5],
        [2, 3],
        [3, 3],
        [2, 4],
        [3, 4],
        [2, 2],
        [3, 2],
        [1, 6],
        [2, 6],
        [3, 6],
        [2, 5],
    ];

    const BTYPE2MB_TYPE: [i16; 11] = [
        MB_TYPE_INTRA,
        MB_TYPE_BACKWARD_MV,
        MB_TYPE_BACKWARD_MV | MB_TYPE_CBP,
        MB_TYPE_FORWARD_MV,
        MB_TYPE_FORWARD_MV | MB_TYPE_CBP,
        MB_TYPE_BIDIR_MV,
        MB_TYPE_BIDIR_MV | MB_TYPE_CBP,
        MB_TYPE_QUANT | MB_TYPE_INTRA,
        MB_TYPE_QUANT | MB_TYPE_BACKWARD_MV | MB_TYPE_CBP,
        MB_TYPE_QUANT | MB_TYPE_FORWARD_MV | MB_TYPE_CBP,
        MB_TYPE_QUANT | MB_TYPE_BIDIR_MV | MB_TYPE_CBP,
    ];

    let mut btype_bits = [0u8; 11];
    let mut btype_codes = [0u16; 11];
    let mut btype_syms = [0i16; 11];
    for i in 0..11 {
        btype_codes[i] = TABLE_MB_BTYPE[i][0] as u16;
        btype_bits[i] = TABLE_MB_BTYPE[i][1];
        btype_syms[i] = BTYPE2MB_TYPE[i];
    }
    let mb_btype = Vlc::init_sparse(MB_BTYPE_VLC_BITS, &btype_bits, &btype_codes, Some(&btype_syms));

    let rl_mpeg1 = init_2d_vlc_rl(&FF_MPEG1_VLC_TABLE);
    let rl_mpeg2 = init_2d_vlc_rl(&FF_MPEG2_VLC_TABLE);

    Mpeg12Vlcs { dc_lum, dc_chroma, mv, mbincr, mb_pat, mb_ptype, mb_btype, rl_mpeg1, rl_mpeg2 }
}

pub fn get_vlcs() -> &'static Mpeg12Vlcs {
    VLC_CACHE.get_or_init(build)
}

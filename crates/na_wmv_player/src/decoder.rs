//! VC-1 / WMV9 Macroblock Decoder
//!
//! Full Simple/Main-profile decode path:
//!   • Proper VLC coefficient decoding (intra + inter TCOEF tables)
//!   • Uniform / non-uniform inverse quantization
//!   • VC-1 integer IDCT (8×8, 8×4, 4×8, 4×4)
//!   • Half-pixel motion compensation with bilinear filter
//!   • Overlap smoothing filter (Main profile)
//!   • Reference frame buffer for P/B frames

use crate::bitreader::BitReader;
use crate::error::{DecoderError, Result};
use crate::na_msmpeg4_mv_tables::{
    FF_MSMP4_MV_TABLE0, FF_MSMP4_MV_TABLE0_LENS, FF_MSMP4_MV_TABLE1, FF_MSMP4_MV_TABLE1_LENS,
};
use crate::na_msmpeg4_tables::FF_MB_NON_INTRA_TABLES;
use crate::na_rl_tables::{
    FF_RL_BASES, FF_WMV1_SCANTABLE, FF_WMV2_SCANTABLE_A, FF_WMV2_SCANTABLE_B,
};
use crate::na_simple_idct as ffidct;
use crate::na_wmv2_tables::{FF_MSMP4_DC_TABLES, FF_MSMP4_MB_I_TABLE};
use crate::na_wmv2dsp as wmv2dsp;
use crate::vc1::{FrameType, MvMode, PictureHeader, Profile, SequenceHeader};
use crate::vc1_tables::{
    TT_4X4, TT_4X8, TT_4X8_LEFT, TT_4X8_RIGHT, TT_8X4, TT_8X4_BOTTOM, TT_8X4_TOP, TT_8X8,
    TTBLK_TO_TT, VC1_ZZ_4X4, VC1_ZZ_4X8, VC1_ZZ_8X4, Vc1AcTable, ac_tables, cbpcy_vlcs,
    mvdata_vlcs, subblkpat_vlcs, ttblk_vlcs, ttmb_vlcs,
};
use crate::vlc::{
    SCAN_INTRA, SCAN_VERT, VLC_ESCAPE, VlcTable, ZIGZAG, unpack_rl, wmv2_cbpc_p_vlc, wmv2_cbpy_vlc,
    wmv2_tcoef_inter_vlc, wmv2_tcoef_intra_vlc,
};
use crate::vlc_tree::VlcTree;
use crate::wmv2::{Wmv2FrameHeader, Wmv2FrameType, Wmv2Params};

// ─── Frame buffer ────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct YuvFrame {
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,
    pub cb: Vec<u8>,
    pub cr: Vec<u8>,
}

impl YuvFrame {
    pub fn new(width: u32, height: u32) -> Self {
        let y_sz = (width * height) as usize;
        let uv_sz = y_sz / 4;
        YuvFrame {
            width,
            height,
            y: vec![16u8; y_sz],
            cb: vec![128u8; uv_sz],
            cr: vec![128u8; uv_sz],
        }
    }

    pub fn to_planar_u8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.y.len() + self.cb.len() + self.cr.len());
        out.extend_from_slice(&self.y);
        out.extend_from_slice(&self.cb);
        out.extend_from_slice(&self.cr);
        out
    }

    pub fn clear(&mut self) {
        self.y.fill(16);
        self.cb.fill(128);
        self.cr.fill(128);
    }
}

// ─── VC-1 inverse transforms ─────────────────────────────────────────────────
// These are integer-for-integer translations of FFmpeg libavcodec/vc1dsp.c.
// VC-1 uses distinct 8x8, 8x4, 4x8 and 4x4 transforms; they are not scaled
// variants of a generic IDCT.

pub fn idct8x8(block: &mut [i32; 64]) {
    let src = *block;
    let mut temp = [0i32; 64];
    for i in 0..8 {
        let t1 = 12 * (src[i] + src[i + 32]) + 4;
        let t2 = 12 * (src[i] - src[i + 32]) + 4;
        let t3 = 16 * src[i + 16] + 6 * src[i + 48];
        let t4 = 6 * src[i + 16] - 16 * src[i + 48];
        let t5 = t1 + t3;
        let t6 = t2 + t4;
        let t7 = t2 - t4;
        let t8 = t1 - t3;
        let o1 = 16 * src[i + 8] + 15 * src[i + 24] + 9 * src[i + 40] + 4 * src[i + 56];
        let o2 = 15 * src[i + 8] - 4 * src[i + 24] - 16 * src[i + 40] - 9 * src[i + 56];
        let o3 = 9 * src[i + 8] - 16 * src[i + 24] + 4 * src[i + 40] + 15 * src[i + 56];
        let o4 = 4 * src[i + 8] - 9 * src[i + 24] + 15 * src[i + 40] - 16 * src[i + 56];
        let d = i * 8;
        temp[d] = (t5 + o1) >> 3;
        temp[d + 1] = (t6 + o2) >> 3;
        temp[d + 2] = (t7 + o3) >> 3;
        temp[d + 3] = (t8 + o4) >> 3;
        temp[d + 4] = (t8 - o4) >> 3;
        temp[d + 5] = (t7 - o3) >> 3;
        temp[d + 6] = (t6 - o2) >> 3;
        temp[d + 7] = (t5 - o1) >> 3;
    }
    for i in 0..8 {
        let t1 = 12 * (temp[i] + temp[i + 32]) + 64;
        let t2 = 12 * (temp[i] - temp[i + 32]) + 64;
        let t3 = 16 * temp[i + 16] + 6 * temp[i + 48];
        let t4 = 6 * temp[i + 16] - 16 * temp[i + 48];
        let t5 = t1 + t3;
        let t6 = t2 + t4;
        let t7 = t2 - t4;
        let t8 = t1 - t3;
        let o1 = 16 * temp[i + 8] + 15 * temp[i + 24] + 9 * temp[i + 40] + 4 * temp[i + 56];
        let o2 = 15 * temp[i + 8] - 4 * temp[i + 24] - 16 * temp[i + 40] - 9 * temp[i + 56];
        let o3 = 9 * temp[i + 8] - 16 * temp[i + 24] + 4 * temp[i + 40] + 15 * temp[i + 56];
        let o4 = 4 * temp[i + 8] - 9 * temp[i + 24] + 15 * temp[i + 40] - 16 * temp[i + 56];
        block[i] = (t5 + o1) >> 7;
        block[i + 8] = (t6 + o2) >> 7;
        block[i + 16] = (t7 + o3) >> 7;
        block[i + 24] = (t8 + o4) >> 7;
        block[i + 32] = (t8 - o4 + 1) >> 7;
        block[i + 40] = (t7 - o3 + 1) >> 7;
        block[i + 48] = (t6 - o2 + 1) >> 7;
        block[i + 56] = (t5 - o1 + 1) >> 7;
    }
}

fn inv_trans_8x4_part(block: &mut [i32; 64], row0: usize) {
    let mut src = [0i32; 32];
    for r in 0..4 {
        for c in 0..8 {
            src[r * 8 + c] = block[(row0 + r) * 8 + c];
        }
    }
    for r in 0..4 {
        let o = r * 8;
        let t1 = 12 * (src[o] + src[o + 4]) + 4;
        let t2 = 12 * (src[o] - src[o + 4]) + 4;
        let t3 = 16 * src[o + 2] + 6 * src[o + 6];
        let t4 = 6 * src[o + 2] - 16 * src[o + 6];
        let t5 = t1 + t3;
        let t6 = t2 + t4;
        let t7 = t2 - t4;
        let t8 = t1 - t3;
        let a = 16 * src[o + 1] + 15 * src[o + 3] + 9 * src[o + 5] + 4 * src[o + 7];
        let b = 15 * src[o + 1] - 4 * src[o + 3] - 16 * src[o + 5] - 9 * src[o + 7];
        let c = 9 * src[o + 1] - 16 * src[o + 3] + 4 * src[o + 5] + 15 * src[o + 7];
        let d = 4 * src[o + 1] - 9 * src[o + 3] + 15 * src[o + 5] - 16 * src[o + 7];
        src[o] = (t5 + a) >> 3;
        src[o + 1] = (t6 + b) >> 3;
        src[o + 2] = (t7 + c) >> 3;
        src[o + 3] = (t8 + d) >> 3;
        src[o + 4] = (t8 - d) >> 3;
        src[o + 5] = (t7 - c) >> 3;
        src[o + 6] = (t6 - b) >> 3;
        src[o + 7] = (t5 - a) >> 3;
    }
    for c in 0..8 {
        let t1 = 17 * (src[c] + src[c + 16]) + 64;
        let t2 = 17 * (src[c] - src[c + 16]) + 64;
        let t3 = 22 * src[c + 8] + 10 * src[c + 24];
        let t4 = 22 * src[c + 24] - 10 * src[c + 8];
        block[row0 * 8 + c] = (t1 + t3) >> 7;
        block[(row0 + 1) * 8 + c] = (t2 - t4) >> 7;
        block[(row0 + 2) * 8 + c] = (t2 + t4) >> 7;
        block[(row0 + 3) * 8 + c] = (t1 - t3) >> 7;
    }
}

fn inv_trans_4x8_part(block: &mut [i32; 64], col0: usize) {
    let mut src = [0i32; 32];
    for r in 0..8 {
        for c in 0..4 {
            src[r * 4 + c] = block[r * 8 + col0 + c];
        }
    }
    for r in 0..8 {
        let o = r * 4;
        let t1 = 17 * (src[o] + src[o + 2]) + 4;
        let t2 = 17 * (src[o] - src[o + 2]) + 4;
        let t3 = 22 * src[o + 1] + 10 * src[o + 3];
        let t4 = 22 * src[o + 3] - 10 * src[o + 1];
        src[o] = (t1 + t3) >> 3;
        src[o + 1] = (t2 - t4) >> 3;
        src[o + 2] = (t2 + t4) >> 3;
        src[o + 3] = (t1 - t3) >> 3;
    }
    for c in 0..4 {
        let t1 = 12 * (src[c] + src[c + 16]) + 64;
        let t2 = 12 * (src[c] - src[c + 16]) + 64;
        let t3 = 16 * src[c + 8] + 6 * src[c + 24];
        let t4 = 6 * src[c + 8] - 16 * src[c + 24];
        let t5 = t1 + t3;
        let t6 = t2 + t4;
        let t7 = t2 - t4;
        let t8 = t1 - t3;
        let a = 16 * src[c + 4] + 15 * src[c + 12] + 9 * src[c + 20] + 4 * src[c + 28];
        let b = 15 * src[c + 4] - 4 * src[c + 12] - 16 * src[c + 20] - 9 * src[c + 28];
        let d = 9 * src[c + 4] - 16 * src[c + 12] + 4 * src[c + 20] + 15 * src[c + 28];
        let e = 4 * src[c + 4] - 9 * src[c + 12] + 15 * src[c + 20] - 16 * src[c + 28];
        block[col0 + c] = (t5 + a) >> 7;
        block[8 + col0 + c] = (t6 + b) >> 7;
        block[16 + col0 + c] = (t7 + d) >> 7;
        block[24 + col0 + c] = (t8 + e) >> 7;
        block[32 + col0 + c] = (t8 - e + 1) >> 7;
        block[40 + col0 + c] = (t7 - d + 1) >> 7;
        block[48 + col0 + c] = (t6 - b + 1) >> 7;
        block[56 + col0 + c] = (t5 - a + 1) >> 7;
    }
}

fn inv_trans_4x4_part(block: &mut [i32; 64], row0: usize, col0: usize) {
    let mut src = [0i32; 16];
    for r in 0..4 {
        for c in 0..4 {
            src[r * 4 + c] = block[(row0 + r) * 8 + col0 + c];
        }
    }
    for r in 0..4 {
        let o = r * 4;
        let t1 = 17 * (src[o] + src[o + 2]) + 4;
        let t2 = 17 * (src[o] - src[o + 2]) + 4;
        let t3 = 22 * src[o + 1] + 10 * src[o + 3];
        let t4 = 22 * src[o + 3] - 10 * src[o + 1];
        src[o] = (t1 + t3) >> 3;
        src[o + 1] = (t2 - t4) >> 3;
        src[o + 2] = (t2 + t4) >> 3;
        src[o + 3] = (t1 - t3) >> 3;
    }
    for c in 0..4 {
        let t1 = 17 * (src[c] + src[c + 8]) + 64;
        let t2 = 17 * (src[c] - src[c + 8]) + 64;
        let t3 = 22 * src[c + 4] + 10 * src[c + 12];
        let t4 = 22 * src[c + 12] - 10 * src[c + 4];
        block[row0 * 8 + col0 + c] = (t1 + t3) >> 7;
        block[(row0 + 1) * 8 + col0 + c] = (t2 - t4) >> 7;
        block[(row0 + 2) * 8 + col0 + c] = (t2 + t4) >> 7;
        block[(row0 + 3) * 8 + col0 + c] = (t1 - t3) >> 7;
    }
}

pub fn apply_idct(block: &mut [i32; 64], tt: u8) {
    use crate::vc1_tables::*;
    match tt {
        TT_8X8 => idct8x8(block),
        TT_8X4 | TT_8X4_TOP | TT_8X4_BOTTOM => {
            if tt != TT_8X4_BOTTOM {
                inv_trans_8x4_part(block, 0);
            }
            if tt != TT_8X4_TOP {
                inv_trans_8x4_part(block, 4);
            }
        }
        TT_4X8 | TT_4X8_LEFT | TT_4X8_RIGHT => {
            if tt != TT_4X8_RIGHT {
                inv_trans_4x8_part(block, 0);
            }
            if tt != TT_4X8_LEFT {
                inv_trans_4x8_part(block, 4);
            }
        }
        TT_4X4 => {
            inv_trans_4x4_part(block, 0, 0);
            inv_trans_4x4_part(block, 0, 4);
            inv_trans_4x4_part(block, 4, 0);
            inv_trans_4x4_part(block, 4, 4);
        }
        _ => idct8x8(block),
    }
}

#[inline(always)]
fn vc1_transpose_scan_index(index: usize) -> usize {
    (index >> 3) | ((index & 7) << 3)
}

#[inline(always)]
fn vc1_scan_index(scan: &[usize], pos: usize, res_fasttx: bool) -> usize {
    let index = scan[pos];
    if res_fasttx {
        vc1_transpose_scan_index(index)
    } else {
        index
    }
}

/// FFmpeg switches the inverse-transform family for WMV3 Simple/Main at
/// decoder initialization time.  When RES_FASTTX=0 it uses the legacy simple
/// IDCT callbacks; when RES_FASTTX=1 it keeps the VC-1 integer transforms.
/// Keep that selection explicit instead of treating RES_FASTTX as a bitstream
/// parsing-only flag.
fn apply_wmv3_idct(block: &mut [i32; 64], tt: u8, res_fasttx: bool) {
    if res_fasttx {
        apply_idct(block, tt);
        return;
    }

    let mut b = [0i16; 64];
    for (dst, src) in b.iter_mut().zip(block.iter()) {
        *dst = *src as i16;
    }

    match tt {
        TT_8X8 => ffidct::ff_simple_idct_int16_8bit(&mut b),
        TT_8X4 | TT_8X4_TOP | TT_8X4_BOTTOM => {
            if tt != TT_8X4_BOTTOM {
                ffidct::ff_simple_idct84_int16_8bit(&mut b, 0);
            }
            if tt != TT_8X4_TOP {
                ffidct::ff_simple_idct84_int16_8bit(&mut b, 4);
            }
        }
        TT_4X8 | TT_4X8_LEFT | TT_4X8_RIGHT => {
            if tt != TT_4X8_RIGHT {
                ffidct::ff_simple_idct48_int16_8bit(&mut b, 0);
            }
            if tt != TT_4X8_LEFT {
                ffidct::ff_simple_idct48_int16_8bit(&mut b, 4);
            }
        }
        TT_4X4 => {
            ffidct::ff_simple_idct44_int16_8bit(&mut b, 0, 0);
            ffidct::ff_simple_idct44_int16_8bit(&mut b, 0, 4);
            ffidct::ff_simple_idct44_int16_8bit(&mut b, 4, 0);
            ffidct::ff_simple_idct44_int16_8bit(&mut b, 4, 4);
        }
        _ => ffidct::ff_simple_idct_int16_8bit(&mut b),
    }

    for (dst, src) in block.iter_mut().zip(b.iter()) {
        *dst = *src as i32;
    }
}

// ─── Inverse quantization ────────────────────────────────────────────────────
// SMPTE 421M §8.1.4.  Two modes: uniform and non-uniform.

fn iquant_uniform(level: i32, pquant: i32, halfqp: bool) -> i32 {
    if level == 0 {
        return 0;
    }
    let step = 2 * pquant;
    let base = step * level.abs() + pquant;
    let delta = if halfqp { pquant } else { 0 };
    let result = if level > 0 {
        base + delta
    } else {
        -(base + delta)
    };
    result.clamp(-2048, 2047)
}

fn iquant_nonuniform(level: i32, pquant: i32) -> i32 {
    if level == 0 {
        return 0;
    }
    // Non-uniform quantizer step table from SMPTE 421M Table 3
    const STEP: [i32; 32] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        27, 29, 31, 33, 35, 37, 63,
    ];
    let step = STEP[(pquant as usize).min(31)];
    let result = step * level.abs() + pquant;
    if level > 0 {
        result.clamp(-2048, 2047)
    } else {
        (-result).clamp(-2048, 2047)
    }
}

// ─── DC step-size tables (SMPTE 421M Table 3) ───────────────────────────────
// Indexed by pquant (0 unused, 1..31).
// Luma and chroma have separate tables.
// The value is multiplied by 128 here to match the IDCT normalization domain
// (IDCT output = input / 128, so DC_recon must be in the ×128 domain).

const DC_STEP_LUMA: [i32; 32] = [
    0, // 0: unused
    128, 256, 384, 512, 640, 768, 896, 1024, // pquant 1-8:  step = pquant
    1152, 1280, 1408, 1536, 1664, 1792, 1920, 2048, // 9-16
    2176, 2304, 2432, 2560, 2688, 2816, 2944, 3072, // 17-24
    3328, 3584, 3840, 4096, 4352, 4608, 8192, // 25-31
];

const DC_STEP_CHROMA: [i32; 32] = [
    0, // 0: unused
    128, 128, 128, 256, 256, 384, 384, 512, // pquant 1-8
    512, 640, 640, 768, 768, 896, 896, 1024, // 9-16
    1024, 1152, 1152, 1280, 1280, 1408, 1408, 1536, // 17-24
    1664, 1792, 1920, 2048, 2176, 2304, 4096, // 25-31
];

#[inline]
fn dc_step(pquant: i32, is_luma: bool) -> i32 {
    let idx = pquant.clamp(1, 31) as usize;
    if is_luma {
        DC_STEP_LUMA[idx]
    } else {
        DC_STEP_CHROMA[idx]
    }
}

// ─── VC-1 loop filter (Simple/Main) ────────────────────────────────────────
// Ported from FFmpeg libavcodec/vc1dsp.c / vc1_loopfilter.c.
//
// VC-1 does not use the WMV1/2 four-tap delta filter here.  The normative
// filter first evaluates a0/a1/a2 against the picture quantizer, then applies
// a clipped correction to the two samples straddling the edge.  Edges are
// processed in groups of four lines: line 2 is the gate for the other three.

#[inline(always)]
fn vc1_filter_line(plane: &mut [u8], src: usize, stride: isize, pq: i32) -> bool {
    #[inline(always)]
    fn at(plane: &[u8], src: usize, stride: isize, n: isize) -> i32 {
        plane[(src as isize + n * stride) as usize] as i32
    }

    let mut a0 = (2 * (at(plane, src, stride, -2) - at(plane, src, stride, 1))
        - 5 * (at(plane, src, stride, -1) - at(plane, src, stride, 0))
        + 4)
        >> 3;
    let a0_sign = a0 >> 31;
    a0 = (a0 ^ a0_sign) - a0_sign;

    if a0 < pq {
        let a1 = ((2 * (at(plane, src, stride, -4) - at(plane, src, stride, -1))
            - 5 * (at(plane, src, stride, -3) - at(plane, src, stride, -2))
            + 4)
            >> 3)
            .abs();
        let a2 = ((2 * (at(plane, src, stride, 0) - at(plane, src, stride, 3))
            - 5 * (at(plane, src, stride, 1) - at(plane, src, stride, 2))
            + 4)
            >> 3)
            .abs();
        if a1 < a0 || a2 < a0 {
            let mut clip = at(plane, src, stride, -1) - at(plane, src, stride, 0);
            let clip_sign = clip >> 31;
            clip = ((clip ^ clip_sign) - clip_sign) >> 1;
            if clip != 0 {
                // FFmpeg vc1_filter_line(): d is based on (a0 - a3), and
                // the correction is applied only when a0 and the edge delta
                // have opposite signs.  The previous Rust port reversed the
                // subtraction, converted d to an absolute value, and inverted
                // the sign test.
                let a3 = a1.min(a2);
                let mut d = (5 * (a0 - a3)) >> 3;
                if (a0_sign ^ clip_sign) != 0 {
                    d = d.min(clip);
                    d = (d ^ clip_sign) - clip_sign;
                    let l = (src as isize - stride) as usize;
                    let r = src;
                    plane[l] = (plane[l] as i32 - d).clamp(0, 255) as u8;
                    plane[r] = (plane[r] as i32 + d).clamp(0, 255) as u8;
                }
                return true;
            }
        }
    }
    false
}

/// FFmpeg vc1_loop_filter(): `src` points at the first pixel on the positive
/// side of the edge. `step` walks along the edge and `stride` crosses it.
fn vc1_loop_filter_edge(
    plane: &mut [u8],
    src: usize,
    step: isize,
    stride: isize,
    len: usize,
    pq: i32,
) {
    let mut i = 0usize;
    while i + 3 < len {
        let p2 = (src as isize + (i as isize + 2) * step) as usize;
        if vc1_filter_line(plane, p2, stride, pq) {
            for off in [0isize, 1, 3] {
                let p = (src as isize + (i as isize + off) * step) as usize;
                vc1_filter_line(plane, p, stride, pq);
            }
        }
        i += 4;
    }
}

#[inline]
fn vc1_filter_horizontal_boundary(
    plane: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    len: usize,
    pq: i32,
) {
    if y < 4 || y + 3 >= height || x >= width {
        return;
    }
    let len = len.min(width - x) & !3;
    if len >= 4 {
        vc1_loop_filter_edge(plane, y * width + x, 1, width as isize, len, pq);
    }
}

#[inline]
fn vc1_filter_vertical_boundary(
    plane: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    len: usize,
    pq: i32,
) {
    if x < 4 || x + 3 >= width || y >= height {
        return;
    }
    let len = len.min(height - y) & !3;
    if len >= 4 {
        vc1_loop_filter_edge(plane, y * width + x, width as isize, 1, len, pq);
    }
}

/// Filter the horizontal borders owned by one progressive I/BI/B macroblock.
///
/// This is the operation FFmpeg calls the "V loop" (the filter runs in the
/// vertical direction across a horizontal edge).  Outer picture edges are not
/// filtered; the internal 8-pixel luma edge is always filtered.
fn vc1_i_v_loop_filter_mb(frame: &mut YuvFrame, mb_row: usize, mb_col: usize, pq: i32) {
    let w = frame.width as usize;
    let h = frame.height as usize;
    let x = mb_col * 16;
    let y = mb_row * 16;

    if y > 0 {
        vc1_filter_horizontal_boundary(&mut frame.y, w, h, x, y, 16, pq);
    }
    if y + 8 < h {
        vc1_filter_horizontal_boundary(&mut frame.y, w, h, x, y + 8, 16, pq);
    }

    let cw = w / 2;
    let ch = h / 2;
    let cx = mb_col * 8;
    let cy = mb_row * 8;
    if cy > 0 {
        vc1_filter_horizontal_boundary(&mut frame.cb, cw, ch, cx, cy, 8, pq);
        vc1_filter_horizontal_boundary(&mut frame.cr, cw, ch, cx, cy, 8, pq);
    }
}

/// Filter the vertical borders owned by one progressive I/BI/B macroblock.
///
/// FFmpeg calls this the "H loop".  It must run after the V loop for the same
/// macroblock because the integer filter changes samples at edge crossings.
fn vc1_i_h_loop_filter_mb(frame: &mut YuvFrame, mb_row: usize, mb_col: usize, pq: i32) {
    let w = frame.width as usize;
    let h = frame.height as usize;
    let x = mb_col * 16;
    let y = mb_row * 16;

    if x > 0 {
        vc1_filter_vertical_boundary(&mut frame.y, w, h, x, y, 16, pq);
    }
    if x + 8 < w {
        vc1_filter_vertical_boundary(&mut frame.y, w, h, x + 8, y, 16, pq);
    }

    let cw = w / 2;
    let ch = h / 2;
    let cx = mb_col * 8;
    let cy = mb_row * 8;
    if cx > 0 {
        vc1_filter_vertical_boundary(&mut frame.cb, cw, ch, cx, cy, 8, pq);
        vc1_filter_vertical_boundary(&mut frame.cr, cw, ch, cx, cy, 8, pq);
    }
}

/// Reproduce the call order of FFmpeg `ff_vc1_i_loop_filter()` after the full
/// progressive picture has been reconstructed.
///
/// It is tempting to run every horizontal edge and then every vertical edge,
/// but that is not equivalent: FFmpeg deliberately pipelines the two filters
/// behind overlap reconstruction.  At edge crossings the first filter changes
/// pixels consumed by the second, so preserving this macroblock wavefront is
/// required for bit-exact reconstruction.
fn vc1_apply_i_loop_filter(frame: &mut YuvFrame, pq: i32, mb_width: usize, mb_height: usize) {
    if mb_width == 0 || mb_height == 0 {
        return;
    }

    for cur_y in 0..mb_height {
        for cur_x in 0..mb_width {
            // ff_vc1_i_loop_filter(): V loop trails overlap by one row and
            // one column, with an explicit last-row/last-column flush.
            if cur_y >= 1 {
                if cur_x >= 1 {
                    vc1_i_v_loop_filter_mb(frame, cur_y - 1, cur_x - 1, pq);
                }
                if cur_x + 1 == mb_width {
                    vc1_i_v_loop_filter_mb(frame, cur_y - 1, cur_x, pq);
                }
            }
            if cur_y + 1 == mb_height {
                if cur_x >= 1 {
                    vc1_i_v_loop_filter_mb(frame, cur_y, cur_x - 1, pq);
                }
                if cur_x + 1 == mb_width {
                    vc1_i_v_loop_filter_mb(frame, cur_y, cur_x, pq);
                }
            }

            // H loop trails the V loop by another row.  Keep the exact
            // wavefront/flush order used by FFmpeg instead of globally
            // regrouping filters by orientation.
            if cur_y >= 2 {
                if cur_x >= 1 {
                    vc1_i_h_loop_filter_mb(frame, cur_y - 2, cur_x - 1, pq);
                }
                if cur_x + 1 == mb_width {
                    vc1_i_h_loop_filter_mb(frame, cur_y - 2, cur_x, pq);
                }
            }
            if cur_y + 1 == mb_height {
                if cur_y >= 1 {
                    if cur_x >= 1 {
                        vc1_i_h_loop_filter_mb(frame, cur_y - 1, cur_x - 1, pq);
                    }
                    if cur_x + 1 == mb_width {
                        vc1_i_h_loop_filter_mb(frame, cur_y - 1, cur_x, pq);
                    }
                }
                if cur_x >= 1 {
                    vc1_i_h_loop_filter_mb(frame, cur_y, cur_x - 1, pq);
                }
                if cur_x + 1 == mb_width {
                    vc1_i_h_loop_filter_mb(frame, cur_y, cur_x, pq);
                }
            }
        }
    }
}

// ─── Coefficient decoder ─────────────────────────────────────────────────────

/// Read raw DC differential (before prediction).
/// Returns the signed differential value (NOT yet scaled / predicted).
fn read_dc_diff(br: &mut BitReader<'_>, dc_vlc: &VlcTable) -> i32 {
    let dc_size = match dc_vlc.decode(br) {
        Some(s @ 0..) => s as u8,
        _ => return 0,
    };
    if dc_size == 0 {
        return 0;
    }
    let raw = br.read_bits(dc_size).unwrap_or(0) as i32;
    // MSB=0 → negative (one's complement offset per SMPTE 421M §8.1.4.4)
    if raw & (1 << (dc_size - 1)) != 0 {
        raw
    } else {
        raw - (1 << dc_size) + 1
    }
}

// ─── DC Prediction buffer ────────────────────────────────────────────────────
// SMPTE 421M §8.1.4.6.
//
// For each macroblock position we store the reconstructed (post-IDCT) DC
// value for each of the 6 blocks (Y0 Y1 Y2 Y3 Cb Cr) in the "DC scale"
// domain (i.e. the integer value before the final /8 normalisation step).
//
// Prediction direction is chosen per-block by comparing the gradient
// magnitudes of the left and top neighbours.

#[derive(Clone)]
pub struct DcPredBuffer {
    mb_w: usize,
    /// Stored as reconstructed DC * 8 / pquant to stay in coeff domain.
    /// Layout: [mb_row * mb_w + mb_col][blk 0..6]
    dc: Vec<[i32; 6]>,
}

impl DcPredBuffer {
    pub fn new(mb_w: usize, mb_h: usize) -> Self {
        DcPredBuffer {
            mb_w,
            dc: vec![[1024i32; 6]; mb_w * mb_h],
        }
    }

    /// Return the predicted DC for block `blk` at (mb_row, mb_col).
    /// Also decides prediction direction (horizontal vs vertical).
    /// Returns (pred_value, use_left: bool).
    pub fn predict(&self, mb_row: usize, mb_col: usize, blk: usize) -> (i32, bool) {
        // Neighbour block positions in the DC grid (SMPTE 421M Fig. 8-4)
        // For luma: blocks are arranged as:
        //   0 1
        //   2 3
        // Left-of-block:  blk 0←left_mb.blk1,  blk 1←same_mb.blk0,
        //                  blk 2←left_mb.blk3,  blk 3←same_mb.blk2
        // Top-of-block:   blk 0←top_mb.blk2,   blk 1←top_mb.blk3,
        //                  blk 2←same_mb.blk0,  blk 3←same_mb.blk1
        // Chroma (blk 4/5): left = left_mb.same_blk, top = top_mb.same_blk
        let (dc_left, dc_top, dc_topleft) = self.dc_neighbours(mb_row, mb_col, blk);

        // Gradient: |A - C| (horizontal) vs |B - C| (vertical)
        // A = left, B = top, C = top-left
        let grad_h = (dc_left - dc_topleft).unsigned_abs();
        let grad_v = (dc_top - dc_topleft).unsigned_abs();

        if grad_v <= grad_h {
            // Predict from top (vertical predictor)
            (dc_top, false)
        } else {
            // Predict from left (horizontal predictor)
            (dc_left, true)
        }
    }

    fn dc_neighbours(&self, mb_row: usize, mb_col: usize, blk: usize) -> (i32, i32, i32) {
        // Helper: get stored DC for a possibly-out-of-bounds MB/blk
        let get = |r: isize, c: isize, b: usize| -> i32 {
            if r < 0 || c < 0 {
                return 1024;
            } // mid-gray default
            let idx = r as usize * self.mb_w + c as usize;
            if idx >= self.dc.len() {
                return 1024;
            }
            self.dc[idx][b]
        };

        let r = mb_row as isize;
        let c = mb_col as isize;

        match blk {
            // ── luma ───────────────────────────────────────────────────────
            0 => {
                let left = get(r, c - 1, 1); // right half of left MB
                let top = get(r - 1, c, 2); // bottom-left of top MB
                let topleft = get(r - 1, c - 1, 3);
                (left, top, topleft)
            }
            1 => {
                // SMPTE 421M §8.1.4.6 Fig 8-4: topleft = blk3 of top-left MB
                let left = get(r, c, 0);
                let top = get(r - 1, c, 3);
                let topleft = get(r - 1, c - 1, 3);
                (left, top, topleft)
            }
            2 => {
                let left = get(r, c - 1, 3);
                let top = get(r, c, 0); // blk0 of same MB
                let topleft = get(r, c - 1, 1);
                (left, top, topleft)
            }
            3 => {
                let left = get(r, c, 2);
                let top = get(r, c, 1);
                let topleft = get(r, c, 0);
                (left, top, topleft)
            }
            // ── chroma ─────────────────────────────────────────────────────
            _ => {
                let left = get(r, c - 1, blk);
                let top = get(r - 1, c, blk);
                let topleft = get(r - 1, c - 1, blk);
                (left, top, topleft)
            }
        }
    }

    /// Store the reconstructed DC value (in coeff domain) for later prediction.
    pub fn store(&mut self, mb_row: usize, mb_col: usize, blk: usize, dc_recon: i32) {
        let idx = mb_row * self.mb_w + mb_col;
        if idx < self.dc.len() {
            self.dc[idx][blk] = dc_recon;
        }
    }
}

// ─── WMV2/MSMPEG4 DC predictor (upstream ff_msmpeg4_pred_dc logic) ─────────────

/// Predictor storage is in "scaled DC coefficient" domain (level * dc_scale).
/// Default value 1024 corresponds to mid-gray (128) with scale=8.
pub struct Wmv2DcPredBuffer {
    mb_w: usize,
    dc: Vec<[i32; 6]>,
}

impl Wmv2DcPredBuffer {
    pub fn new(mb_w: usize, mb_h: usize) -> Self {
        Wmv2DcPredBuffer {
            mb_w,
            dc: vec![[1024i32; 6]; mb_w * mb_h],
        }
    }

    fn neighbours(&self, mb_row: usize, mb_col: usize, blk: usize) -> (i32, i32, i32) {
        // upstream alignment note:
        //   ff_msmpeg4_pred_dc() predicts in the *8x8 block grid* (block_index[n]) with
        //   neighbours A(left), B(above-left), C(above):
        //       B C
        //       A X
        // Our storage is per-macroblock ([i32; 6]), so we emulate upstream's block-grid
        // addressing for luma blocks (0..3) and macroblock-grid for chroma (4..5).

        #[inline(always)]
        fn get_luma(dc: &[[i32; 6]], mb_w: usize, bx: isize, by: isize) -> i32 {
            if bx < 0 || by < 0 {
                return 1024;
            }
            let mb_x = (bx >> 1) as usize;
            let mb_y = (by >> 1) as usize;
            let idx = mb_y * mb_w + mb_x;
            if idx >= dc.len() {
                return 1024;
            }
            let sub_x = (bx & 1) as usize;
            let sub_y = (by & 1) as usize;
            let b = (sub_y << 1) | sub_x; // 0..3
            dc[idx][b]
        }

        #[inline(always)]
        fn get_chroma(dc: &[[i32; 6]], mb_w: usize, mb_x: isize, mb_y: isize, blk: usize) -> i32 {
            if mb_x < 0 || mb_y < 0 {
                return 1024;
            }
            let idx = (mb_y as usize) * mb_w + (mb_x as usize);
            if idx >= dc.len() {
                return 1024;
            }
            dc[idx][blk]
        }

        if blk < 4 {
            let bx = (mb_col as isize) * 2 + ((blk & 1) as isize);
            let by = (mb_row as isize) * 2 + ((blk >> 1) as isize);
            let a = get_luma(&self.dc, self.mb_w, bx - 1, by);
            let b = get_luma(&self.dc, self.mb_w, bx - 1, by - 1);
            let c = get_luma(&self.dc, self.mb_w, bx, by - 1);
            (a, b, c)
        } else {
            let mx = mb_col as isize;
            let my = mb_row as isize;
            let a = get_chroma(&self.dc, self.mb_w, mx - 1, my, blk);
            let b = get_chroma(&self.dc, self.mb_w, mx - 1, my - 1, blk);
            let c = get_chroma(&self.dc, self.mb_w, mx, my - 1, blk);
            (a, b, c)
        }
    }

    /// Returns (pred_level, dir). dir=0 => left, dir=1 => top.
    pub fn predict(&self, mb_row: usize, mb_col: usize, blk: usize, scale: i32) -> (i32, i32) {
        let (a0, b0, c0) = self.neighbours(mb_row, mb_col, blk);
        // Convert from scaled DC to level domain with rounding: (x + scale/2) / scale.
        let a = (a0 + (scale >> 1)) / scale;
        let b = (b0 + (scale >> 1)) / scale;
        let c = (c0 + (scale >> 1)) / scale;

        // WMV2/MSMPEG4 version > V3 uses STRICT '<' (see upstream ff_msmpeg4_pred_dc).
        if (a - b).abs() < (b - c).abs() {
            (c, 1)
        } else {
            (a, 0)
        }
    }

    pub fn store(&mut self, mb_row: usize, mb_col: usize, blk: usize, dc_coeff_scaled: i32) {
        let idx = mb_row * self.mb_w + mb_col;
        if idx < self.dc.len() {
            self.dc[idx][blk] = dc_coeff_scaled;
        }
    }
}

// ─── AC escape decoder ───────────────────────────────────────────────────────
// SMPTE 421M §8.1.4.5 — Three escape modes following VLC_ESCAPE sentinel.
//
//   After VLC_ESCAPE, read mode bits:
//     "0"  → Mode 1: level offset
//     "10" → Mode 2: run offset
//     "11" → Mode 3: absolute fixed-length
//
// Returns (run, signed_level, last).
#[inline]
fn decode_escape_coeff(br: &mut BitReader<'_>, ac_vlc: &VlcTable) -> (u8, i32, bool) {
    let mode = {
        let b0 = br.read_bit().unwrap_or(false);
        if !b0 {
            1u8
        } else {
            let b1 = br.read_bit().unwrap_or(false);
            if b1 { 3 } else { 2 }
        }
    };
    match mode {
        1 => {
            // Mode 1: level offset — VLC gives (run, base_level, last)
            let sym = ac_vlc.decode(br).unwrap_or(0);
            if sym == VLC_ESCAPE {
                return (0, 0, true);
            }
            let (run, base_level, last) = unpack_rl(sym);
            let sign = br.read_bit().unwrap_or(false);
            let offset = ac_vlc.max_level(run as usize, last) as i32 + 1;
            let level = base_level as i32 + offset;
            (run, if sign { -level } else { level }, last)
        }
        2 => {
            // Mode 2: run offset — VLC gives (base_run, level, last)
            let sym = ac_vlc.decode(br).unwrap_or(0);
            if sym == VLC_ESCAPE {
                return (0, 0, true);
            }
            let (base_run, level, last) = unpack_rl(sym);
            let sign = br.read_bit().unwrap_or(false);
            let offset = ac_vlc.max_run(level as usize, last) as i32 + 1;
            let run = (base_run as i32 + offset).min(63) as u8;
            let sl = level as i32;
            (run, if sign { -sl } else { sl }, last)
        }
        _ => {
            // Mode 3: absolute — 1-bit LAST + 6-bit RUN + 8-bit |LEVEL| + 1-bit SIGN
            let last = br.read_bit().unwrap_or(false);
            let run = br.read_bits(6).unwrap_or(0) as u8;
            let level = br.read_bits(8).unwrap_or(1).max(1) as i32;
            let sign = br.read_bit().unwrap_or(false);
            (run, if sign { -level } else { level }, last)
        }
    }
}

/// Decode one 8×8 block of AC+DC coefficients.
/// `is_intra`: use intra table / scan, otherwise inter.
/// `is_luma`:  use luma DC VLC.
/// Returns filled `[i32; 64]` in natural order (not zigzag).
fn decode_block(
    br: &mut BitReader<'_>,
    is_intra: bool,
    is_luma: bool,
    pquant: i32,
    halfqp: bool,
    uniform: bool,
    tt: u8,
    dc_luma: &VlcTable,
    dc_chroma: &VlcTable,
    ac_intra: &VlcTable,
    ac_inter: &VlcTable,
) -> [i32; 64] {
    let mut blk = [0i32; 64];
    let scan: &[usize; 64] = if is_intra { &SCAN_INTRA } else { &ZIGZAG };

    // DC coefficient (intra only)
    if is_intra {
        let dc_vlc = if is_luma { dc_luma } else { dc_chroma };
        blk[0] = read_dc_diff(br, dc_vlc);
    }

    // AC coefficients
    let ac_vlc = if is_intra { ac_intra } else { ac_inter };
    let mut idx = if is_intra { 1usize } else { 0 };

    while let Some(sym) = ac_vlc.decode(br) {
        let (run, signed_level, last) = if sym == VLC_ESCAPE {
            decode_escape_coeff(br, ac_vlc)
        } else {
            let (r, l, last) = unpack_rl(sym);
            let sign = br.read_bit().unwrap_or(false);
            (r, if sign { -(l as i32) } else { l as i32 }, last)
        };

        idx += run as usize;
        if idx >= 64 {
            break;
        }

        let mag = signed_level.abs();
        let qval = if uniform {
            iquant_uniform(mag, pquant, halfqp)
        } else {
            iquant_nonuniform(mag, pquant)
        };
        let signed_val = if signed_level < 0 { -qval } else { qval };

        // Use appropriate scan based on transform type
        let scan_order: &[usize; 64] = match tt {
            3 | 4 => &SCAN_VERT,
            _ => scan,
        };
        let pos = scan_order.get(idx).copied().unwrap_or(idx);
        blk[pos] = signed_val;
        idx += 1;

        if last || br.is_empty() {
            break;
        }
    }

    blk
}

/// Decode only the AC coefficients of one intra block (DC is handled separately).
fn decode_block_ac(
    br: &mut BitReader<'_>,
    _is_luma: bool,
    pquant: i32,
    halfqp: bool,
    uniform: bool,
    tt: u8,
    ac_vlc: &VlcTable,
) -> [i32; 64] {
    let mut blk = [0i32; 64];
    let mut idx = 1usize; // start at 1, skip DC slot

    while let Some(sym) = ac_vlc.decode(br) {
        let (run, signed_level, last) = if sym == VLC_ESCAPE {
            decode_escape_coeff(br, ac_vlc)
        } else {
            let (r, l, last) = unpack_rl(sym);
            let sign = br.read_bit().unwrap_or(false);
            (r, if sign { -(l as i32) } else { l as i32 }, last)
        };

        idx += run as usize;
        if idx >= 64 {
            break;
        }

        let mag = signed_level.abs();
        let qval = if uniform {
            iquant_uniform(mag, pquant, halfqp)
        } else {
            iquant_nonuniform(mag, pquant)
        };
        let sval = if signed_level < 0 { -qval } else { qval };

        let scan_order: &[usize; 64] = match tt {
            3 | 4 => &SCAN_VERT,
            _ => &SCAN_INTRA,
        };
        let pos = scan_order.get(idx).copied().unwrap_or(idx);
        blk[pos] = sval;
        idx += 1;

        if last || br.is_empty() {
            break;
        }
    }
    blk
}

// ─── AC Prediction buffer ────────────────────────────────────────────────────
// SMPTE 421M §8.1.4.7.
//
// For each macroblock/block we cache the first row (AC[1..7]) and first
// column (AC[8,16,24,32,40,48,56]) of reconstructed coefficients (pre-IDCT,
// post-IQ) so they can be used as predictors for neighbouring blocks.

#[derive(Clone)]
pub struct AcPredBuffer {
    mb_w: usize,
    /// First row of coefficients for each MB×block: [mb_idx][blk][0..7]
    row: Vec<[[i32; 7]; 6]>,
    /// First col of coefficients for each MB×block: [mb_idx][blk][0..7]
    col: Vec<[[i32; 7]; 6]>,
}

impl AcPredBuffer {
    pub fn new(mb_w: usize, mb_h: usize) -> Self {
        let n = mb_w * mb_h;
        AcPredBuffer {
            mb_w,
            row: vec![[[0i32; 7]; 6]; n],
            col: vec![[[0i32; 7]; 6]; n],
        }
    }

    pub fn clear(&mut self) {
        self.row.fill([[0i32; 7]; 6]);
        self.col.fill([[0i32; 7]; 6]);
    }

    /// Get the AC predictor row (indices 1..7 of the reconstructed block).
    /// Returns the first row of the left neighbour (for horizontal prediction).
    pub fn pred_row(&self, mb_row: usize, mb_col: usize, blk: usize) -> [i32; 7] {
        let (src_mb_r, src_mb_c, src_blk) = Self::left_neighbour(mb_row, mb_col, blk);
        if src_mb_r as isize >= 0 && src_mb_c as isize >= 0 {
            let idx = src_mb_r * self.mb_w + src_mb_c;
            if idx < self.row.len() {
                return self.row[idx][src_blk];
            }
        }
        [0i32; 7]
    }

    /// Get the AC predictor column (rows 1..7 of the reconstructed block).
    /// Returns the first column of the top neighbour (for vertical prediction).
    pub fn pred_col(&self, mb_row: usize, mb_col: usize, blk: usize) -> [i32; 7] {
        let (src_mb_r, src_mb_c, src_blk) = Self::top_neighbour(mb_row, mb_col, blk);
        if src_mb_r as isize >= 0 && src_mb_c as isize >= 0 {
            let idx = src_mb_r * self.mb_w + src_mb_c;
            if idx < self.col.len() {
                return self.col[idx][src_blk];
            }
        }
        [0i32; 7]
    }

    /// VC-1 AC prediction from the left uses the neighbour's first coefficient column.
    pub fn pred_left_col(&self, mb_row: usize, mb_col: usize, blk: usize) -> [i32; 7] {
        let (r, c, b) = Self::left_neighbour(mb_row, mb_col, blk);
        let idx = r.wrapping_mul(self.mb_w).wrapping_add(c);
        if c < self.mb_w && idx < self.col.len() {
            self.col[idx][b]
        } else {
            [0; 7]
        }
    }

    /// VC-1 AC prediction from the top uses the neighbour's first coefficient row.
    pub fn pred_top_row(&self, mb_row: usize, mb_col: usize, blk: usize) -> [i32; 7] {
        let (r, c, b) = Self::top_neighbour(mb_row, mb_col, blk);
        let idx = r.wrapping_mul(self.mb_w).wrapping_add(c);
        if self.mb_w != 0 && c < self.mb_w && idx < self.row.len() {
            self.row[idx][b]
        } else {
            [0; 7]
        }
    }

    pub fn store_row(&mut self, mb_row: usize, mb_col: usize, blk: usize, row: [i32; 7]) {
        let idx = mb_row * self.mb_w + mb_col;
        if idx < self.row.len() {
            self.row[idx][blk] = row;
        }
    }

    pub fn store_col(&mut self, mb_row: usize, mb_col: usize, blk: usize, col: [i32; 7]) {
        let idx = mb_row * self.mb_w + mb_col;
        if idx < self.col.len() {
            self.col[idx][blk] = col;
        }
    }

    /// Left neighbour source: same logic as DC prediction neighbour mapping.
    fn left_neighbour(mb_row: usize, mb_col: usize, blk: usize) -> (usize, usize, usize) {
        match blk {
            0 => (mb_row, mb_col.wrapping_sub(1), 1),
            1 => (mb_row, mb_col, 0),
            2 => (mb_row, mb_col.wrapping_sub(1), 3),
            3 => (mb_row, mb_col, 2),
            _ => (mb_row, mb_col.wrapping_sub(1), blk),
        }
    }

    fn top_neighbour(mb_row: usize, mb_col: usize, blk: usize) -> (usize, usize, usize) {
        match blk {
            0 => (mb_row.wrapping_sub(1), mb_col, 2),
            1 => (mb_row.wrapping_sub(1), mb_col, 3),
            2 => (mb_row, mb_col, 0),
            3 => (mb_row, mb_col, 1),
            _ => (mb_row.wrapping_sub(1), mb_col, blk),
        }
    }
}

// ─── MV Predictor ────────────────────────────────────────────────────────────
// SMPTE 421M §8.3.5.3.
//
// The MV predictor for 1-MV macroblocks is the median of three neighbouring
// MVs: left (A), top (B), and top-right (C).  When a neighbour is out-of-
// frame or skipped, its MV is treated as (0,0).

#[derive(Clone, Default)]
pub struct MvPredictor {
    mb_w: usize,
    /// Stored MVs per MB: (mvx, mvy) in half-pixel units
    mvs: Vec<(i32, i32)>,
    /// Whether each MB was skipped (skipped MBs propagate MV=0)
    skipped: Vec<bool>,
}

impl MvPredictor {
    pub fn new(mb_w: usize, mb_h: usize) -> Self {
        let n = mb_w * mb_h;
        MvPredictor {
            mb_w,
            mvs: vec![(0, 0); n],
            skipped: vec![true; n],
        }
    }

    /// Compute the predicted MV for (mb_row, mb_col) from three neighbours.
    pub fn predict(&self, mb_row: usize, mb_col: usize) -> (i32, i32) {
        let get = |r: isize, c: isize| -> (i32, i32) {
            if r < 0 || c < 0 {
                return (0, 0);
            }
            let idx = r as usize * self.mb_w + c as usize;
            if idx >= self.mvs.len() || self.skipped[idx] {
                return (0, 0);
            }
            self.mvs[idx]
        };

        let r = mb_row as isize;
        let c = mb_col as isize;

        let (ax, ay) = get(r, c - 1); // left
        let (bx, by) = get(r - 1, c); // top
        let (cx, cy) = get(r - 1, c + 1); // top-right (or top-left if rightmost)
        // If top-right is out of bounds, use top-left instead (per spec)
        let (cx, cy) = if c + 1 >= self.mb_w as isize {
            get(r - 1, c - 1)
        } else {
            (cx, cy)
        };

        (median3(ax, bx, cx), median3(ay, by, cy))
    }

    pub fn store(&mut self, mb_row: usize, mb_col: usize, mv: (i32, i32), skipped: bool) {
        let idx = mb_row * self.mb_w + mb_col;
        if idx < self.mvs.len() {
            self.mvs[idx] = mv;
            self.skipped[idx] = skipped;
        }
    }
}

#[inline]
fn median3(a: i32, b: i32, c: i32) -> i32 {
    // Returns the median of three values
    if (a <= b && b <= c) || (c <= b && b <= a) {
        b
    } else if (b <= a && a <= c) || (c <= a && a <= b) {
        a
    } else {
        c
    }
}

#[inline(always)]
fn mid_pred(a: i32, b: i32, c: i32) -> i32 {
    // upstream mid_pred() helper.
    median3(a, b, c)
}

// ─── VC-1 signed overlap transform (Simple/Main) ───────────────────────────
// FFmpeg applies vc1_h_s_overlap()/vc1_v_s_overlap() to inverse-transform
// blocks before clipping them to u8.  Keeping the blocks signed is essential:
// clipping first and overlapping the final pixels is not equivalent.

#[inline]
fn vc1_h_s_overlap(left: &mut [i32; 64], right: &mut [i32; 64]) {
    let mut rnd1 = 4i32;
    let mut rnd2 = 3i32;
    for row in 0..8usize {
        let li = row * 8;
        let ri = row * 8;
        let a = left[li + 6];
        let b = left[li + 7];
        let c = right[ri];
        let d = right[ri + 1];
        let d1 = a - d;
        let d2 = a - d + b - c;

        left[li + 6] = ((a * 8) - d1 + rnd1) >> 3;
        left[li + 7] = ((b * 8) - d2 + rnd2) >> 3;
        right[ri] = ((c * 8) + d2 + rnd1) >> 3;
        right[ri + 1] = ((d * 8) + d1 + rnd2) >> 3;

        // Progressive Simple/Main uses flags=1 in FFmpeg: alternate rounding
        // on every row.
        rnd2 = 7 - rnd2;
        rnd1 = 7 - rnd1;
    }
}

#[inline]
fn vc1_v_s_overlap(top: &mut [i32; 64], bottom: &mut [i32; 64]) {
    let mut rnd1 = 4i32;
    let mut rnd2 = 3i32;
    for col in 0..8usize {
        let a = top[48 + col];
        let b = top[56 + col];
        let c = bottom[col];
        let d = bottom[8 + col];
        let d1 = a - d;
        let d2 = a - d + b - c;

        top[48 + col] = ((a * 8) - d1 + rnd1) >> 3;
        top[56 + col] = ((b * 8) - d2 + rnd2) >> 3;
        bottom[col] = ((c * 8) + d2 + rnd1) >> 3;
        bottom[8 + col] = ((d * 8) + d1 + rnd2) >> 3;

        rnd2 = 7 - rnd2;
        rnd1 = 7 - rnd1;
    }
}

#[inline]
fn vc1_two_blocks_mut(
    blocks: &mut [[i32; 64]],
    a: usize,
    b: usize,
) -> (&mut [i32; 64], &mut [i32; 64]) {
    debug_assert_ne!(a, b);
    if a < b {
        let (left, right) = blocks.split_at_mut(b);
        (&mut left[a], &mut right[0])
    } else {
        let (left, right) = blocks.split_at_mut(a);
        (&mut right[0], &mut left[b])
    }
}

#[inline]
fn vc1_recon_index(mb_width: usize, mb_row: usize, mb_col: usize, block: usize) -> usize {
    (mb_row * mb_width + mb_col) * 6 + block
}

/// Apply the progressive Simple/Main overlap transform to buffered signed
/// blocks.  Horizontal overlap always precedes vertical overlap, matching
/// ff_vc1_i_overlap_filter()/ff_vc1_p_overlap_filter().  `intra` is used for
/// P pictures where an edge is overlapped only when both adjacent blocks are
/// intra-coded.
fn vc1_apply_signed_overlap(
    blocks: &mut [[i32; 64]],
    intra: &[[bool; 6]],
    mb_width: usize,
    mb_height: usize,
    scale_current_after: bool,
) {
    #[inline]
    fn h_if(blocks: &mut [[i32; 64]], intra: &[[bool; 6]], left: usize, right: usize) {
        let lmb = left / 6;
        let lblk = left % 6;
        let rmb = right / 6;
        let rblk = right % 6;
        if intra[lmb][lblk] && intra[rmb][rblk] {
            let (l, r) = vc1_two_blocks_mut(blocks, left, right);
            vc1_h_s_overlap(l, r);
        }
    }

    #[inline]
    fn v_if(blocks: &mut [[i32; 64]], intra: &[[bool; 6]], top: usize, bottom: usize) {
        let tmb = top / 6;
        let tblk = top % 6;
        let bmb = bottom / 6;
        let bblk = bottom % 6;
        if intra[tmb][tblk] && intra[bmb][bblk] {
            let (t, b) = vc1_two_blocks_mut(blocks, top, bottom);
            vc1_v_s_overlap(t, b);
        }
    }

    #[inline]
    fn finish_v_column(
        blocks: &mut [[i32; 64]],
        intra: &[[bool; 6]],
        mb_width: usize,
        mb_row: usize,
        target_col: usize,
    ) {
        let bottom = |blk: usize| vc1_recon_index(mb_width, mb_row, target_col, blk);
        if mb_row > 0 {
            let top = |blk: usize| vc1_recon_index(mb_width, mb_row - 1, target_col, blk);
            v_if(blocks, intra, top(2), bottom(0));
            v_if(blocks, intra, top(3), bottom(1));
        }
        v_if(blocks, intra, bottom(0), bottom(2));
        v_if(blocks, intra, bottom(1), bottom(3));
        if mb_row > 0 {
            let top = |blk: usize| vc1_recon_index(mb_width, mb_row - 1, target_col, blk);
            v_if(blocks, intra, top(4), bottom(4));
            v_if(blocks, intra, top(5), bottom(5));
        }
    }

    // This deliberately mirrors ff_vc1_i_overlap_filter() /
    // ff_vc1_p_overlap_filter() in raster order instead of applying all H
    // edges and then all V edges globally.  The order is observable because
    // the integer overlap transform rounds, and for Simple/Main I pictures
    // FFmpeg applies RANGEREDFRM's x2 to the current MB after that MB's
    // overlap-filter call.
    for mb_row in 0..mb_height {
        for mb_col in 0..mb_width {
            let cur = |blk: usize| vc1_recon_index(mb_width, mb_row, mb_col, blk);

            // ff_vc1_*_overlap_filter(): H overlap on the left and internal
            // vertical borders of the current macroblock, in block-number
            // order 0..5 (block_map is already normalized by our layout).
            if mb_col > 0 {
                let left = |blk: usize| vc1_recon_index(mb_width, mb_row, mb_col - 1, blk);
                h_if(blocks, intra, left(1), cur(0));
                h_if(blocks, intra, cur(0), cur(1));
                h_if(blocks, intra, left(3), cur(2));
                h_if(blocks, intra, cur(2), cur(3));
                h_if(blocks, intra, left(4), cur(4));
                h_if(blocks, intra, left(5), cur(5));
            } else {
                h_if(blocks, intra, cur(0), cur(1));
                h_if(blocks, intra, cur(2), cur(3));
            }

            // V overlap trails H by one MB column.  At each decode position
            // FFmpeg finalizes the column on the left, and also the current
            // column when this is the last MB in the row.
            if mb_col > 0 {
                finish_v_column(blocks, intra, mb_width, mb_row, mb_col - 1);
            }
            if mb_col + 1 == mb_width {
                finish_v_column(blocks, intra, mb_width, mb_row, mb_col);
            }

            // vc1_decode_i_blocks() performs this after
            // ff_vc1_i_overlap_filter() for the current macroblock.  P
            // pictures already apply x2 immediately after each intra IDCT,
            // so callers pass false there.
            if scale_current_after {
                let base = vc1_recon_index(mb_width, mb_row, mb_col, 0);
                for blk in 0..6usize {
                    for v in &mut blocks[base + blk] {
                        *v *= 2;
                    }
                }
            }
        }
    }
}

/// Advanced Profile I-picture overlap for CONDOVER=SELECT.
///
/// This is the progressive subset of FFmpeg's `ff_vc1_i_overlap_filter()`.
/// A horizontal edge is enabled by the current MB's OVERFLAGS bit and, for
/// inter-MB edges, the neighbouring MB's bit as well. Vertical edges follow
/// the corresponding current/top rule. Internal luma edges only require the
/// current MB bit. The traversal intentionally matches the H-then-delayed-V
/// order used by the reference decoder because integer overlap rounding makes
/// that order observable.
fn vc1_apply_advanced_selective_i_overlap(
    blocks: &mut [[i32; 64]],
    mb_width: usize,
    mb_height: usize,
    overflags: &[u8],
) {
    #[inline]
    fn h_edge(
        blocks: &mut [[i32; 64]],
        mb_width: usize,
        mb_row: usize,
        mb_col: usize,
        left_blk: usize,
        right_blk: usize,
    ) {
        let left = vc1_recon_index(mb_width, mb_row, mb_col, left_blk);
        let right = vc1_recon_index(mb_width, mb_row, mb_col, right_blk);
        let (l, r) = vc1_two_blocks_mut(blocks, left, right);
        vc1_h_s_overlap(l, r);
    }

    #[inline]
    fn h_cross(
        blocks: &mut [[i32; 64]],
        mb_width: usize,
        mb_row: usize,
        mb_col: usize,
        left_blk: usize,
        right_blk: usize,
    ) {
        let left = vc1_recon_index(mb_width, mb_row, mb_col - 1, left_blk);
        let right = vc1_recon_index(mb_width, mb_row, mb_col, right_blk);
        let (l, r) = vc1_two_blocks_mut(blocks, left, right);
        vc1_h_s_overlap(l, r);
    }

    #[inline]
    fn v_edge(
        blocks: &mut [[i32; 64]],
        mb_width: usize,
        mb_row: usize,
        mb_col: usize,
        top_blk: usize,
        bottom_blk: usize,
    ) {
        let top = vc1_recon_index(mb_width, mb_row, mb_col, top_blk);
        let bottom = vc1_recon_index(mb_width, mb_row, mb_col, bottom_blk);
        let (t, b) = vc1_two_blocks_mut(blocks, top, bottom);
        vc1_v_s_overlap(t, b);
    }

    #[inline]
    fn v_cross(
        blocks: &mut [[i32; 64]],
        mb_width: usize,
        mb_row: usize,
        mb_col: usize,
        top_blk: usize,
        bottom_blk: usize,
    ) {
        let top = vc1_recon_index(mb_width, mb_row - 1, mb_col, top_blk);
        let bottom = vc1_recon_index(mb_width, mb_row, mb_col, bottom_blk);
        let (t, b) = vc1_two_blocks_mut(blocks, top, bottom);
        vc1_v_s_overlap(t, b);
    }

    #[inline]
    fn finish_v_column(
        blocks: &mut [[i32; 64]],
        mb_width: usize,
        mb_row: usize,
        col: usize,
        overflags: &[u8],
    ) {
        let cur = mb_row * mb_width + col;
        let cur_on = overflags.get(cur).copied().unwrap_or(0) != 0;
        if !cur_on {
            return;
        }

        let top_on = mb_row > 0 && overflags.get(cur - mb_width).copied().unwrap_or(0) != 0;
        // Preserve FFmpeg's block-number order: cross-row luma blocks 0/1,
        // internal luma blocks 2/3, then chroma 4/5.
        if top_on {
            v_cross(blocks, mb_width, mb_row, col, 2, 0);
            v_cross(blocks, mb_width, mb_row, col, 3, 1);
        }
        v_edge(blocks, mb_width, mb_row, col, 0, 2);
        v_edge(blocks, mb_width, mb_row, col, 1, 3);
        if top_on {
            v_cross(blocks, mb_width, mb_row, col, 4, 4);
            v_cross(blocks, mb_width, mb_row, col, 5, 5);
        }
    }

    for mb_row in 0..mb_height {
        for mb_col in 0..mb_width {
            let cur = mb_row * mb_width + mb_col;
            let cur_on = overflags.get(cur).copied().unwrap_or(0) != 0;

            if cur_on {
                let left_on = mb_col > 0 && overflags.get(cur - 1).copied().unwrap_or(0) != 0;
                // Preserve i=0..5 ordering from ff_vc1_i_overlap_filter().
                if left_on {
                    h_cross(blocks, mb_width, mb_row, mb_col, 1, 0);
                }
                h_edge(blocks, mb_width, mb_row, mb_col, 0, 1);
                if left_on {
                    h_cross(blocks, mb_width, mb_row, mb_col, 3, 2);
                }
                h_edge(blocks, mb_width, mb_row, mb_col, 2, 3);
                if left_on {
                    h_cross(blocks, mb_width, mb_row, mb_col, 4, 4);
                    h_cross(blocks, mb_width, mb_row, mb_col, 5, 5);
                }
            }

            // Vertical overlap trails by one MB column, exactly like the
            // reference implementation.
            if mb_col > 0 {
                finish_v_column(blocks, mb_width, mb_row, mb_col - 1, overflags);
            }
            if mb_col + 1 == mb_width {
                finish_v_column(blocks, mb_width, mb_row, mb_col, overflags);
            }
        }
    }
}

#[inline(always)]
fn vc1_range_reduce_sample(v: u8) -> u8 {
    (((v as i32 - 128) >> 1) + 128) as u8
}

fn vc1_range_reduce_reference(frame: &mut YuvFrame) {
    for v in &mut frame.y {
        *v = vc1_range_reduce_sample(*v);
    }
    for v in &mut frame.cb {
        *v = vc1_range_reduce_sample(*v);
    }
    for v in &mut frame.cr {
        *v = vc1_range_reduce_sample(*v);
    }
}

/// FFmpeg vc1_mc.c stores `luma_mv` for chroma loop-filter decisions after
/// converting the qpel luma vector to the chroma-vector domain, but before
/// FASTUVMC rounding. Keep exactly that representation in vc1_chroma_mvs.
#[inline]
fn vc1_loop_chroma_mv(mv: (i32, i32)) -> (i32, i32) {
    (
        (mv.0 + ((mv.0 & 3) == 3) as i32) >> 1,
        (mv.1 + ((mv.1 & 3) == 3) as i32) >> 1,
    )
}

// ─── Motion compensation ─────────────────────────────────────────────────────
// Half-pixel bilinear interpolation per SMPTE 421M §7.3.

fn mc_luma(
    dst: &mut [u8],
    dst_stride: usize,
    src: &[u8],
    src_stride: usize,
    src_w: usize,
    src_h: usize,
    x: i32,
    y: i32,
    w: usize,
    h: usize,
) {
    // x and y in half-pixel units
    let xh = x & 1 != 0;
    let yh = y & 1 != 0;
    let x0 = (x >> 1) as isize;
    let y0 = (y >> 1) as isize;

    for dy in 0..h {
        for dx in 0..w {
            let sx = (x0 + dx as isize).clamp(0, src_w as isize - 1) as usize;
            let sy = (y0 + dy as isize).clamp(0, src_h as isize - 1) as usize;
            let sx1 = (sx + 1).min(src_w - 1);
            let sy1 = (sy + 1).min(src_h - 1);

            let p00 = src[sy * src_stride + sx] as i32;
            let p10 = src[sy * src_stride + sx1] as i32;
            let p01 = src[sy1 * src_stride + sx] as i32;
            let p11 = src[sy1 * src_stride + sx1] as i32;

            let val = match (xh, yh) {
                (false, false) => p00,
                (true, false) => (p00 + p10 + 1) >> 1,
                (false, true) => (p00 + p01 + 1) >> 1,
                (true, true) => (p00 + p10 + p01 + p11 + 2) >> 2,
            };
            dst[dy * dst_stride + dx] = val.clamp(0, 255) as u8;
        }
    }
}

// ─── DQUANT: macroblock-level differential quantizer ────────────────────────
// SMPTE 421M §8.1.4.10 / §8.3.7.
//
// This follows FFmpeg's GET_MQUANT() semantics.  A negative mquant is an
// internal marker meaning that HALFQP must not be applied to residual scaling;
// the absolute value is still the macroblock quantizer.
fn read_mquant(
    br: &mut BitReader<'_>,
    dquant: &crate::vc1::DQuantInfo,
    pquant: i32,
    mb_x: u32,
    mb_y: u32,
    mb_width: u32,
    mb_height: u32,
) -> Result<i32> {
    if !dquant.enabled {
        return Ok(pquant);
    }

    let mut mquant = pquant;
    let mut edges = 0u8;
    match dquant.profile {
        3 => {
            // DQPROFILE_ALL_MBS
            if dquant.bi_level {
                let bilevel = br.read_bit().ok_or_else(|| {
                    DecoderError::InvalidData("truncated WMV3 DQBILEVEL macroblock bit".into())
                })?;
                mquant = if bilevel {
                    -(dquant.alt_pquant as i32)
                } else {
                    pquant
                };
            } else {
                let mqdiff = br
                    .read_bits(3)
                    .ok_or_else(|| DecoderError::InvalidData("truncated WMV3 MQDIFF".into()))?
                    as i32;
                mquant = if mqdiff != 7 {
                    -pquant - mqdiff
                } else {
                    -(br.read_bits(5)
                        .ok_or_else(|| DecoderError::InvalidData("truncated WMV3 MQUANT".into()))?
                        as i32)
                };
            }
        }
        0 => edges = 1u8 << dquant.edge.min(3),
        1 => edges = ((3u16 << dquant.edge.min(3)) % 15) as u8,
        2 => edges = 15,
        _ => {}
    }

    if (edges & 1) != 0 && mb_x == 0 {
        mquant = -(dquant.alt_pquant as i32);
    }
    if (edges & 2) != 0 && mb_y == 0 {
        mquant = -(dquant.alt_pquant as i32);
    }
    if (edges & 4) != 0 && mb_x + 1 == mb_width {
        mquant = -(dquant.alt_pquant as i32);
    }
    if (edges & 8) != 0 && mb_y + 1 == mb_height {
        mquant = -(dquant.alt_pquant as i32);
    }

    if mquant == 0 || !(-31..=31).contains(&mquant) {
        mquant = 1;
    }
    Ok(mquant)
}

// Range reduction for inter motion compensation is applied to the current
// picture's sampled reference (see vc1_range_reduce_reference above).  Intra
// range expansion stays in the signed transform domain and is performed by
// the WMV3 reconstruction paths below.

// ─── Write helpers ───────────────────────────────────────────────────────────

/// Block (mb_row, mb_col, blk_idx) → (plane ref, x, y, stride, plane_h)
fn block_coords(
    mb_row: u32,
    mb_col: u32,
    blk: usize,
    width: u32,
    height: u32,
) -> (bool, usize, usize, usize, usize) {
    // Returns (is_luma, px, py, stride, plane_height)
    let (is_luma, bx, by) = match blk {
        0 => (true, (mb_col * 16) as usize, (mb_row * 16) as usize),
        1 => (true, (mb_col * 16 + 8) as usize, (mb_row * 16) as usize),
        2 => (true, (mb_col * 16) as usize, (mb_row * 16 + 8) as usize),
        3 => (true, (mb_col * 16 + 8) as usize, (mb_row * 16 + 8) as usize),
        _ => (false, (mb_col * 8) as usize, (mb_row * 8) as usize),
    };
    let stride = if is_luma {
        width as usize
    } else {
        (width / 2) as usize
    };
    let ph = if is_luma {
        height as usize
    } else {
        (height / 2) as usize
    };
    (is_luma, bx, by, stride, ph)
}

fn write_intra_block(
    frame: &mut YuvFrame,
    mb_row: u32,
    mb_col: u32,
    blk: usize,
    coeff: &[i32; 64],
) {
    let (is_luma, bx, by, stride, ph) =
        block_coords(mb_row, mb_col, blk, frame.width, frame.height);
    let plane: &mut Vec<u8> = if is_luma {
        &mut frame.y
    } else if blk == 4 {
        &mut frame.cb
    } else {
        &mut frame.cr
    };
    for r in 0..8 {
        if by + r >= ph {
            break;
        }
        for c in 0..8 {
            if bx + c >= stride {
                break;
            }
            let idx = (by + r) * stride + (bx + c);
            plane[idx] = (128 + coeff[r * 8 + c]).clamp(0, 255) as u8;
        }
    }
}

/// VC-1 Simple/Main I pictures without the high-quant overlap path use
/// put_pixels_clamped(): the inverse-transform result is already in the
/// unsigned pixel domain and must not receive the +128 signed-block bias.
fn write_intra_block_unsigned(
    frame: &mut YuvFrame,
    mb_row: u32,
    mb_col: u32,
    blk: usize,
    coeff: &[i32; 64],
) {
    let (is_luma, bx, by, stride, ph) =
        block_coords(mb_row, mb_col, blk, frame.width, frame.height);
    let plane: &mut Vec<u8> = if is_luma {
        &mut frame.y
    } else if blk == 4 {
        &mut frame.cb
    } else {
        &mut frame.cr
    };
    for r in 0..8 {
        if by + r >= ph {
            break;
        }
        for c in 0..8 {
            if bx + c >= stride {
                break;
            }
            let idx = (by + r) * stride + (bx + c);
            plane[idx] = coeff[r * 8 + c].clamp(0, 255) as u8;
        }
    }
}

#[inline]
fn write_block_to_frame(
    frame: &mut YuvFrame,
    mb_row: usize,
    mb_col: usize,
    blk: usize,
    coeff: &[i32; 64],
) {
    write_intra_block(frame, mb_row as u32, mb_col as u32, blk, coeff);
}

// WMV2 path uses upstream's Simple IDCT (int16). Provide i16 write/add helpers.
fn write_intra_block_i16(
    frame: &mut YuvFrame,
    mb_row: u32,
    mb_col: u32,
    blk: usize,
    coeff: &[i16; 64],
) {
    let (is_luma, bx, by, stride, ph) =
        block_coords(mb_row, mb_col, blk, frame.width, frame.height);
    let plane: &mut Vec<u8> = if is_luma {
        &mut frame.y
    } else if blk == 4 {
        &mut frame.cb
    } else {
        &mut frame.cr
    };
    for r in 0..8usize {
        if by + r >= ph {
            break;
        }
        for c in 0..8usize {
            if bx + c >= stride {
                break;
            }
            let idx = (by + r) * stride + (bx + c);
            let v = coeff[r * 8 + c] as i32;
            plane[idx] = (v + 128).clamp(0, 255) as u8;
        }
    }
}

fn add_residual_block_i16(
    frame: &mut YuvFrame,
    mb_row: u32,
    mb_col: u32,
    blk: usize,
    coeff: &[i16; 64],
) {
    let (is_luma, bx, by, stride, ph) =
        block_coords(mb_row, mb_col, blk, frame.width, frame.height);
    let plane: &mut Vec<u8> = if is_luma {
        &mut frame.y
    } else if blk == 4 {
        &mut frame.cb
    } else {
        &mut frame.cr
    };
    for r in 0..8usize {
        if by + r >= ph {
            break;
        }
        for c in 0..8usize {
            if bx + c >= stride {
                break;
            }
            let idx = (by + r) * stride + (bx + c);
            let v = plane[idx] as i32 + coeff[r * 8 + c] as i32;
            plane[idx] = v.clamp(0, 255) as u8;
        }
    }
}

/// Motion compensate one 16×16 macroblock from `reference` into `dst`.
/// Motion vectors are in half-pel units (like H.263/MSMPEG4/WMV2).
fn motion_compensate_mb(
    dst: &mut YuvFrame,
    reference: &YuvFrame,
    mb_row: usize,
    mb_col: usize,
    mvx: i32,
    mvy: i32,
) {
    let fw = dst.width as usize;
    let fh = dst.height as usize;
    if fw == 0 || fh == 0 {
        return;
    }
    let cw = fw / 2;
    let ch = fh / 2;

    // ── Luma (16×16) ────────────────────────────────────────────────────────
    let dst_x = mb_col * 16;
    let dst_y = mb_row * 16;
    let src_x = dst_x as i32 * 2 + mvx; // half-pel coordinate
    let src_y = dst_y as i32 * 2 + mvy;

    if reference.y.len() == fw * fh && dst.y.len() == fw * fh {
        let mut tmp = [0u8; 256];
        mc_luma(&mut tmp, 16, &reference.y, fw, fw, fh, src_x, src_y, 16, 16);
        for r in 0..16 {
            if dst_y + r >= fh {
                break;
            }
            if dst_x >= fw {
                break;
            }
            let d_off = (dst_y + r) * fw + dst_x;
            let s_off = r * 16;
            let max = (fw - dst_x).min(16);
            dst.y[d_off..d_off + max].copy_from_slice(&tmp[s_off..s_off + max]);
        }
    }

    // ── Chroma (8×8) ───────────────────────────────────────────────────────
    // upstream (ff_mspel_motion): motion vectors are in half-luma-pel units.
    // For 4:2:0 chroma, 1 chroma pixel = 2 luma pixels, so the same MV value
    // corresponds to quarter-chroma-pel units.
    // upstream collapses the 2-bit chroma fraction to a boolean (any non-zero
    // fractional part triggers half-chroma interpolation):
    //   dxy |= (motion_x & 3) != 0
    //   mx  = motion_x >> 2
    // We reproduce that mapping here by converting to half-chroma-pel coords.
    if reference.cb.len() == cw * ch && dst.cb.len() == cw * ch {
        let dst_xc = mb_col * 8;
        let dst_yc = mb_row * 8;

        let mx = mvx >> 2;
        let my = mvy >> 2;
        let xh = (mvx & 3) != 0;
        let yh = (mvy & 3) != 0;

        // Half-chroma-pel coordinate for mc_luma().
        let src_xc = (dst_xc as i32 + mx) * 2 + if xh { 1 } else { 0 };
        let src_yc = (dst_yc as i32 + my) * 2 + if yh { 1 } else { 0 };

        let mut tmp_cb = [0u8; 64];
        let mut tmp_cr = [0u8; 64];
        mc_luma(
            &mut tmp_cb,
            8,
            &reference.cb,
            cw,
            cw,
            ch,
            src_xc,
            src_yc,
            8,
            8,
        );
        mc_luma(
            &mut tmp_cr,
            8,
            &reference.cr,
            cw,
            cw,
            ch,
            src_xc,
            src_yc,
            8,
            8,
        );

        for r in 0..8 {
            if dst_yc + r >= ch {
                break;
            }
            if dst_xc >= cw {
                break;
            }
            let d_off = (dst_yc + r) * cw + dst_xc;
            let s_off = r * 8;
            let max = (cw - dst_xc).min(8);
            dst.cb[d_off..d_off + max].copy_from_slice(&tmp_cb[s_off..s_off + max]);
            dst.cr[d_off..d_off + max].copy_from_slice(&tmp_cr[s_off..s_off + max]);
        }
    }
}

// ── WMV2 MSPEL motion compensation (direct port of upstream ff_mspel_motion + wmv2_mspel_init) ──

#[inline]
fn clip_u8(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

#[inline]
fn rnd_avg_u8(a: u8, b: u8) -> u8 {
    ((a as u16 + b as u16 + 1) >> 1) as u8
}

#[inline]
fn no_rnd_avg_u8(a: u8, b: u8) -> u8 {
    ((a as u16 + b as u16) >> 1) as u8
}

fn wmv2_mspel8_h_lowpass(
    dst: &mut [u8],
    dst_off: usize,
    dst_stride: usize,
    src: &[u8],
    src_off: usize,
    src_stride: usize,
    h: usize,
) {
    for i in 0..h {
        let so = src_off + i * src_stride;
        let doff = dst_off + i * dst_stride;
        // dst[0..8]
        dst[doff] = clip_u8(
            (9 * (src[so] as i32 + src[so + 1] as i32) - (src[so - 1] as i32 + src[so + 2] as i32)
                + 8)
                >> 4,
        );
        dst[doff + 1] = clip_u8(
            (9 * (src[so + 1] as i32 + src[so + 2] as i32) - (src[so] as i32 + src[so + 3] as i32)
                + 8)
                >> 4,
        );
        dst[doff + 2] = clip_u8(
            (9 * (src[so + 2] as i32 + src[so + 3] as i32)
                - (src[so + 1] as i32 + src[so + 4] as i32)
                + 8)
                >> 4,
        );
        dst[doff + 3] = clip_u8(
            (9 * (src[so + 3] as i32 + src[so + 4] as i32)
                - (src[so + 2] as i32 + src[so + 5] as i32)
                + 8)
                >> 4,
        );
        dst[doff + 4] = clip_u8(
            (9 * (src[so + 4] as i32 + src[so + 5] as i32)
                - (src[so + 3] as i32 + src[so + 6] as i32)
                + 8)
                >> 4,
        );
        dst[doff + 5] = clip_u8(
            (9 * (src[so + 5] as i32 + src[so + 6] as i32)
                - (src[so + 4] as i32 + src[so + 7] as i32)
                + 8)
                >> 4,
        );
        dst[doff + 6] = clip_u8(
            (9 * (src[so + 6] as i32 + src[so + 7] as i32)
                - (src[so + 5] as i32 + src[so + 8] as i32)
                + 8)
                >> 4,
        );
        dst[doff + 7] = clip_u8(
            (9 * (src[so + 7] as i32 + src[so + 8] as i32)
                - (src[so + 6] as i32 + src[so + 9] as i32)
                + 8)
                >> 4,
        );
    }
}

fn wmv2_mspel8_v_lowpass(
    dst: &mut [u8],
    dst_off: usize,
    dst_stride: usize,
    src: &[u8],
    src_off: usize,
    src_stride: usize,
    w: usize,
) {
    for i in 0..w {
        let so = src_off + i;
        let s_1 = src[so - src_stride] as i32;
        let s0 = src[so] as i32;
        let s1 = src[so + src_stride] as i32;
        let s2 = src[so + 2 * src_stride] as i32;
        let s3 = src[so + 3 * src_stride] as i32;
        let s4 = src[so + 4 * src_stride] as i32;
        let s5 = src[so + 5 * src_stride] as i32;
        let s6 = src[so + 6 * src_stride] as i32;
        let s7 = src[so + 7 * src_stride] as i32;
        let s8 = src[so + 8 * src_stride] as i32;
        let s9 = src[so + 9 * src_stride] as i32;

        let do0 = dst_off + i + 0 * dst_stride;
        let do1 = dst_off + i + dst_stride;
        let do2 = dst_off + i + 2 * dst_stride;
        let do3 = dst_off + i + 3 * dst_stride;
        let do4 = dst_off + i + 4 * dst_stride;
        let do5 = dst_off + i + 5 * dst_stride;
        let do6 = dst_off + i + 6 * dst_stride;
        let do7 = dst_off + i + 7 * dst_stride;

        dst[do0] = clip_u8((9 * (s0 + s1) - (s_1 + s2) + 8) >> 4);
        dst[do1] = clip_u8((9 * (s1 + s2) - (s0 + s3) + 8) >> 4);
        dst[do2] = clip_u8((9 * (s2 + s3) - (s1 + s4) + 8) >> 4);
        dst[do3] = clip_u8((9 * (s3 + s4) - (s2 + s5) + 8) >> 4);
        dst[do4] = clip_u8((9 * (s4 + s5) - (s3 + s6) + 8) >> 4);
        dst[do5] = clip_u8((9 * (s5 + s6) - (s4 + s7) + 8) >> 4);
        dst[do6] = clip_u8((9 * (s6 + s7) - (s5 + s8) + 8) >> 4);
        dst[do7] = clip_u8((9 * (s7 + s8) - (s6 + s9) + 8) >> 4);
    }
}

#[inline]
fn put_pixels8x8(dst: &mut [u8], dst_off: usize, src: &[u8], src_off: usize, stride: usize) {
    for y in 0..8 {
        let d = dst_off + y * stride;
        let s = src_off + y * stride;
        dst[d..d + 8].copy_from_slice(&src[s..s + 8]);
    }
}

#[inline]
fn put_pixels8_l2_8_no_rnd(
    dst: &mut [u8],
    dst_off: usize,
    src1: &[u8],
    src1_off: usize,
    src2: &[u8],
    src2_off: usize,
    dst_stride: usize,
    src1_stride: usize,
    src2_stride: usize,
    h: usize,
) {
    for y in 0..h {
        let d = dst_off + y * dst_stride;
        let s1 = src1_off + y * src1_stride;
        let s2 = src2_off + y * src2_stride;
        for x in 0..8 {
            dst[d + x] = no_rnd_avg_u8(src1[s1 + x], src2[s2 + x]);
        }
    }
}

fn put_mspel8_mc10(dst: &mut [u8], dst_off: usize, src: &[u8], src_off: usize, stride: usize) {
    let mut half = [0u8; 64];
    wmv2_mspel8_h_lowpass(&mut half, 0, 8, src, src_off, stride, 8);
    put_pixels8_l2_8_no_rnd(dst, dst_off, src, src_off, &half, 0, stride, stride, 8, 8);
}

fn put_mspel8_mc20(dst: &mut [u8], dst_off: usize, src: &[u8], src_off: usize, stride: usize) {
    wmv2_mspel8_h_lowpass(dst, dst_off, stride, src, src_off, stride, 8);
}

fn put_mspel8_mc30(dst: &mut [u8], dst_off: usize, src: &[u8], src_off: usize, stride: usize) {
    let mut half = [0u8; 64];
    wmv2_mspel8_h_lowpass(&mut half, 0, 8, src, src_off, stride, 8);
    put_pixels8_l2_8_no_rnd(
        dst,
        dst_off,
        src,
        src_off + 1,
        &half,
        0,
        stride,
        stride,
        8,
        8,
    );
}

fn put_mspel8_mc02(dst: &mut [u8], dst_off: usize, src: &[u8], src_off: usize, stride: usize) {
    wmv2_mspel8_v_lowpass(dst, dst_off, stride, src, src_off, stride, 8);
}

fn put_mspel8_mc12(dst: &mut [u8], dst_off: usize, src: &[u8], src_off: usize, stride: usize) {
    let mut half_h = [0u8; 88];
    let mut half_v = [0u8; 64];
    let mut half_hv = [0u8; 64];
    // h_lowpass(halfH, src - stride, 8, stride, 11)
    wmv2_mspel8_h_lowpass(&mut half_h, 0, 8, src, src_off - stride, stride, 11);
    // v_lowpass(halfV, src, 8, stride, 8)
    wmv2_mspel8_v_lowpass(&mut half_v, 0, 8, src, src_off, stride, 8);
    // v_lowpass(halfHV, halfH + 8, 8, 8, 8)
    wmv2_mspel8_v_lowpass(&mut half_hv, 0, 8, &half_h, 8, 8, 8);
    put_pixels8_l2_8_no_rnd(dst, dst_off, &half_v, 0, &half_hv, 0, stride, 8, 8, 8);
}

fn put_mspel8_mc22(dst: &mut [u8], dst_off: usize, src: &[u8], src_off: usize, stride: usize) {
    let mut half_h = [0u8; 88];
    wmv2_mspel8_h_lowpass(&mut half_h, 0, 8, src, src_off - stride, stride, 11);
    wmv2_mspel8_v_lowpass(dst, dst_off, stride, &half_h, 8, 8, 8);
}

fn put_mspel8_mc32(dst: &mut [u8], dst_off: usize, src: &[u8], src_off: usize, stride: usize) {
    let mut half_h = [0u8; 88];
    let mut half_v = [0u8; 64];
    let mut half_hv = [0u8; 64];
    wmv2_mspel8_h_lowpass(&mut half_h, 0, 8, src, src_off - stride, stride, 11);
    wmv2_mspel8_v_lowpass(&mut half_v, 0, 8, src, src_off + 1, stride, 8);
    wmv2_mspel8_v_lowpass(&mut half_hv, 0, 8, &half_h, 8, 8, 8);
    put_pixels8_l2_8_no_rnd(dst, dst_off, &half_v, 0, &half_hv, 0, stride, 8, 8, 8);
}

#[inline]
fn wmv2_put_mspel_pixels(
    dxy: usize,
    dst: &mut [u8],
    dst_off: usize,
    src: &[u8],
    src_off: usize,
    stride: usize,
) {
    match dxy {
        0 => put_pixels8x8(dst, dst_off, src, src_off, stride),
        1 => put_mspel8_mc10(dst, dst_off, src, src_off, stride),
        2 => put_mspel8_mc20(dst, dst_off, src, src_off, stride),
        3 => put_mspel8_mc30(dst, dst_off, src, src_off, stride),
        4 => put_mspel8_mc02(dst, dst_off, src, src_off, stride),
        5 => put_mspel8_mc12(dst, dst_off, src, src_off, stride),
        6 => put_mspel8_mc22(dst, dst_off, src, src_off, stride),
        7 => put_mspel8_mc32(dst, dst_off, src, src_off, stride),
        _ => put_pixels8x8(dst, dst_off, src, src_off, stride),
    }
}

fn emulated_edge_mc(
    buf: &mut [u8],
    buf_stride: usize,
    src: &[u8],
    src_stride: usize,
    block_w: usize,
    block_h: usize,
    src_x: i32,
    src_y: i32,
    h_edge: usize,
    v_edge: usize,
) {
    let max_x = (h_edge as i32 - 1).max(0);
    let max_y = (v_edge as i32 - 1).max(0);
    for y in 0..block_h {
        let sy = (src_y + y as i32).clamp(0, max_y) as usize;
        let drow = y * buf_stride;
        let srow = sy * src_stride;
        for x in 0..block_w {
            let sx = (src_x + x as i32).clamp(0, max_x) as usize;
            buf[drow + x] = src[srow + sx];
        }
    }
}

#[inline]
fn chroma_put_pixels(
    dst: &mut [u8],
    dst_off: usize,
    src: &[u8],
    src_off: usize,
    stride: usize,
    h: usize,
) {
    for y in 0..h {
        let d = dst_off + y * stride;
        let s = src_off + y * stride;
        dst[d..d + 8].copy_from_slice(&src[s..s + 8]);
    }
}

#[inline]
fn chroma_put_x2(
    dst: &mut [u8],
    dst_off: usize,
    src: &[u8],
    src_off: usize,
    stride: usize,
    h: usize,
) {
    for y in 0..h {
        let d = dst_off + y * stride;
        let s = src_off + y * stride;
        for x in 0..8 {
            dst[d + x] = rnd_avg_u8(src[s + x], src[s + x + 1]);
        }
    }
}

#[inline]
fn chroma_put_y2(
    dst: &mut [u8],
    dst_off: usize,
    src: &[u8],
    src_off: usize,
    stride: usize,
    h: usize,
) {
    for y in 0..h {
        let d = dst_off + y * stride;
        let s = src_off + y * stride;
        let s2 = s + stride;
        for x in 0..8 {
            dst[d + x] = rnd_avg_u8(src[s + x], src[s2 + x]);
        }
    }
}

#[inline]
fn chroma_put_xy2(
    dst: &mut [u8],
    dst_off: usize,
    src: &[u8],
    src_off: usize,
    stride: usize,
    h: usize,
) {
    for y in 0..h {
        let d = dst_off + y * stride;
        let s = src_off + y * stride;
        let s2 = s + stride;
        for x in 0..8 {
            let a = src[s + x] as u16;
            let b = src[s + x + 1] as u16;
            let c = src[s2 + x] as u16;
            let e = src[s2 + x + 1] as u16;
            dst[d + x] = ((a + b + c + e + 2) >> 2) as u8;
        }
    }
}

/// Direct port of upstream `ff_mspel_motion` for WMV2 (MV in half-luma-pel units).
fn wmv2_mspel_motion_mb(
    dst: &mut YuvFrame,
    reference: &YuvFrame,
    mb_row: usize,
    mb_col: usize,
    motion_x: i32,
    motion_y: i32,
    hshift: u8,
) {
    let fw = dst.width as usize;
    let fh = dst.height as usize;
    if fw == 0 || fh == 0 {
        return;
    }
    let cw = fw / 2;
    let ch = fh / 2;

    // ---- Luma ----
    let mut dxy = ((motion_y & 1) << 1) | (motion_x & 1);
    dxy = 2 * dxy + hshift as i32;

    let mut src_x = mb_col as i32 * 16 + (motion_x >> 1);
    let mut src_y = mb_row as i32 * 16 + (motion_y >> 1);

    // clip to [-16, width] / [-16, height]
    if src_x < -16 {
        src_x = -16;
    }
    if src_x > dst.width as i32 {
        src_x = dst.width as i32;
    }
    if src_y < -16 {
        src_y = -16;
    }
    if src_y > dst.height as i32 {
        src_y = dst.height as i32;
    }

    if src_x <= -16 || src_x >= dst.width as i32 {
        dxy &= !3;
    }
    if src_y <= -16 || src_y >= dst.height as i32 {
        dxy &= !4;
    }

    let linesize = fw;
    let mut src_plane: &[u8] = &reference.y;
    let src_off: usize;

    // edge condition: same as upstream (using h_edge_pos=width, v_edge_pos=height)
    if src_x < 1
        || src_y < 1
        || src_x + 17 >= dst.width as i32
        || src_y + 16 + 1 >= dst.height as i32
    {
        let mut edge = vec![0u8; linesize * 19];
        emulated_edge_mc(
            &mut edge,
            linesize,
            &reference.y,
            linesize,
            19,
            19,
            src_x - 1,
            src_y - 1,
            fw,
            fh,
        );
        src_plane = edge.as_slice();
        src_off = 1 + linesize;
        // keep edge alive via scope capture
        // (we rebind below for actual reads)
        // NOTE: src_plane points into `edge` which must live for the rest of this function.
        // Rust ensures this because `edge` is in this scope.

        // Use the edge buffer for the remainder of this luma section.
        let dst_x = mb_col * 16;
        let dst_y = mb_row * 16;
        let dxyu = (dxy as usize).min(7);

        // 4x 8x8 blocks
        let dst00 = dst_y * linesize + dst_x;
        let src00 = src_off;
        wmv2_put_mspel_pixels(dxyu, &mut dst.y, dst00, src_plane, src00, linesize);
        wmv2_put_mspel_pixels(dxyu, &mut dst.y, dst00 + 8, src_plane, src00 + 8, linesize);
        wmv2_put_mspel_pixels(
            dxyu,
            &mut dst.y,
            dst00 + 8 * linesize,
            src_plane,
            src00 + 8 * linesize,
            linesize,
        );
        wmv2_put_mspel_pixels(
            dxyu,
            &mut dst.y,
            dst00 + 8 + 8 * linesize,
            src_plane,
            src00 + 8 + 8 * linesize,
            linesize,
        );

        // ---- Chroma (still within edge scope) ----
        if dst.cb.is_empty() || reference.cb.is_empty() {
            return;
        }

        let mut cdxy = 0usize;
        if (motion_x & 3) != 0 {
            cdxy |= 1;
        }
        if (motion_y & 3) != 0 {
            cdxy |= 2;
        }
        let mx = motion_x >> 2;
        let my = motion_y >> 2;

        let mut csrc_x = mb_col as i32 * 8 + mx;
        let mut csrc_y = mb_row as i32 * 8 + my;

        if csrc_x < -8 {
            csrc_x = -8;
        }
        if csrc_x > (dst.width as i32 >> 1) {
            csrc_x = dst.width as i32 >> 1;
        }
        if csrc_y < -8 {
            csrc_y = -8;
        }
        if csrc_y > (dst.height as i32 >> 1) {
            csrc_y = dst.height as i32 >> 1;
        }

        if csrc_x == (dst.width as i32 >> 1) {
            cdxy &= !1;
        }
        if csrc_y == (dst.height as i32 >> 1) {
            cdxy &= !2;
        }

        let uvlinesize = cw;

        let mut edge_uv = vec![0u8; uvlinesize * 9];
        // cb
        emulated_edge_mc(
            &mut edge_uv,
            uvlinesize,
            &reference.cb,
            uvlinesize,
            9,
            9,
            csrc_x,
            csrc_y,
            cw,
            ch,
        );
        let dst_xc = mb_col * 8;
        let dst_yc = mb_row * 8;
        let dst_cb_off = dst_yc * uvlinesize + dst_xc;
        match cdxy {
            0 => chroma_put_pixels(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
            1 => chroma_put_x2(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
            2 => chroma_put_y2(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
            _ => chroma_put_xy2(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
        }

        // cr
        emulated_edge_mc(
            &mut edge_uv,
            uvlinesize,
            &reference.cr,
            uvlinesize,
            9,
            9,
            csrc_x,
            csrc_y,
            cw,
            ch,
        );
        let dst_cr_off = dst_yc * uvlinesize + dst_xc;
        match cdxy {
            0 => chroma_put_pixels(&mut dst.cr, dst_cr_off, &edge_uv, 0, uvlinesize, 8),
            1 => chroma_put_x2(&mut dst.cr, dst_cr_off, &edge_uv, 0, uvlinesize, 8),
            2 => chroma_put_y2(&mut dst.cr, dst_cr_off, &edge_uv, 0, uvlinesize, 8),
            _ => chroma_put_xy2(&mut dst.cr, dst_cr_off, &edge_uv, 0, uvlinesize, 8),
        }
        return;
    }

    // non-emu luma
    src_off = (src_y as usize) * linesize + (src_x as usize);
    let dst_x = mb_col * 16;
    let dst_y = mb_row * 16;
    let dxyu = (dxy as usize).min(7);

    let dst00 = dst_y * linesize + dst_x;
    wmv2_put_mspel_pixels(dxyu, &mut dst.y, dst00, src_plane, src_off, linesize);
    wmv2_put_mspel_pixels(
        dxyu,
        &mut dst.y,
        dst00 + 8,
        src_plane,
        src_off + 8,
        linesize,
    );
    wmv2_put_mspel_pixels(
        dxyu,
        &mut dst.y,
        dst00 + 8 * linesize,
        src_plane,
        src_off + 8 * linesize,
        linesize,
    );
    wmv2_put_mspel_pixels(
        dxyu,
        &mut dst.y,
        dst00 + 8 + 8 * linesize,
        src_plane,
        src_off + 8 + 8 * linesize,
        linesize,
    );

    // ---- Chroma ----
    if dst.cb.is_empty() || reference.cb.is_empty() {
        return;
    }

    let mut cdxy = 0usize;
    if (motion_x & 3) != 0 {
        cdxy |= 1;
    }
    if (motion_y & 3) != 0 {
        cdxy |= 2;
    }
    let mx = motion_x >> 2;
    let my = motion_y >> 2;

    let mut csrc_x = mb_col as i32 * 8 + mx;
    let mut csrc_y = mb_row as i32 * 8 + my;

    if csrc_x < -8 {
        csrc_x = -8;
    }
    if csrc_x > (dst.width as i32 >> 1) {
        csrc_x = dst.width as i32 >> 1;
    }
    if csrc_y < -8 {
        csrc_y = -8;
    }
    if csrc_y > (dst.height as i32 >> 1) {
        csrc_y = dst.height as i32 >> 1;
    }

    if csrc_x == (dst.width as i32 >> 1) {
        cdxy &= !1;
    }
    if csrc_y == (dst.height as i32 >> 1) {
        cdxy &= !2;
    }

    let uvlinesize = cw;
    let need_emu_uv =
        csrc_x < 0 || csrc_y < 0 || csrc_x + 9 >= cw as i32 || csrc_y + 9 >= ch as i32;
    if need_emu_uv {
        let mut edge_uv = vec![0u8; uvlinesize * 9];
        let dst_xc = mb_col * 8;
        let dst_yc = mb_row * 8;
        let dst_cb_off = dst_yc * uvlinesize + dst_xc;
        emulated_edge_mc(
            &mut edge_uv,
            uvlinesize,
            &reference.cb,
            uvlinesize,
            9,
            9,
            csrc_x,
            csrc_y,
            cw,
            ch,
        );
        match cdxy {
            0 => chroma_put_pixels(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
            1 => chroma_put_x2(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
            2 => chroma_put_y2(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
            _ => chroma_put_xy2(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
        }

        let dst_cr_off = dst_yc * uvlinesize + dst_xc;
        emulated_edge_mc(
            &mut edge_uv,
            uvlinesize,
            &reference.cr,
            uvlinesize,
            9,
            9,
            csrc_x,
            csrc_y,
            cw,
            ch,
        );
        match cdxy {
            0 => chroma_put_pixels(&mut dst.cr, dst_cr_off, &edge_uv, 0, uvlinesize, 8),
            1 => chroma_put_x2(&mut dst.cr, dst_cr_off, &edge_uv, 0, uvlinesize, 8),
            2 => chroma_put_y2(&mut dst.cr, dst_cr_off, &edge_uv, 0, uvlinesize, 8),
            _ => chroma_put_xy2(&mut dst.cr, dst_cr_off, &edge_uv, 0, uvlinesize, 8),
        }
        return;
    }
    let coff = (csrc_y as usize) * uvlinesize + (csrc_x as usize);
    let dst_xc = mb_col * 8;
    let dst_yc = mb_row * 8;
    let dst_cb_off = dst_yc * uvlinesize + dst_xc;

    match cdxy {
        0 => chroma_put_pixels(&mut dst.cb, dst_cb_off, &reference.cb, coff, uvlinesize, 8),
        1 => chroma_put_x2(&mut dst.cb, dst_cb_off, &reference.cb, coff, uvlinesize, 8),
        2 => chroma_put_y2(&mut dst.cb, dst_cb_off, &reference.cb, coff, uvlinesize, 8),
        _ => chroma_put_xy2(&mut dst.cb, dst_cb_off, &reference.cb, coff, uvlinesize, 8),
    }

    let dst_cr_off = dst_yc * uvlinesize + dst_xc;
    match cdxy {
        0 => chroma_put_pixels(&mut dst.cr, dst_cr_off, &reference.cr, coff, uvlinesize, 8),
        1 => chroma_put_x2(&mut dst.cr, dst_cr_off, &reference.cr, coff, uvlinesize, 8),
        2 => chroma_put_y2(&mut dst.cr, dst_cr_off, &reference.cr, coff, uvlinesize, 8),
        _ => chroma_put_xy2(&mut dst.cr, dst_cr_off, &reference.cr, coff, uvlinesize, 8),
    }
}

fn add_residual_block(
    frame: &mut YuvFrame,
    mb_row: u32,
    mb_col: u32,
    blk: usize,
    coeff: &[i32; 64],
) {
    let (is_luma, bx, by, stride, ph) =
        block_coords(mb_row, mb_col, blk, frame.width, frame.height);
    let plane: &mut Vec<u8> = if is_luma {
        &mut frame.y
    } else if blk == 4 {
        &mut frame.cb
    } else {
        &mut frame.cr
    };
    for r in 0..8 {
        if by + r >= ph {
            break;
        }
        for c in 0..8 {
            if bx + c >= stride {
                break;
            }
            let idx = (by + r) * stride + (bx + c);
            plane[idx] = (plane[idx] as i32 + coeff[r * 8 + c]).clamp(0, 255) as u8;
        }
    }
}

// ─── Macroblock Decoder ───────────────────────────────────────────────────────

// ─── upstream RLTable (WMV1/2/MSMPEG4) ───────────────────────────────────

const FF_RL_MAX_RUN: usize = 64;
const FF_RL_MAX_LEVEL: usize = 64;

#[derive(Clone)]
struct Wmv2Rl {
    n: usize,
    last: usize,
    vlc: VlcTree,
    run: &'static [u8],
    level: &'static [u8],
    max_level: [[u8; FF_RL_MAX_RUN + 1]; 2],
    max_run: [[u8; FF_RL_MAX_LEVEL + 1]; 2],
}

impl Wmv2Rl {
    fn new(base: &crate::na_rl_tables::RlBase) -> Self {
        let mut t = VlcTree::new();
        for (idx, (code, len)) in base.vlc.iter().enumerate() {
            if *len != 0 {
                t.insert(*code, *len, idx as i32);
            }
        }

        let mut max_level = [[0u8; FF_RL_MAX_RUN + 1]; 2];
        let mut max_run = [[0u8; FF_RL_MAX_LEVEL + 1]; 2];

        for last_flag in 0..2usize {
            let (start, end) = if last_flag == 0 {
                (0usize, base.last)
            } else {
                (base.last, base.n)
            };
            for i in start..end {
                let r = base.run[i] as usize;
                let l = base.level[i] as usize;
                if r <= FF_RL_MAX_RUN && l <= FF_RL_MAX_LEVEL {
                    if base.level[i] > max_level[last_flag][r] {
                        max_level[last_flag][r] = base.level[i];
                    }
                    if base.run[i] > max_run[last_flag][l] {
                        max_run[last_flag][l] = base.run[i];
                    }
                }
            }
        }

        Wmv2Rl {
            n: base.n,
            last: base.last,
            vlc: t,
            run: base.run,
            level: base.level,
            max_level,
            max_run,
        }
    }

    #[inline(always)]
    fn decode_sym(&self, br: &mut BitReader<'_>, qscale: i32) -> Option<(i32, i32)> {
        let idx = self.vlc.decode(br)? as usize;
        if idx == self.n {
            // Match upstream ff_rl_init_vlc(): escape maps to level==0, run==66.
            // Using run==0 can underflow i (starts at -1) and trigger OOB.
            return Some((0, 66));
        }
        let (qmul, qadd) = if qscale == 0 {
            (1i32, 0i32)
        } else {
            (qscale * 2, (qscale - 1) | 1)
        };
        let mut run = (self.run[idx] as i32) + 1;
        let level = (self.level[idx] as i32) * qmul + qadd;
        if idx >= self.last {
            run += 192;
        }
        Some((level, run))
    }

    #[inline(always)]
    fn max_level_for(&self, last: usize, run: usize) -> i32 {
        self.max_level[last.min(1)][run.min(FF_RL_MAX_RUN)] as i32
    }

    #[inline(always)]
    fn max_run_for(&self, last: usize, level: usize) -> i32 {
        self.max_run[last.min(1)][level.min(FF_RL_MAX_LEVEL)] as i32
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Vc1MvData {
    dx: i32,
    dy: i32,
    intra: bool,
    has_coeffs: bool,
}

#[inline(always)]
fn vc1_dc_scale(q: i32) -> i32 {
    const T: [i32; 32] = [
        0, 2, 4, 8, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17, 18,
        18, 19, 19, 20, 20, 21, 21,
    ];
    T[q.abs().clamp(1, 31) as usize]
}

#[inline(always)]
fn vc1_decode012_bits(br: &mut BitReader<'_>) -> Result<u8> {
    let first = br
        .read_bit()
        .ok_or_else(|| DecoderError::InvalidData("truncated VC-1 decode012".into()))?;
    if !first {
        Ok(0)
    } else {
        Ok(
            if br
                .read_bit()
                .ok_or_else(|| DecoderError::InvalidData("truncated VC-1 decode012".into()))?
            {
                2
            } else {
                1
            },
        )
    }
}

#[inline(always)]
fn vc1_decode210_bits(br: &mut BitReader<'_>) -> Result<u8> {
    // FFmpeg decode210(): 1 -> 0, 01 -> 1, 00 -> 2.
    let first = br
        .read_bit()
        .ok_or_else(|| DecoderError::InvalidData("truncated VC-1 escape selector".into()))?;
    if first {
        Ok(0)
    } else {
        Ok(
            if br
                .read_bit()
                .ok_or_else(|| DecoderError::InvalidData("truncated VC-1 escape selector".into()))?
            {
                1
            } else {
                2
            },
        )
    }
}

#[inline(always)]
fn vc1_read_unary_stop_one(br: &mut BitReader<'_>, max: u8) -> Result<u8> {
    let mut n = 0u8;
    while n < max {
        if br
            .read_bit()
            .ok_or_else(|| DecoderError::InvalidData("truncated VC-1 unary".into()))?
        {
            break;
        }
        n += 1;
    }
    Ok(n)
}

#[inline(always)]
fn vc1_scale_level(level: i32, mquant: i32, halfqp: bool, pquantizer: bool) -> i32 {
    if level == 0 {
        return 0;
    }
    let quant = mquant.abs().clamp(1, 31);
    let scale = quant * 2
        + if mquant < 0 {
            0
        } else if halfqp {
            1
        } else {
            0
        };
    let mut out = level * scale;
    if !pquantizer {
        out += if out < 0 { -quant } else { quant };
    }
    out
}

#[inline(always)]
fn vc1_rescale_dc_pred(v: i32, current_q: i32, neighbour_q: i32) -> i32 {
    const DQSCALE: [i32; 63] = [
        0x40000, 0x20000, 0x15555, 0x10000, 0x0CCCD, 0x0AAAB, 0x09249, 0x08000, 0x071C7, 0x06666,
        0x05D17, 0x05555, 0x04EC5, 0x04925, 0x04444, 0x04000, 0x03C3C, 0x038E4, 0x035E5, 0x03333,
        0x030C3, 0x02E8C, 0x02C86, 0x02AAB, 0x028F6, 0x02762, 0x025ED, 0x02492, 0x0234F, 0x02222,
        0x02108, 0x02000, 0x01F08, 0x01E1E, 0x01D42, 0x01C72, 0x01BAD, 0x01AF3, 0x01A42, 0x0199A,
        0x018FA, 0x01862, 0x017D0, 0x01746, 0x016C1, 0x01643, 0x015CA, 0x01555, 0x014E6, 0x0147B,
        0x01414, 0x013B1, 0x01352, 0x012F7, 0x0129E, 0x01249, 0x011F7, 0x011A8, 0x0115B, 0x01111,
        0x010C9, 0x01084, 0x01041,
    ];
    let q1 = current_q.abs().clamp(1, 31);
    let q2 = neighbour_q.abs().clamp(1, 31);
    if v == 0 || neighbour_q == 0 || q1 == q2 {
        return v;
    }
    let idx = vc1_dc_scale(q1) - 1;
    if !(0..63).contains(&idx) {
        return v;
    }
    (((v as i64) * vc1_dc_scale(q2) as i64 * DQSCALE[idx as usize] as i64 + 0x20000) >> 18) as i32
}

#[inline(always)]
fn vc1_rescale_ac_pred(v: i32, current_q: i32, neighbour_q: i32, halfqp: bool) -> i32 {
    const DQSCALE: [i32; 63] = [
        0x40000, 0x20000, 0x15555, 0x10000, 0x0CCCD, 0x0AAAB, 0x09249, 0x08000, 0x071C7, 0x06666,
        0x05D17, 0x05555, 0x04EC5, 0x04925, 0x04444, 0x04000, 0x03C3C, 0x038E4, 0x035E5, 0x03333,
        0x030C3, 0x02E8C, 0x02C86, 0x02AAB, 0x028F6, 0x02762, 0x025ED, 0x02492, 0x0234F, 0x02222,
        0x02108, 0x02000, 0x01F08, 0x01E1E, 0x01D42, 0x01C72, 0x01BAD, 0x01AF3, 0x01A42, 0x0199A,
        0x018FA, 0x01862, 0x017D0, 0x01746, 0x016C1, 0x01643, 0x015CA, 0x01555, 0x014E6, 0x0147B,
        0x01414, 0x013B1, 0x01352, 0x012F7, 0x0129E, 0x01249, 0x011F7, 0x011A8, 0x0115B, 0x01111,
        0x010C9, 0x01084, 0x01041,
    ];
    if v == 0 || neighbour_q == 0 || current_q == neighbour_q {
        return v;
    }
    let q1 = current_q.abs() * 2
        + if current_q < 0 {
            0
        } else if halfqp {
            1
        } else {
            0
        }
        - 1;
    let q2 = neighbour_q.abs() * 2
        + if neighbour_q < 0 {
            0
        } else if halfqp {
            1
        } else {
            0
        }
        - 1;
    if !(1..=63).contains(&q1) || q2 < 1 {
        return v;
    }
    (((v as i64) * (q2 as i64) * (DQSCALE[(q1 - 1) as usize] as i64) + 0x20000) >> 18) as i32
}

#[inline(always)]
fn vc1_bfraction_scale(num: i32, den: i32) -> i32 {
    // SMPTE 421M / FFmpeg use a fixed denominator of 256 for B-fraction
    // motion-vector scaling.  These are not the exact mathematical fractions:
    // e.g. 1/3 is represented by 85 and 2/3 by 170.  Using num/den directly
    // changes rounding, especially for negative vectors, and causes visible
    // drift/ghosting across B pictures.
    match (num, den) {
        (1, 2) => 128,
        (1, 3) => 85,
        (2, 3) => 170,
        (1, 4) => 64,
        (3, 4) => 192,
        (1, 5) => 51,
        (2, 5) => 102,
        (3, 5) => 153,
        (4, 5) => 204,
        (1, 6) => 43,
        (5, 6) => 215,
        (1, 7) => 37,
        (2, 7) => 74,
        (3, 7) => 111,
        (4, 7) => 148,
        (5, 7) => 185,
        (6, 7) => 222,
        (1, 8) => 32,
        (3, 8) => 96,
        (5, 8) => 160,
        (7, 8) => 224,
        _ => 0,
    }
}

#[inline(always)]
fn vc1_scale_b_mv(value: i32, num: i32, den: i32, inverse: bool, quarter_sample: bool) -> i32 {
    let mut n = vc1_bfraction_scale(num, den);
    if inverse {
        n -= 256;
    }
    // FFmpeg scale_mv(), B_FRACTION_DEN == 256.  Rust signed right shift is
    // arithmetic, matching the C implementation for the values used here.
    if quarter_sample {
        (value * n + 128) >> 8
    } else {
        2 * ((value * n + 255) >> 9)
    }
}

#[inline(always)]
fn vc1_sample_clamped(src: &[u8], stride: usize, w: usize, h: usize, x: i32, y: i32) -> i32 {
    if w == 0 || h == 0 || src.is_empty() {
        return 0;
    }
    let xx = x.clamp(0, w as i32 - 1) as usize;
    let yy = y.clamp(0, h as i32 - 1) as usize;
    src[yy * stride + xx] as i32
}

#[inline(always)]
fn vc1_mspel_raw4(a: i32, b: i32, c: i32, d: i32, mode: i32) -> i32 {
    match mode {
        1 => -4 * a + 53 * b + 18 * c - 3 * d,
        2 => -a + 9 * b + 9 * c - d,
        3 => -3 * a + 18 * b + 53 * c - 4 * d,
        _ => b,
    }
}

#[inline(always)]
fn vc1_mspel_one(a: i32, b: i32, c: i32, d: i32, mode: i32, r: i32) -> i32 {
    match mode {
        1 | 3 => (vc1_mspel_raw4(a, b, c, d, mode) + 32 - r) >> 6,
        2 => (vc1_mspel_raw4(a, b, c, d, mode) + 8 - r) >> 4,
        _ => b,
    }
}

fn vc1_mspel_block(
    dst: &mut [u8],
    dst_stride: usize,
    src: &[u8],
    src_stride: usize,
    sw: usize,
    sh: usize,
    src_x: i32,
    src_y: i32,
    bw: usize,
    bh: usize,
    hmode: i32,
    vmode: i32,
    rnd: bool,
) {
    let rnd_i = rnd as i32;

    // FFmpeg runs the normal MC path directly on the reference picture and
    // only invokes edge emulation for blocks whose filter footprint crosses a
    // picture boundary. The old Rust port called clamp() for every single tap,
    // even for interior macroblocks. At 1080p that means hundreds of millions
    // of redundant clamps/index calculations per second.
    let x0 = src_x as i64;
    let y0 = src_y as i64;
    let interior_hv =
        x0 >= 1 && y0 >= 1 && x0 + bw as i64 + 1 < sw as i64 && y0 + bh as i64 + 1 < sh as i64;
    let interior_v =
        x0 >= 0 && y0 >= 1 && x0 + bw as i64 <= sw as i64 && y0 + bh as i64 + 1 < sh as i64;
    let interior_h =
        x0 >= 1 && y0 >= 0 && x0 + bw as i64 + 1 < sw as i64 && y0 + bh as i64 <= sh as i64;
    let interior_copy =
        x0 >= 0 && y0 >= 0 && x0 + bw as i64 <= sw as i64 && y0 + bh as i64 <= sh as i64;

    if hmode != 0 && vmode != 0 {
        let shift_value = [0i32, 5, 1, 5];
        let shift = (shift_value[hmode as usize] + shift_value[vmode as usize]) >> 1;
        let r1 = (1 << (shift - 1)) + rnd_i - 1;
        let tw = bw + 3;
        // VC-1 luma MC is at most 16x16, so the separable-filter scratch
        // buffer is bounded by (16 + 3) * 16. Keep it on the stack: this
        // routine is called thousands of times per 1080p frame.
        let mut tmp_storage = [0i32; 19 * 16];
        let tmp = &mut tmp_storage[..tw * bh];

        if interior_hv {
            let sx = src_x as usize;
            let sy = src_y as usize;
            for y in 0..bh {
                let yy = sy + y;
                for tx in 0..tw {
                    let x = sx + tx - 1;
                    let raw = vc1_mspel_raw4(
                        src[(yy - 1) * src_stride + x] as i32,
                        src[yy * src_stride + x] as i32,
                        src[(yy + 1) * src_stride + x] as i32,
                        src[(yy + 2) * src_stride + x] as i32,
                        vmode,
                    );
                    tmp[y * tw + tx] = (raw + r1) >> shift;
                }
            }
        } else {
            for y in 0..bh {
                for tx in 0..tw {
                    let x = src_x + tx as i32 - 1;
                    let yy = src_y + y as i32;
                    let raw = vc1_mspel_raw4(
                        vc1_sample_clamped(src, src_stride, sw, sh, x, yy - 1),
                        vc1_sample_clamped(src, src_stride, sw, sh, x, yy),
                        vc1_sample_clamped(src, src_stride, sw, sh, x, yy + 1),
                        vc1_sample_clamped(src, src_stride, sw, sh, x, yy + 2),
                        vmode,
                    );
                    tmp[y * tw + tx] = (raw + r1) >> shift;
                }
            }
        }

        let r2 = 64 - rnd_i;
        for y in 0..bh {
            for x in 0..bw {
                let i = y * tw + x;
                let raw = vc1_mspel_raw4(tmp[i], tmp[i + 1], tmp[i + 2], tmp[i + 3], hmode);
                dst[y * dst_stride + x] = ((raw + r2) >> 7).clamp(0, 255) as u8;
            }
        }
    } else if vmode != 0 {
        let r = 1 - rnd_i;
        if interior_v {
            let sx = src_x as usize;
            let sy = src_y as usize;
            for y in 0..bh {
                let yy = sy + y;
                for x in 0..bw {
                    let xx = sx + x;
                    let v = vc1_mspel_one(
                        src[(yy - 1) * src_stride + xx] as i32,
                        src[yy * src_stride + xx] as i32,
                        src[(yy + 1) * src_stride + xx] as i32,
                        src[(yy + 2) * src_stride + xx] as i32,
                        vmode,
                        r,
                    );
                    dst[y * dst_stride + x] = v.clamp(0, 255) as u8;
                }
            }
        } else {
            for y in 0..bh {
                for x in 0..bw {
                    let xx = src_x + x as i32;
                    let yy = src_y + y as i32;
                    let v = vc1_mspel_one(
                        vc1_sample_clamped(src, src_stride, sw, sh, xx, yy - 1),
                        vc1_sample_clamped(src, src_stride, sw, sh, xx, yy),
                        vc1_sample_clamped(src, src_stride, sw, sh, xx, yy + 1),
                        vc1_sample_clamped(src, src_stride, sw, sh, xx, yy + 2),
                        vmode,
                        r,
                    );
                    dst[y * dst_stride + x] = v.clamp(0, 255) as u8;
                }
            }
        }
    } else if hmode != 0 {
        let r = rnd_i;
        if interior_h {
            let sx = src_x as usize;
            let sy = src_y as usize;
            for y in 0..bh {
                let yy = sy + y;
                let row = yy * src_stride;
                for x in 0..bw {
                    let xx = sx + x;
                    let v = vc1_mspel_one(
                        src[row + xx - 1] as i32,
                        src[row + xx] as i32,
                        src[row + xx + 1] as i32,
                        src[row + xx + 2] as i32,
                        hmode,
                        r,
                    );
                    dst[y * dst_stride + x] = v.clamp(0, 255) as u8;
                }
            }
        } else {
            for y in 0..bh {
                for x in 0..bw {
                    let xx = src_x + x as i32;
                    let yy = src_y + y as i32;
                    let v = vc1_mspel_one(
                        vc1_sample_clamped(src, src_stride, sw, sh, xx - 1, yy),
                        vc1_sample_clamped(src, src_stride, sw, sh, xx, yy),
                        vc1_sample_clamped(src, src_stride, sw, sh, xx + 1, yy),
                        vc1_sample_clamped(src, src_stride, sw, sh, xx + 2, yy),
                        hmode,
                        r,
                    );
                    dst[y * dst_stride + x] = v.clamp(0, 255) as u8;
                }
            }
        }
    } else if interior_copy {
        let sx = src_x as usize;
        let sy = src_y as usize;
        for y in 0..bh {
            let src_off = (sy + y) * src_stride + sx;
            let dst_off = y * dst_stride;
            dst[dst_off..dst_off + bw].copy_from_slice(&src[src_off..src_off + bw]);
        }
    } else {
        for y in 0..bh {
            for x in 0..bw {
                dst[y * dst_stride + x] =
                    vc1_sample_clamped(src, src_stride, sw, sh, src_x + x as i32, src_y + y as i32)
                        as u8;
            }
        }
    }
}

fn vc1_bilinear_qpel_block(
    dst: &mut [u8],
    dst_stride: usize,
    src: &[u8],
    src_stride: usize,
    sw: usize,
    sh: usize,
    src_x: i32,
    src_y: i32,
    bw: usize,
    bh: usize,
    fx: i32,
    fy: i32,
    denom: i32,
    rnd: bool,
) {
    // H.264-style chroma interpolation uses +32 before >>6 when rounded.
    // VC-1's no-round chroma primitive is *not* +31: FFmpeg/SMPTE use
    // +32-4 (+28) before >>6. Keep the generic half-pel case unchanged.
    let add = if denom == 8 && rnd {
        28
    } else {
        (denom * denom) / 2 - rnd as i32
    };
    let denom2 = denom * denom;
    let x0 = src_x as i64;
    let y0 = src_y as i64;
    let interior =
        x0 >= 0 && y0 >= 0 && x0 + (bw as i64) < sw as i64 && y0 + (bh as i64) < sh as i64;

    if interior {
        let sx = src_x as usize;
        let sy = src_y as usize;
        let wa = (denom - fx) * (denom - fy);
        let wb = fx * (denom - fy);
        let wc = (denom - fx) * fy;
        let wd = fx * fy;
        for y in 0..bh {
            let row0 = (sy + y) * src_stride + sx;
            let row1 = row0 + src_stride;
            let dst_row = y * dst_stride;
            for x in 0..bw {
                let v = src[row0 + x] as i32 * wa
                    + src[row0 + x + 1] as i32 * wb
                    + src[row1 + x] as i32 * wc
                    + src[row1 + x + 1] as i32 * wd;
                dst[dst_row + x] = ((v + add) / denom2).clamp(0, 255) as u8;
            }
        }
    } else {
        for y in 0..bh {
            for x in 0..bw {
                let xx = src_x + x as i32;
                let yy = src_y + y as i32;
                let a = vc1_sample_clamped(src, src_stride, sw, sh, xx, yy);
                let b = vc1_sample_clamped(src, src_stride, sw, sh, xx + 1, yy);
                let c = vc1_sample_clamped(src, src_stride, sw, sh, xx, yy + 1);
                let d = vc1_sample_clamped(src, src_stride, sw, sh, xx + 1, yy + 1);
                let v = a * (denom - fx) * (denom - fy)
                    + b * fx * (denom - fy)
                    + c * (denom - fx) * fy
                    + d * fx * fy;
                dst[y * dst_stride + x] = ((v + add) / denom2).clamp(0, 255) as u8;
            }
        }
    }
}

fn vc1_ic_value(v: u8, lumscale: u8, lumshift: u8, chroma: bool) -> u8 {
    let (scale, shift) = if lumscale == 0 {
        let mut sh = (255 - 2 * lumshift as i32) << 6;
        if lumshift > 31 {
            sh += 128 << 6;
        }
        (-64, sh)
    } else {
        let sh = if lumshift > 31 {
            (lumshift as i32 - 64) << 6
        } else {
            (lumshift as i32) << 6
        };
        (lumscale as i32 + 32, sh)
    };
    // FFmpeg INIT_LUT() deliberately applies LUMSHIFT only to luma.
    // Chroma is scaled around the neutral 128 centre and has no shift term:
    //   Y  = clip((scale * Y       + shift  + 32) >> 6)
    //   UV = clip((scale * (UV-128) + 128*64 + 32) >> 6)
    // Applying `shift` to Cb/Cr produces the green/magenta trails that are
    // most visible on intensity-compensated P pictures used for fades.
    if chroma {
        ((scale * (v as i32 - 128) + (128 << 6) + 32) >> 6).clamp(0, 255) as u8
    } else {
        ((scale * v as i32 + shift + 32) >> 6).clamp(0, 255) as u8
    }
}

/// Apply the progressive Simple/Main intensity-compensation LUT to a reference
/// picture. FFmpeg applies scale+shift to luma, but chroma is only scaled around
/// the neutral 128 centre; LUMSHIFT is not part of the chroma LUT. Keeping this
/// as a reference-picture
/// operation lets P and B pictures share the exact same ordering with
/// RANGEREDFRM: range reduction first, intensity compensation second.
fn vc1_apply_ic_reference(frame: &mut YuvFrame, lumscale: u8, lumshift: u8) {
    for v in &mut frame.y {
        *v = vc1_ic_value(*v, lumscale, lumshift, false);
    }
    for v in &mut frame.cb {
        *v = vc1_ic_value(*v, lumscale, lumshift, true);
    }
    for v in &mut frame.cr {
        *v = vc1_ic_value(*v, lumscale, lumshift, true);
    }
}

pub struct MacroblockDecoder {
    pub width: u32,
    pub height: u32,
    pub width_mb: u32,
    pub height_mb: u32,
    /// Reference frame for P/B decoding
    pub ref_frame: Option<YuvFrame>,
    // Exact VC-1 Simple/Main tables and per-picture predictor state.
    vc1_ac: [Vc1AcTable; 8],
    vc1_cbpcy: [VlcTable; 4],
    vc1_mvdata: [VlcTable; 4],
    vc1_ttmb: [VlcTable; 3],
    vc1_ttblk: [VlcTable; 3],
    vc1_subblkpat: [VlcTable; 3],
    vc1_esc3_level_length: u8,
    vc1_esc3_run_length: u8,
    vc1_coded_block: Vec<u8>,
    vc1_dc: Vec<[i32; 6]>,
    vc1_intra_blocks: Vec<[bool; 6]>,
    /// Signed post-inverse-transform blocks.  I pictures and P-picture intra
    /// blocks remain here until overlap/range processing is complete.
    vc1_recon_blocks: Vec<[i32; 64]>,
    /// Per-block residual coded-subblock pattern used by the VC-1 P loop filter.
    vc1_block_cbp: Vec<[u8; 6]>,
    /// Per-block transform type used by the VC-1 P loop filter.
    vc1_block_tt: Vec<[u8; 6]>,
    vc1_qscale: Vec<i32>,
    /// Advanced Profile CONDOVER=SELECT bitplane for the current I/BI picture.
    vc1_overflags: Vec<u8>,
    vc1_current_mvs: Vec<(i32, i32)>,
    vc1_mv4: Vec<[(i32, i32); 4]>,
    /// Derived chroma MV (FFmpeg luma_mv[]) for P-loop-filter decisions.
    vc1_chroma_mvs: Vec<(i32, i32)>,
    mv_pred: MvPredictor,
    vc1_bwd_anchor_mvs: Option<Vec<(i32, i32)>>,
    vc1_fwd_anchor_mvs: Option<Vec<(i32, i32)>>,
    vc1_rnd: bool,
    // ── WMV2 VLC tables (built lazily; shared with VC-1 decode machinery) ─────
    wmv2_inter: [VlcTable; 2], // ttcoef 0-1
    wmv2_intra: [VlcTable; 2],
    wmv2_cbpy: VlcTable,
    wmv2_cbpc: VlcTable,
    /// WMV2 reference frame (single-reference; no B-frame support)
    wmv2_ref: Option<YuvFrame>,
    // ── WMV2/MSMPEG4 (upstream-aligned) VLCs / state ─────────────────────────
    wmv2_mb_i_vlc: VlcTree,
    wmv2_dc_vlc: [[VlcTree; 2]; 2], // [dc_table_index][is_chroma]
    wmv2_coded_block: Vec<u8>,      // coded_block predictor grid (luma 8×8)
    wmv2_dc_pred: Wmv2DcPredBuffer,
    // ext-header flags (decode_ext_header)
    wmv2_mspel_bit: bool,
    wmv2_abt_flag: bool,
    wmv2_j_type_bit: bool,
    wmv2_top_left_mv_flag: bool,
    wmv2_per_mb_rl_bit: bool,
    // per-picture derived state (secondary picture header)
    wmv2_j_type: bool,
    wmv2_per_mb_rl_table: bool,
    wmv2_rl_table_index: u8,
    wmv2_rl_chroma_table_index: u8,
    wmv2_dc_table_index: usize,

    // P-picture secondary header state (upstream wmv2dec.c)
    wmv2_cbp_table_index: usize,
    wmv2_mv_table_index: usize,
    wmv2_mspel: bool,
    wmv2_hshift: u8,
    wmv2_per_mb_abt: bool,
    wmv2_abt_type: u8,
    wmv2_skip_type: u8,
    wmv2_slice_height: usize,
    wmv2_mb_skip: Vec<bool>,
    wmv2_motion: Vec<(i32, i32)>,

    // upstream MB and MV VLC tables
    wmv2_mb_non_intra_vlc: [VlcTree; 4],
    wmv2_mv_vlc: [VlcTree; 2],
    // upstream RL tables (run/level)
    wmv2_rl: [Wmv2Rl; 6],
    // WMV2 escape-3 adaptive lengths (reset each picture)
    wmv2_esc3_level_length: u8,
    wmv2_esc3_run_length: u8,
    // AC prediction buffer (16 values per block: [1..7] left, [9..15] top)
    wmv2_ac_val: Vec<[i16; 16]>,
    ac_pred: AcPredBuffer,
    /// Forward reference (anchor before B-frames in display order)
    fwd_ref: Option<YuvFrame>,
    /// Backward reference (anchor after B-frames in display order)
    bwd_ref: Option<YuvFrame>,
    /// Intensity compensation attached to the forward reference for the B
    /// pictures between the two anchors.  In FFmpeg this is `last_use_ic` /
    /// `last_luty` after the future anchor's P-picture header is parsed.
    /// The backward/current anchor uses the identity LUT in progressive
    /// Simple/Main profile.
    vc1_b_fwd_ic: Option<(u8, u8)>,
}

impl MacroblockDecoder {
    pub fn new(width: u32, height: u32) -> Self {
        let mb_w = width.div_ceil(16) as usize;
        let mb_h = height.div_ceil(16) as usize;

        // Build upstream MSMPEG4/WMV2 VLCs (MB I-table + DC tables).
        let wmv2_mb_i_vlc: VlcTree = {
            let mut t = VlcTree::new();
            for (sym, (code, len)) in FF_MSMP4_MB_I_TABLE.iter().enumerate() {
                t.insert(*code, *len, sym as i32);
            }
            t
        };

        let wmv2_dc_vlc: [[VlcTree; 2]; 2] = std::array::from_fn(|ti| {
            std::array::from_fn(|ch| {
                let mut t = VlcTree::new();
                for (sym, (code, len)) in FF_MSMP4_DC_TABLES[ti][ch].iter().enumerate() {
                    t.insert(*code, *len, sym as i32);
                }
                t
            })
        });

        // upstream mb_non_intra VLC tables (4 variants)
        let wmv2_mb_non_intra_vlc: [VlcTree; 4] = std::array::from_fn(|ti| {
            let mut t = VlcTree::new();
            for (sym, (code, len)) in FF_MB_NON_INTRA_TABLES[ti].iter().enumerate() {
                if *len != 0 {
                    t.insert(*code, *len, sym as i32);
                }
            }
            t
        });

        // upstream motion vector VLC tables (2 variants)
        // Built exactly like ff_vlc_init_tables_from_lengths() + ff_vlc_init_from_lengths()
        // (msmpeg4dec.c msmpeg4_decode_init_static).
        let build_mv_from_lengths = |lens: &[u8; 1100], syms: &[u16; 1100]| -> VlcTree {
            let mut t = VlcTree::new();
            let mut code: u32 = 0;
            for i in 0..1100usize {
                let len = lens[i] as i32;
                if len == 0 {
                    continue;
                }
                let l = len.unsigned_abs() as u8;
                // upstream stores code left-aligned in a 32-bit word.
                let right_aligned = if l == 0 { 0 } else { code >> (32 - l) };
                if len > 0 {
                    t.insert(right_aligned, l, syms[i] as i32);
                }
                code = code.wrapping_add(1u32 << (32 - l));
            }
            t
        };
        let wmv2_mv_vlc: [VlcTree; 2] = [
            build_mv_from_lengths(&FF_MSMP4_MV_TABLE0_LENS, &FF_MSMP4_MV_TABLE0),
            build_mv_from_lengths(&FF_MSMP4_MV_TABLE1_LENS, &FF_MSMP4_MV_TABLE1),
        ];

        // upstream RL tables (run/level)
        let wmv2_rl: [Wmv2Rl; 6] = std::array::from_fn(|i| Wmv2Rl::new(&FF_RL_BASES[i]));
        let wmv2_ac_val: Vec<[i16; 16]> = vec![[0i16; 16]; mb_w * mb_h * 6];

        MacroblockDecoder {
            width,
            height,
            width_mb: width.div_ceil(16),
            height_mb: height.div_ceil(16),
            ref_frame: None,
            vc1_ac: ac_tables(),
            vc1_cbpcy: cbpcy_vlcs(),
            vc1_mvdata: mvdata_vlcs(),
            vc1_ttmb: ttmb_vlcs(),
            vc1_ttblk: ttblk_vlcs(),
            vc1_subblkpat: subblkpat_vlcs(),
            vc1_esc3_level_length: 0,
            vc1_esc3_run_length: 0,
            vc1_coded_block: vec![0u8; (2 * mb_w) * (2 * mb_h)],
            vc1_dc: vec![[0i32; 6]; mb_w * mb_h],
            vc1_intra_blocks: vec![[false; 6]; mb_w * mb_h],
            vc1_recon_blocks: vec![[0i32; 64]; mb_w * mb_h * 6],
            vc1_block_cbp: vec![[0u8; 6]; mb_w * mb_h],
            vc1_block_tt: vec![[0u8; 6]; mb_w * mb_h],
            vc1_qscale: vec![0i32; mb_w * mb_h],
            vc1_overflags: vec![0u8; mb_w * mb_h],
            vc1_current_mvs: vec![(0, 0); mb_w * mb_h],
            vc1_mv4: vec![[(0, 0); 4]; mb_w * mb_h],
            vc1_chroma_mvs: vec![(0, 0); mb_w * mb_h],
            mv_pred: MvPredictor::new(mb_w, mb_h),
            vc1_bwd_anchor_mvs: None,
            vc1_fwd_anchor_mvs: None,
            vc1_rnd: false,
            ac_pred: AcPredBuffer::new(mb_w, mb_h),
            fwd_ref: None,
            bwd_ref: None,
            vc1_b_fwd_ic: None,
            wmv2_inter: [wmv2_tcoef_inter_vlc(0), wmv2_tcoef_inter_vlc(1)],
            wmv2_intra: [wmv2_tcoef_intra_vlc(0), wmv2_tcoef_intra_vlc(1)],
            wmv2_cbpy: wmv2_cbpy_vlc(),
            wmv2_cbpc: wmv2_cbpc_p_vlc(),
            wmv2_ref: None,

            // upstream-aligned WMV2/MSMPEG4 state
            wmv2_mb_i_vlc,
            wmv2_dc_vlc,
            wmv2_coded_block: vec![0u8; (2 * mb_w) * (2 * mb_h)],
            wmv2_dc_pred: Wmv2DcPredBuffer::new(mb_w, mb_h),

            // ext-header flags (default false until set_extradata)
            wmv2_mspel_bit: false,
            wmv2_abt_flag: false,
            wmv2_j_type_bit: false,
            wmv2_top_left_mv_flag: false,
            wmv2_per_mb_rl_bit: false,

            // per-picture derived state
            wmv2_j_type: false,
            wmv2_per_mb_rl_table: false,
            wmv2_rl_table_index: 0,
            wmv2_rl_chroma_table_index: 0,
            wmv2_dc_table_index: 0,

            wmv2_cbp_table_index: 0,
            wmv2_mv_table_index: 0,
            wmv2_mspel: false,
            wmv2_hshift: 0,
            wmv2_per_mb_abt: false,
            wmv2_abt_type: 0,
            wmv2_skip_type: 0,
            wmv2_slice_height: mb_h.max(1),
            wmv2_mb_skip: vec![false; mb_w * mb_h],
            wmv2_motion: vec![(0, 0); mb_w * mb_h],

            wmv2_mb_non_intra_vlc,
            wmv2_mv_vlc,

            wmv2_rl,
            wmv2_esc3_level_length: 0,
            wmv2_esc3_run_length: 0,
            wmv2_ac_val,
        }
    }

    pub fn decode_frame(
        &mut self,
        payload: &[u8],
        pic_hdr: &PictureHeader,
        seq: &SequenceHeader,
        frame: &mut YuvFrame,
    ) -> Result<()> {
        if let Some(rnd) = pic_hdr.rnd {
            // Advanced Profile carries RNDCTRL in every non-skipped picture.
            // Simple/Main derives the state by toggling it at picture boundaries.
            self.vc1_rnd = rnd;
        } else {
            match pic_hdr.frame_type {
                FrameType::I | FrameType::BI => self.vc1_rnd = true,
                FrameType::P => self.vc1_rnd = !self.vc1_rnd,
                _ => {}
            }
        }

        match pic_hdr.frame_type {
            FrameType::I | FrameType::BI => {
                self.decode_intra(payload, pic_hdr, seq, frame)?;
                self.vc1_finish_i_reconstruction(frame, pic_hdr, seq);
                if seq.loop_filter {
                    vc1_apply_i_loop_filter(
                        frame,
                        pic_hdr.pquant as i32,
                        self.width_mb as usize,
                        self.height_mb as usize,
                    );
                }
            }
            FrameType::P => {
                self.decode_p(payload, pic_hdr, seq, frame)?;
                self.vc1_finish_p_reconstruction(frame, pic_hdr, seq);
                if seq.loop_filter {
                    self.vc1_apply_p_loop_filter(frame, pic_hdr.pquant as i32);
                }
            }
            FrameType::B => {
                self.decode_b(payload, pic_hdr, seq, frame)?;
                // FFmpeg uses the I-picture loop-filter traversal for
                // progressive B pictures.
                if seq.loop_filter {
                    vc1_apply_i_loop_filter(
                        frame,
                        pic_hdr.pquant as i32,
                        self.width_mb as usize,
                        self.height_mb as usize,
                    );
                }
            }
            FrameType::Skipped => {
                if let Some(ref rf) = self.ref_frame {
                    frame.y.copy_from_slice(&rf.y);
                    frame.cb.copy_from_slice(&rf.cb);
                    frame.cr.copy_from_slice(&rf.cr);
                }
            }
        }

        // Anchor frames (I/P) become references.  RANGEREDFRM is a property
        // of the *current* picture's MC operation; references are never
        // destructively compressed when stored.
        match pic_hdr.frame_type {
            FrameType::B | FrameType::BI => {}
            _ => {
                self.fwd_ref = self.bwd_ref.take();
                self.bwd_ref = Some(frame.clone());
                self.ref_frame = Some(frame.clone());
                self.vc1_fwd_anchor_mvs = self.vc1_bwd_anchor_mvs.take();
                self.vc1_bwd_anchor_mvs = Some(self.vc1_current_mvs.clone());

                // ff_vc1_parse_frame_header() records P-picture intensity
                // compensation on `last_luty`, i.e. on the older anchor that
                // becomes the forward reference for following B pictures.
                // B/BI pictures do not rotate or replace this state.
                self.vc1_b_fwd_ic = if pic_hdr.frame_type == FrameType::P
                    && pic_hdr.mv_mode == MvMode::IntensityComp
                {
                    Some((pic_hdr.lumscale, pic_hdr.lumshift))
                } else {
                    None
                };
            }
        }
        Ok(())
    }

    // ─── VC-1 macroblock helpers ───────────────────────────────────────────

    fn vc1_reset_picture_state(&mut self) {
        self.vc1_coded_block.fill(0);
        self.vc1_dc.fill([0; 6]);
        self.vc1_intra_blocks.fill([false; 6]);
        self.vc1_recon_blocks.fill([0; 64]);
        self.vc1_block_cbp.fill([0; 6]);
        self.vc1_block_tt.fill([0; 6]);
        self.vc1_qscale.fill(0);
        self.vc1_overflags.fill(0);
        self.vc1_current_mvs.fill((0, 0));
        self.vc1_mv4.fill([(0, 0); 4]);
        self.vc1_chroma_mvs.fill((0, 0));
        self.vc1_esc3_level_length = 0;
        self.vc1_esc3_run_length = 0;
        self.ac_pred.clear();
    }

    fn vc1_finish_i_reconstruction(
        &mut self,
        frame: &mut YuvFrame,
        pic: &PictureHeader,
        seq: &SequenceHeader,
    ) {
        let mb_w = self.width_mb as usize;
        let mb_h = self.height_mb as usize;

        if seq.profile == Profile::Advanced {
            // FFmpeg vc1_decode_i_blocks_adv(): Advanced Profile always uses
            // signed inverse-transform samples and `vc1_put_blocks_clamped(...,
            // 1)`, i.e. the +128 reconstruction path, even when overlap is
            // disabled. At low quantizers CONDOVER controls whether overlap is
            // off, enabled everywhere, or selected by the OVERFLAGS bitplane.
            if seq.overlap {
                if pic.pquant >= 9 || pic.condover == 1 {
                    vc1_apply_signed_overlap(
                        &mut self.vc1_recon_blocks,
                        &self.vc1_intra_blocks,
                        mb_w,
                        mb_h,
                        false,
                    );
                } else if pic.condover == 2 {
                    vc1_apply_advanced_selective_i_overlap(
                        &mut self.vc1_recon_blocks,
                        mb_w,
                        mb_h,
                        &self.vc1_overflags,
                    );
                }
            }

            for r in 0..mb_h {
                for c in 0..mb_w {
                    for blk in 0..6usize {
                        let ri = vc1_recon_index(mb_w, r, c, blk);
                        write_intra_block(
                            frame,
                            r as u32,
                            c as u32,
                            blk,
                            &self.vc1_recon_blocks[ri],
                        );
                    }
                }
            }
            return;
        }

        let overlap = seq.overlap && pic.pquant >= 9;

        if overlap {
            vc1_apply_signed_overlap(
                &mut self.vc1_recon_blocks,
                &self.vc1_intra_blocks,
                mb_w,
                mb_h,
                pic.rangeredfrm,
            );
        }

        // FFmpeg vc1_decode_i_blocks(): for the high-quant overlap path,
        // RANGEREDFRM multiplies the signed block after overlap and the result
        // is put with +128.  Without overlap, it uses (block - 64) * 2 and
        // put_pixels_clamped() (no signed +128 bias).
        for r in 0..mb_h {
            for c in 0..mb_w {
                for blk in 0..6usize {
                    let ri = vc1_recon_index(mb_w, r, c, blk);
                    let block = &mut self.vc1_recon_blocks[ri];
                    if pic.rangeredfrm && !overlap {
                        for v in block.iter_mut() {
                            *v = (*v - 64) * 2;
                        }
                    }
                    if overlap {
                        write_intra_block(frame, r as u32, c as u32, blk, block);
                    } else {
                        write_intra_block_unsigned(frame, r as u32, c as u32, blk, block);
                    }
                }
            }
        }
    }

    fn vc1_finish_p_reconstruction(
        &mut self,
        frame: &mut YuvFrame,
        pic: &PictureHeader,
        seq: &SequenceHeader,
    ) {
        let mb_w = self.width_mb as usize;
        let mb_h = self.height_mb as usize;

        // In a P picture FFmpeg multiplies RANGEREDFRM intra blocks by two
        // immediately after inverse transform, then performs overlap only when
        // both neighboring blocks are intra.  The multiply is done during
        // decode_p(); only overlap and delayed signed write remain here.
        if seq.overlap && pic.pquant >= 9 {
            vc1_apply_signed_overlap(
                &mut self.vc1_recon_blocks,
                &self.vc1_intra_blocks,
                mb_w,
                mb_h,
                false,
            );
        }

        for r in 0..mb_h {
            for c in 0..mb_w {
                let mbi = r * mb_w + c;
                for blk in 0..6usize {
                    if !self.vc1_intra_blocks[mbi][blk] {
                        continue;
                    }
                    let ri = vc1_recon_index(mb_w, r, c, blk);
                    write_intra_block(frame, r as u32, c as u32, blk, &self.vc1_recon_blocks[ri]);
                }
            }
        }
    }

    #[inline]
    fn vc1_p_loop_meta(
        &self,
        plane: usize,
        block_x: usize,
        block_y: usize,
    ) -> (bool, u8, u8, (i32, i32)) {
        let mb_w = self.width_mb as usize;
        if plane == 0 {
            let mb_col = block_x / 2;
            let mb_row = block_y / 2;
            let blk = (block_y & 1) * 2 + (block_x & 1);
            let mbi = mb_row * mb_w + mb_col;
            (
                self.vc1_intra_blocks[mbi][blk],
                self.vc1_block_cbp[mbi][blk],
                self.vc1_block_tt[mbi][blk],
                self.vc1_mv4[mbi][blk],
            )
        } else {
            let blk = if plane == 1 { 4 } else { 5 };
            let mbi = block_y * mb_w + block_x;
            (
                self.vc1_intra_blocks[mbi][blk],
                self.vc1_block_cbp[mbi][blk],
                self.vc1_block_tt[mbi][blk],
                self.vc1_chroma_mvs[mbi],
            )
        }
    }

    #[inline]
    fn vc1_p_v_loop_filter_block(
        &self,
        plane: &mut [u8],
        width: usize,
        height: usize,
        plane_no: usize,
        bx: usize,
        by: usize,
        pq: i32,
    ) {
        let blocks_w = if plane_no == 0 {
            self.width_mb as usize * 2
        } else {
            self.width_mb as usize
        };
        let blocks_h = if plane_no == 0 {
            self.height_mb as usize * 2
        } else {
            self.height_mb as usize
        };
        if bx >= blocks_w || by >= blocks_h {
            return;
        }

        let x0 = bx * 8;
        let y0 = by * 8;
        if x0 >= width || y0 >= height {
            return;
        }
        let top = self.vc1_p_loop_meta(plane_no, bx, by);

        // ff_vc1_p_v_loop_filter(): the 8-pixel edge below this block.
        if by + 1 < blocks_h && (by + 1) * 8 < height {
            let bottom = self.vc1_p_loop_meta(plane_no, bx, by + 1);
            let edge_y = (by + 1) * 8;
            if top.0 || bottom.0 || top.3 != bottom.3 {
                vc1_filter_horizontal_boundary(plane, width, height, x0, edge_y, 8, pq);
            } else {
                let idx = (top.1 | (bottom.1 >> 2)) & 3;
                if (idx & 1) != 0 {
                    vc1_filter_horizontal_boundary(plane, width, height, x0 + 4, edge_y, 4, pq);
                }
                if (idx & 2) != 0 {
                    vc1_filter_horizontal_boundary(plane, width, height, x0, edge_y, 4, pq);
                }
            }
        }

        // Internal 4-pixel transform edge of this 8x8 block.
        if matches!(top.2, TT_4X4 | TT_8X4) {
            let edge_y = y0 + 4;
            if edge_y < height {
                if (top.1 & 5) != 0 {
                    vc1_filter_horizontal_boundary(plane, width, height, x0 + 4, edge_y, 4, pq);
                }
                if (top.1 & 10) != 0 {
                    vc1_filter_horizontal_boundary(plane, width, height, x0, edge_y, 4, pq);
                }
            }
        }
    }

    #[inline]
    fn vc1_p_h_loop_filter_block(
        &self,
        plane: &mut [u8],
        width: usize,
        height: usize,
        plane_no: usize,
        bx: usize,
        by: usize,
        pq: i32,
    ) {
        let blocks_w = if plane_no == 0 {
            self.width_mb as usize * 2
        } else {
            self.width_mb as usize
        };
        let blocks_h = if plane_no == 0 {
            self.height_mb as usize * 2
        } else {
            self.height_mb as usize
        };
        if bx >= blocks_w || by >= blocks_h {
            return;
        }

        let x0 = bx * 8;
        let y0 = by * 8;
        if x0 >= width || y0 >= height {
            return;
        }
        let left = self.vc1_p_loop_meta(plane_no, bx, by);

        // ff_vc1_p_h_loop_filter(): the 8-pixel edge right of this block.
        if bx + 1 < blocks_w && (bx + 1) * 8 < width {
            let right = self.vc1_p_loop_meta(plane_no, bx + 1, by);
            let edge_x = (bx + 1) * 8;
            if left.0 || right.0 || left.3 != right.3 {
                vc1_filter_vertical_boundary(plane, width, height, edge_x, y0, 8, pq);
            } else {
                let idx = (left.1 | (right.1 >> 1)) & 5;
                if (idx & 1) != 0 {
                    vc1_filter_vertical_boundary(plane, width, height, edge_x, y0 + 4, 4, pq);
                }
                if (idx & 4) != 0 {
                    vc1_filter_vertical_boundary(plane, width, height, edge_x, y0, 4, pq);
                }
            }
        }

        // Internal 4-pixel transform edge of this 8x8 block.
        if matches!(left.2, TT_4X4 | TT_4X8) {
            let edge_x = x0 + 4;
            if edge_x < width {
                if (left.1 & 3) != 0 {
                    vc1_filter_vertical_boundary(plane, width, height, edge_x, y0 + 4, 4, pq);
                }
                if (left.1 & 12) != 0 {
                    vc1_filter_vertical_boundary(plane, width, height, edge_x, y0, 4, pq);
                }
            }
        }
    }

    fn vc1_p_v_loop_filter_mb(&self, frame: &mut YuvFrame, mb_row: usize, mb_col: usize, pq: i32) {
        let w = frame.width as usize;
        let h = frame.height as usize;
        for local_y in 0..2 {
            for local_x in 0..2 {
                self.vc1_p_v_loop_filter_block(
                    &mut frame.y,
                    w,
                    h,
                    0,
                    mb_col * 2 + local_x,
                    mb_row * 2 + local_y,
                    pq,
                );
            }
        }

        let cw = w / 2;
        let ch = h / 2;
        self.vc1_p_v_loop_filter_block(&mut frame.cb, cw, ch, 1, mb_col, mb_row, pq);
        self.vc1_p_v_loop_filter_block(&mut frame.cr, cw, ch, 2, mb_col, mb_row, pq);
    }

    fn vc1_p_h_loop_filter_mb(&self, frame: &mut YuvFrame, mb_row: usize, mb_col: usize, pq: i32) {
        let w = frame.width as usize;
        let h = frame.height as usize;
        for local_y in 0..2 {
            for local_x in 0..2 {
                self.vc1_p_h_loop_filter_block(
                    &mut frame.y,
                    w,
                    h,
                    0,
                    mb_col * 2 + local_x,
                    mb_row * 2 + local_y,
                    pq,
                );
            }
        }

        let cw = w / 2;
        let ch = h / 2;
        self.vc1_p_h_loop_filter_block(&mut frame.cb, cw, ch, 1, mb_col, mb_row, pq);
        self.vc1_p_h_loop_filter_block(&mut frame.cr, cw, ch, 2, mb_col, mb_row, pq);
    }

    /// Reproduce the macroblock wavefront used by FFmpeg
    /// `ff_vc1_p_loop_filter()`.  P filtering trails overlap by two rows/two
    /// columns; running all V edges and then all H edges changes the samples at
    /// edge crossings and is not bit-exact.
    fn vc1_apply_p_loop_filter(&self, frame: &mut YuvFrame, pq: i32) {
        let mb_width = self.width_mb as usize;
        let mb_height = self.height_mb as usize;
        if mb_width == 0 || mb_height == 0 {
            return;
        }

        for cur_y in 0..mb_height {
            for cur_x in 0..mb_width {
                // V-loop portion of ff_vc1_p_loop_filter().
                if cur_y >= 2 {
                    if cur_x >= 1 {
                        self.vc1_p_v_loop_filter_mb(frame, cur_y - 2, cur_x - 1, pq);
                    }
                    if cur_x + 1 == mb_width {
                        self.vc1_p_v_loop_filter_mb(frame, cur_y - 2, cur_x, pq);
                    }
                }
                if cur_y + 1 == mb_height {
                    if cur_x >= 1 {
                        if cur_y >= 1 {
                            self.vc1_p_v_loop_filter_mb(frame, cur_y - 1, cur_x - 1, pq);
                        }
                        self.vc1_p_v_loop_filter_mb(frame, cur_y, cur_x - 1, pq);
                    }
                    if cur_x + 1 == mb_width {
                        if cur_y >= 1 {
                            self.vc1_p_v_loop_filter_mb(frame, cur_y - 1, cur_x, pq);
                        }
                        self.vc1_p_v_loop_filter_mb(frame, cur_y, cur_x, pq);
                    }
                }

                // H-loop portion.  It trails by one additional MB column.
                if cur_y >= 2 {
                    if cur_x >= 2 {
                        self.vc1_p_h_loop_filter_mb(frame, cur_y - 2, cur_x - 2, pq);
                    }
                    if cur_x + 1 == mb_width {
                        if cur_x >= 1 {
                            self.vc1_p_h_loop_filter_mb(frame, cur_y - 2, cur_x - 1, pq);
                        }
                        self.vc1_p_h_loop_filter_mb(frame, cur_y - 2, cur_x, pq);
                    }
                }
                if cur_y + 1 == mb_height {
                    if cur_y >= 1 {
                        if cur_x >= 2 {
                            self.vc1_p_h_loop_filter_mb(frame, cur_y - 1, cur_x - 2, pq);
                        }
                        if cur_x + 1 == mb_width {
                            if cur_x >= 1 {
                                self.vc1_p_h_loop_filter_mb(frame, cur_y - 1, cur_x - 1, pq);
                            }
                            self.vc1_p_h_loop_filter_mb(frame, cur_y - 1, cur_x, pq);
                        }
                    }
                    if cur_x >= 2 {
                        self.vc1_p_h_loop_filter_mb(frame, cur_y, cur_x - 2, pq);
                    }
                    if cur_x + 1 == mb_width {
                        if cur_x >= 1 {
                            self.vc1_p_h_loop_filter_mb(frame, cur_y, cur_x - 1, pq);
                        }
                        self.vc1_p_h_loop_filter_mb(frame, cur_y, cur_x, pq);
                    }
                }
            }
        }
    }

    #[inline]
    fn vc1_coding_sets(pic: &PictureHeader) -> (usize, usize) {
        let intra = match pic.transacfrm2 {
            0 => {
                if pic.pqindex <= 8 {
                    6
                } else {
                    2
                }
            }
            1 => 0,
            _ => 4,
        };
        let inter = match pic.transacfrm {
            0 => {
                if pic.pqindex <= 8 {
                    7
                } else {
                    3
                }
            }
            1 => 1,
            _ => 5,
        };
        (intra, inter)
    }

    #[inline]
    fn vc1_read_dc_diff(
        &self,
        br: &mut BitReader<'_>,
        pic: &PictureHeader,
        chroma: bool,
        quant: i32,
    ) -> Result<i32> {
        let table = &self.wmv2_dc_vlc[pic.dctab as usize][chroma as usize];
        let mut d = table
            .decode(br)
            .ok_or_else(|| DecoderError::InvalidData("invalid WMV3 DC VLC".into()))?;
        if d != 0 {
            let m = if quant == 1 || quant == 2 {
                3 - quant
            } else {
                0
            };
            if d == 119 {
                d = br
                    .read_bits((8 + m) as u8)
                    .ok_or_else(|| DecoderError::InvalidData("truncated WMV3 DC escape".into()))?
                    as i32;
            } else if m != 0 {
                let ext = br.read_bits(m as u8).ok_or_else(|| {
                    DecoderError::InvalidData("truncated WMV3 DC extension".into())
                })? as i32;
                d = (d << m) + ext - ((1 << m) - 1);
            }
            if br
                .read_bit()
                .ok_or_else(|| DecoderError::InvalidData("truncated WMV3 DC sign".into()))?
            {
                d = -d;
            }
        }
        Ok(d)
    }

    fn vc1_decode_ac_coeff(
        &mut self,
        br: &mut BitReader<'_>,
        set: usize,
        pq: i32,
        dquantfrm: bool,
    ) -> Result<(usize, i32, bool)> {
        let set = set.min(7);
        let idx = self.vc1_ac[set]
            .decode_index(br)
            .ok_or_else(|| DecoderError::InvalidData("invalid WMV3 AC VLC".into()))?;
        let (run, level, last, sign) = if !self.vc1_ac[set].is_escape(idx) {
            let (r, l) = self.vc1_ac[set]
                .run_level(idx)
                .ok_or_else(|| DecoderError::InvalidData("invalid WMV3 AC symbol".into()))?;
            let sign = br
                .read_bit()
                .ok_or_else(|| DecoderError::InvalidData("truncated WMV3 AC sign".into()))?;
            (r as usize, l as i32, self.vc1_ac[set].is_last(idx), sign)
        } else {
            let escape = vc1_decode210_bits(br)?;
            if escape != 2 {
                let idx2 = self.vc1_ac[set].decode_index(br).ok_or_else(|| {
                    DecoderError::InvalidData("invalid WMV3 escaped AC VLC".into())
                })?;
                if self.vc1_ac[set].is_escape(idx2) {
                    return Err(DecoderError::InvalidData("nested WMV3 AC escape".into()));
                }
                let (r0, l0) = self.vc1_ac[set].run_level(idx2).ok_or_else(|| {
                    DecoderError::InvalidData("invalid WMV3 escaped AC symbol".into())
                })?;
                let last = self.vc1_ac[set].is_last(idx2);
                let (r, l) = if escape == 0 {
                    (
                        r0 as usize,
                        l0 as i32 + self.vc1_ac[set].max_level(last, r0 as usize) as i32,
                    )
                } else {
                    (
                        r0 as usize + self.vc1_ac[set].max_run(last, l0 as usize) as usize + 1,
                        l0 as i32,
                    )
                };
                let sign = br.read_bit().ok_or_else(|| {
                    DecoderError::InvalidData("truncated WMV3 escaped AC sign".into())
                })?;
                (r, l, last, sign)
            } else {
                let last = br.read_bit().ok_or_else(|| {
                    DecoderError::InvalidData("truncated WMV3 escape3 last".into())
                })?;
                if self.vc1_esc3_level_length == 0 {
                    let ll = if pq < 8 || dquantfrm {
                        let n = br.read_bits(3).ok_or_else(|| {
                            DecoderError::InvalidData("truncated WMV3 escape3 level length".into())
                        })? as u8;
                        if n == 0 {
                            br.read_bits(2).ok_or_else(|| {
                                DecoderError::InvalidData(
                                    "truncated WMV3 escape3 extended level length".into(),
                                )
                            })? as u8
                                + 8
                        } else {
                            n
                        }
                    } else {
                        vc1_read_unary_stop_one(br, 6)? + 2
                    };
                    self.vc1_esc3_level_length = ll;
                    self.vc1_esc3_run_length = 3 + br.read_bits(2).ok_or_else(|| {
                        DecoderError::InvalidData("truncated WMV3 escape3 run length".into())
                    })? as u8;
                }
                let r = br
                    .read_bits(self.vc1_esc3_run_length)
                    .ok_or_else(|| DecoderError::InvalidData("truncated WMV3 escape3 run".into()))?
                    as usize;
                let sign = br.read_bit().ok_or_else(|| {
                    DecoderError::InvalidData("truncated WMV3 escape3 sign".into())
                })?;
                let l = br.read_bits(self.vc1_esc3_level_length).ok_or_else(|| {
                    DecoderError::InvalidData("truncated WMV3 escape3 level".into())
                })? as i32;
                (r, l, last, sign)
            }
        };
        Ok((run, if sign { -level } else { level }, last))
    }

    #[inline]
    fn vc1_mb_index(&self, r: usize, c: usize) -> Option<usize> {
        if r < self.height_mb as usize && c < self.width_mb as usize {
            Some(r * self.width_mb as usize + c)
        } else {
            None
        }
    }

    fn vc1_luma_slot(&self, gx: isize, gy: isize) -> Option<(usize, usize)> {
        if gx < 0 || gy < 0 {
            return None;
        }
        let gx = gx as usize;
        let gy = gy as usize;
        let c = gx / 2;
        let r = gy / 2;
        let idx = self.vc1_mb_index(r, c)?;
        let blk = (gy & 1) * 2 + (gx & 1);
        Some((idx, blk))
    }

    fn vc1_dc_neighbours(
        &self,
        r: usize,
        c: usize,
        blk: usize,
    ) -> (Option<i32>, Option<i32>, Option<i32>) {
        if blk < 4 {
            let gx = (c * 2 + (blk & 1)) as isize;
            let gy = (r * 2 + (blk >> 1)) as isize;
            let get = |x: isize, y: isize| self.vc1_luma_slot(x, y).map(|(i, b)| self.vc1_dc[i][b]);
            (get(gx, gy - 1), get(gx - 1, gy - 1), get(gx - 1, gy))
        } else {
            let get = |rr: isize, cc: isize| -> Option<i32> {
                if rr < 0 || cc < 0 {
                    return None;
                }
                self.vc1_mb_index(rr as usize, cc as usize)
                    .map(|i| self.vc1_dc[i][blk])
            };
            (
                get(r as isize - 1, c as isize),
                get(r as isize - 1, c as isize - 1),
                get(r as isize, c as isize - 1),
            )
        }
    }

    fn vc1_i_dc_pred(
        &self,
        r: usize,
        c: usize,
        blk: usize,
        seq: &SequenceHeader,
        pq: i32,
    ) -> (i32, bool) {
        const DCPRED: [i32; 32] = [
            -1, 1024, 512, 341, 256, 205, 171, 146, 128, 114, 102, 93, 85, 79, 73, 68, 64, 60, 57,
            54, 51, 49, 47, 45, 43, 41, 39, 38, 37, 35, 34, 33,
        ];
        let (oa, ob, oc) = self.vc1_dc_neighbours(r, c, blk);
        let boundary = if pq < 9 || !seq.overlap {
            DCPRED[vc1_dc_scale(pq) as usize]
        } else {
            0
        };
        let a = oa.unwrap_or(boundary);
        let b = ob.unwrap_or(boundary);
        let cc = oc.unwrap_or(boundary);
        if (a - b).abs() <= (b - cc).abs() {
            (cc, true)
        } else {
            (a, false)
        }
    }

    fn vc1_intra_neighbour(
        &self,
        r: usize,
        c: usize,
        blk: usize,
        top: bool,
    ) -> Option<(usize, usize)> {
        if blk < 4 {
            let gx = (c * 2 + (blk & 1)) as isize;
            let gy = (r * 2 + (blk >> 1)) as isize;
            let (x, y) = if top { (gx, gy - 1) } else { (gx - 1, gy) };
            self.vc1_luma_slot(x, y)
        } else {
            let (rr, cc) = if top {
                (r as isize - 1, c as isize)
            } else {
                (r as isize, c as isize - 1)
            };
            if rr < 0 || cc < 0 {
                return None;
            }
            self.vc1_mb_index(rr as usize, cc as usize)
                .map(|i| (i, blk))
        }
    }

    fn vc1_inter_dc_pred(
        &self,
        r: usize,
        c: usize,
        blk: usize,
        mquant: i32,
        _halfqp: bool,
    ) -> (i32, bool, bool, bool, i32) {
        let cur_idx = self.vc1_mb_index(r, c).unwrap_or(0);
        let top_slot = self.vc1_intra_neighbour(r, c, blk, true);
        let left_slot = self.vc1_intra_neighbour(r, c, blk, false);
        let diag_slot = if blk < 4 {
            let gx = (c * 2 + (blk & 1)) as isize;
            let gy = (r * 2 + (blk >> 1)) as isize;
            self.vc1_luma_slot(gx - 1, gy - 1)
        } else if r > 0 && c > 0 {
            self.vc1_mb_index(r - 1, c - 1).map(|i| (i, blk))
        } else {
            None
        };
        let a_av = top_slot
            .map(|(i, b)| self.vc1_intra_blocks[i][b])
            .unwrap_or(false);
        let c_av = left_slot
            .map(|(i, b)| self.vc1_intra_blocks[i][b])
            .unwrap_or(false);
        let mut a = top_slot
            .filter(|(i, b)| self.vc1_intra_blocks[*i][*b])
            .map(|(i, b)| self.vc1_dc[i][b])
            .unwrap_or(0);
        let mut cc = left_slot
            .filter(|(i, b)| self.vc1_intra_blocks[*i][*b])
            .map(|(i, b)| self.vc1_dc[i][b])
            .unwrap_or(0);
        let mut b = diag_slot.map(|(i, b)| self.vc1_dc[i][b]).unwrap_or(0);
        if a_av && let Some((i, _)) = top_slot {
            a = vc1_rescale_dc_pred(a, mquant, self.vc1_qscale[i]);
        }
        if c_av && let Some((i, _)) = left_slot {
            cc = vc1_rescale_dc_pred(cc, mquant, self.vc1_qscale[i]);
        }
        // ff_vc1_pred_dc rescales B only when both directional predictors
        // are available and block 3 is not being decoded.
        if a_av
            && c_av
            && blk != 3
            && let Some((i, _)) = diag_slot
        {
            b = vc1_rescale_dc_pred(b, mquant, self.vc1_qscale[i]);
        }
        let (pred, left) = if c_av && (!a_av || (a - b).abs() <= (b - cc).abs()) {
            (cc, true)
        } else if a_av {
            (a, false)
        } else {
            (0, true)
        };
        let q2 = if left {
            left_slot
                .map(|(i, _)| self.vc1_qscale[i])
                .unwrap_or(self.vc1_qscale[cur_idx])
        } else {
            top_slot
                .map(|(i, _)| self.vc1_qscale[i])
                .unwrap_or(self.vc1_qscale[cur_idx])
        };
        (pred, left, a_av, c_av, q2)
    }

    fn vc1_decode_intra_coeffs(
        &mut self,
        br: &mut BitReader<'_>,
        pic: &PictureHeader,
        seq: &SequenceHeader,
        r: usize,
        c: usize,
        blk: usize,
        coded: bool,
        mquant: i32,
        acpred: bool,
        pure_i: bool,
        set: usize,
    ) -> Result<[i32; 64]> {
        let quant = mquant.abs().clamp(1, 31);
        let dc_diff = self.vc1_read_dc_diff(br, pic, blk >= 4, quant)?;
        let (pred, mut left, a_av, c_av, q2) = if pure_i && seq.profile != Profile::Advanced {
            // Simple/Main I pictures use vc1_i_pred_dc(), which supplies the
            // profile's outer-boundary DC values rather than treating the
            // neighbours as unavailable.
            let (p, l) = self.vc1_i_dc_pred(r, c, blk, seq, quant);
            (p, l, true, true, mquant)
        } else {
            // Advanced I/BI pictures use ff_vc1_pred_dc() exactly like intra
            // macroblocks in inter pictures. This is required for the real
            // neighbour availability rules and per-MB DQuant rescaling.
            self.vc1_inter_dc_pred(r, c, blk, mquant, pic.halfqp)
        };

        // ff_vc1_pred_dc() adjusts AC prediction direction when only one real
        // neighbour is available. Simple/Main pure-I uses vc1_i_pred_dc(), so
        // its edge handling stays in the legacy branch above.
        if !pure_i {
            if !a_av {
                left = true;
            }
            if !c_av {
                left = false;
            }
        }
        let use_pred = acpred
            && if pure_i && seq.profile != Profile::Advanced {
                true
            } else {
                a_av || c_av
            };

        let dc = pred + dc_diff;
        let mbi = self.vc1_mb_index(r, c).unwrap();
        self.vc1_dc[mbi][blk] = dc;
        self.vc1_intra_blocks[mbi][blk] = true;

        let mut coeff = [0i32; 64];
        coeff[0] = dc * vc1_dc_scale(quant);

        if coded {
            // FFmpeg uses two different scan rules here:
            // vc1_decode_i_block() (true I/BI pictures) selects zz_8x8[1],
            // [2], or [3] according to AC prediction, while
            // vc1_decode_intra_block() (intra MB inside P/B) uses zz_8x8[0]
            // for progressive Simple/Main profile regardless of ACPRED.
            let scan: &[usize; 64] = if pure_i {
                // vc1_decode_i_block[_adv] selects the AC-prediction scan from
                // the signalled ACPRED bit even when an Advanced edge block
                // has no real neighbour and therefore cannot apply the
                // predictor itself.
                if acpred {
                    if left {
                        &FF_WMV1_SCANTABLE[3]
                    } else {
                        &FF_WMV1_SCANTABLE[2]
                    }
                } else {
                    &FF_WMV1_SCANTABLE[1]
                }
            } else {
                &FF_WMV1_SCANTABLE[0]
            };

            let mut pos = 1usize;
            let mut last = false;
            while !last && pos < 64 {
                let (run, level, l) =
                    self.vc1_decode_ac_coeff(br, set, pic.pquant as i32, pic.dquant.enabled)?;
                let at = pos.saturating_add(run);
                if at > 63 {
                    break;
                }
                let dst = vc1_scan_index(scan, at, seq.uses_transposed_scans());
                coeff[dst] = level;
                pos = at + 1;
                last = l;
            }
        }

        if use_pred {
            if left {
                // FFmpeg: block[k << left_blk_sh] += ac_val[k].
                // left_blk_sh is 3 for the legacy simple IDCT and 0 for the
                // transposed RES_FASTTX path.
                let mut p = if seq.uses_transposed_scans() {
                    self.ac_pred.pred_row(r, c, blk)
                } else {
                    self.ac_pred.pred_left_col(r, c, blk)
                };
                if q2 != 0 && q2 != mquant {
                    for v in &mut p {
                        *v = vc1_rescale_ac_pred(*v, mquant, q2, pic.halfqp);
                    }
                }
                let sh = if seq.uses_transposed_scans() { 0 } else { 3 };
                for k in 1..8 {
                    coeff[k << sh] += p[k - 1];
                }
            } else {
                // FFmpeg: block[k << top_blk_sh] += ac_val[k + 8].
                // top_blk_sh is 0 for the legacy simple IDCT and 3 for the
                // transposed RES_FASTTX path.
                let mut p = if seq.uses_transposed_scans() {
                    self.ac_pred.pred_col(r, c, blk)
                } else {
                    self.ac_pred.pred_top_row(r, c, blk)
                };
                if q2 != 0 && q2 != mquant {
                    for v in &mut p {
                        *v = vc1_rescale_ac_pred(*v, mquant, q2, pic.halfqp);
                    }
                }
                let sh = if seq.uses_transposed_scans() { 3 } else { 0 };
                for k in 1..8 {
                    coeff[k << sh] += p[k - 1];
                }
            }
        }

        // Cache both physical coefficient axes before inverse quantization.
        // Which axis represents the left/top predictor is selected above from
        // RES_FASTTX, exactly like FFmpeg's left_blk_sh/top_blk_sh.
        self.ac_pred.store_row(
            r,
            c,
            blk,
            [
                coeff[1], coeff[2], coeff[3], coeff[4], coeff[5], coeff[6], coeff[7],
            ],
        );
        self.ac_pred.store_col(
            r,
            c,
            blk,
            [
                coeff[8], coeff[16], coeff[24], coeff[32], coeff[40], coeff[48], coeff[56],
            ],
        );

        for k in 1..64 {
            if coeff[k] != 0 {
                coeff[k] = vc1_scale_level(coeff[k], mquant, pic.halfqp, pic.pqual_mode != 0);
            }
        }
        Ok(coeff)
    }

    fn vc1_coded_block_pred(&mut self, r: usize, c: usize, blk: usize, diff: bool) -> bool {
        let bw = self.width_mb as usize * 2;
        let x = c * 2 + (blk & 1);
        let y = r * 2 + (blk >> 1);
        let idx = y * bw + x;
        let a = if x > 0 {
            self.vc1_coded_block[idx - 1]
        } else {
            0
        };
        let b = if x > 0 && y > 0 {
            self.vc1_coded_block[idx - bw - 1]
        } else {
            0
        };
        let cc = if y > 0 {
            self.vc1_coded_block[idx - bw]
        } else {
            0
        };
        let pred = if b == cc { a } else { cc };
        let v = pred ^ diff as u8;
        self.vc1_coded_block[idx] = v;
        v != 0
    }

    fn vc1_read_mvdata(
        &self,
        br: &mut BitReader<'_>,
        pic: &PictureHeader,
        quarter_sample: bool,
    ) -> Result<Vc1MvData> {
        let vlc_bit = br.bits_read();
        let sym = self.vc1_mvdata[(pic.mvtab as usize).min(3)]
            .decode(br)
            .ok_or_else(|| {
                DecoderError::InvalidData(format!(
                    "invalid WMV3 MVDATA VLC at bit {vlc_bit} (left={})",
                    br.bits_left()
                ))
            })?;
        let mut index = 1 + sym;
        let has_coeffs = index > 36;
        if has_coeffs {
            index -= 37;
        }
        if index == 0 {
            return Ok(Vc1MvData {
                dx: 0,
                dy: 0,
                intra: false,
                has_coeffs,
            });
        }

        let (_, _, _, _, kx, ky) = self.vc1_mv_params(pic);
        if index == 35 {
            let xb = (kx - 1 + quarter_sample as i32) as u8;
            let yb = (ky - 1 + quarter_sample as i32) as u8;
            let xbit = br.bits_read();
            let dx = br.read_bits(xb).ok_or_else(|| {
                DecoderError::InvalidData(format!(
                    "truncated WMV3 MVDATA x escape at bit {xbit}: need {xb}, left={}",
                    br.bits_left()
                ))
            })? as i32;
            let ybit = br.bits_read();
            let dy = br.read_bits(yb).ok_or_else(|| {
                DecoderError::InvalidData(format!(
                    "truncated WMV3 MVDATA y escape at bit {ybit}: need {yb}, left={}",
                    br.bits_left()
                ))
            })? as i32;
            return Ok(Vc1MvData {
                dx,
                dy,
                intra: false,
                has_coeffs,
            });
        }
        if index == 36 {
            return Ok(Vc1MvData {
                dx: 0,
                dy: 0,
                intra: true,
                has_coeffs,
            });
        }

        const SIZE: [i32; 6] = [0, 2, 3, 4, 5, 8];
        const OFF: [i32; 6] = [0, 1, 3, 7, 15, 31];
        let dec = |br: &mut BitReader<'_>, g: i32, axis: char| -> Result<i32> {
            let gi = g as usize;
            let mut out = OFF[gi];
            let n = SIZE[gi] - ((!quarter_sample && gi == 5) as i32);
            if n > 0 {
                let bit = br.bits_read();
                let v = br.read_bits(n as u8).ok_or_else(|| {
                    DecoderError::InvalidData(format!(
                        "truncated WMV3 MVDATA {axis} component at bit {bit}: index={index} group={gi} need={n}, left={}",
                        br.bits_left()
                    ))
                })? as i32;
                let sign = -(v & 1);
                out = (sign ^ ((v >> 1) + out)) - sign;
            }
            Ok(out)
        };
        Ok(Vc1MvData {
            dx: dec(br, index % 6, 'x')?,
            dy: dec(br, index / 6, 'y')?,
            intra: false,
            has_coeffs,
        })
    }

    #[inline]
    fn vc1_mv_params(&self, pic: &PictureHeader) -> (bool, bool, i32, i32, i32, i32) {
        let mode = if pic.mv_mode == MvMode::IntensityComp {
            pic.mv_mode2
        } else {
            pic.mv_mode
        };
        let quarter = !matches!(mode, MvMode::OneMvHpel | MvMode::OneMvHpelBilin);
        let mspel = mode != MvMode::OneMvHpelBilin;

        // FFmpeg ff_vc1_parse_frame_header():
        //   k_x = mvrange + 9 + (mvrange >> 1)
        //   k_y = mvrange + 8
        // Thus MVRANGE=0 is 9/8, not 10/9.  The old values make an escape
        // MVDATA component consume one bit too many and desynchronise the MB layer.
        let r = pic.mvrange as i32;
        let mut kx = r + 9 + (r >> 1);
        let mut ky = r + 8;
        if self.width < 120 {
            kx = 9;
        }
        if self.height < 60 {
            ky = 8;
        }
        (quarter, mspel, 1 << (kx - 1), 1 << (ky - 1), kx, ky)
    }

    fn vc1_predict_p_mv(
        &mut self,
        br: &mut BitReader<'_>,
        pic: &PictureHeader,
        seq: &SequenceHeader,
        r: usize,
        c: usize,
        n: usize,
        dmv: (i32, i32),
        mv1: bool,
        intra: bool,
    ) -> Result<(i32, i32)> {
        let (quarter, _, rx, ry, _, _) = self.vc1_mv_params(pic);
        let mut dx = dmv.0;
        let mut dy = dmv.1;
        if !quarter {
            dx *= 2;
            dy *= 2;
        }

        let idx = self.vc1_mb_index(r, c).unwrap();
        if intra {
            self.vc1_mv4[idx][n] = (0, 0);
            if mv1 {
                self.vc1_mv4[idx] = [(0, 0); 4];
                self.vc1_current_mvs[idx] = (0, 0);
            }
            return Ok((0, 0));
        }

        let gx = (c * 2 + (n & 1)) as isize;
        let gy = (r * 2 + (n >> 1)) as isize;

        // ff_vc1_pred_mv() addresses motion_val through FFmpeg's padded
        // b8 grid, whose stride is 2 * mb_width + 1.  Do not apply its raw
        // pointer offsets directly to our compact 2 * mb_width grid: the
        // first-column 4MV block-0 special case deliberately crosses the
        // padding cell into the previous b8 row.  Translate the FFmpeg
        // address back into a compact luma slot instead.
        let b8_stride = self.width_mb as isize * 2 + 1;
        let ff_xy = b8_stride * gy + gx - 2;
        let slot_from_ff_index = |ff_index: isize| -> Option<(usize, usize)> {
            let shifted = ff_index + 2;
            let yy = shifted.div_euclid(b8_stride);
            let xx = shifted.rem_euclid(b8_stride);
            if yy < 0 || xx >= self.width_mb as isize * 2 {
                None
            } else {
                self.vc1_luma_slot(xx, yy)
            }
        };

        // A = above, B = the FFmpeg `off` candidate, C = left.  Keep the
        // slots as well as the vectors because VC-1 hybrid prediction tests
        // the exact A/C block's intra state.
        let a_slot = slot_from_ff_index(ff_xy - b8_stride);
        let c_slot = slot_from_ff_index(ff_xy - 1);
        let boff: isize = if mv1 {
            if c + 1 >= self.width_mb as usize {
                -1
            } else {
                2
            }
        } else {
            match n {
                0 => {
                    if seq.res_rtm_flag {
                        if c > 0 { -1 } else { 1 }
                    } else if c > 0 {
                        -1
                    } else {
                        // Exact FFmpeg expression:
                        //   2 * mb_width - b8_stride - 1
                        2 * self.width_mb as isize - b8_stride - 1
                    }
                }
                1 => {
                    if c + 1 >= self.width_mb as usize {
                        -1
                    } else {
                        1
                    }
                }
                2 => 1,
                _ => -1,
            }
        };
        let b_slot = slot_from_ff_index(ff_xy - b8_stride + boff);

        let a = a_slot.map(|(mi, bi)| self.vc1_mv4[mi][bi]);
        let b = b_slot.map(|(mi, bi)| self.vc1_mv4[mi][bi]);
        let cc = c_slot.map(|(mi, bi)| self.vc1_mv4[mi][bi]);
        let vals = [
            a.unwrap_or((0, 0)),
            b.unwrap_or((0, 0)),
            cc.unwrap_or((0, 0)),
        ];
        let valid = (a.is_some() as usize) + (b.is_some() as usize) + (cc.is_some() as usize);
        let (mut px, mut py) = if valid > 1 {
            (
                median3(vals[0].0, vals[1].0, vals[2].0),
                median3(vals[0].1, vals[1].1, vals[2].1),
            )
        } else if let Some(v) = a {
            v
        } else if let Some(v) = cc {
            v
        } else {
            b.unwrap_or((0, 0))
        };

        // Pullback, SMPTE 421M 8.3.5.3.4.
        let lim = if mv1 { -60 } else { -28 };
        let qx = ((c as i32) << 6) + if !mv1 && matches!(n, 1 | 3) { 32 } else { 0 };
        let qy = ((r as i32) << 6) + if !mv1 && matches!(n, 2 | 3) { 32 } else { 0 };
        let xx = ((self.width_mb as i32) << 6) - 4;
        let yy = ((self.height_mb as i32) << 6) - 4;
        if qx + px < lim {
            px = lim - qx;
        }
        if qy + py < lim {
            py = lim - qy;
        }
        if qx + px > xx {
            px = xx - qx;
        }
        if qy + py > yy {
            py = yy - qy;
        }

        // Hybrid prediction, SMPTE 421M 8.3.5.3.5 / FFmpeg ff_vc1_pred_mv().
        // This is a bitstream operation, not merely a predictor choice: both A
        // and C tests can independently require consumption of HYBRIDPRED.
        // The previous port only implemented the A test. If A did not cross
        // the threshold but C did, one bit was left unread and every following
        // macroblock was shifted by one bit, eventually ending in truncated
        // MVDATA.
        if let (Some(av), Some(cv)) = (a, cc) {
            let a_intra = a_slot
                .map(|(mi, bi)| self.vc1_intra_blocks[mi][bi])
                .unwrap_or(false);
            let c_intra = c_slot
                .map(|(mi, bi)| self.vc1_intra_blocks[mi][bi])
                .unwrap_or(false);

            let mut sum = if a_intra {
                px.abs() + py.abs()
            } else {
                (px - av.0).abs() + (py - av.1).abs()
            };

            let mut need_hybrid_bit = sum > 32;
            if !need_hybrid_bit {
                sum = if c_intra {
                    px.abs() + py.abs()
                } else {
                    (px - cv.0).abs() + (py - cv.1).abs()
                };
                need_hybrid_bit = sum > 32;
            }

            if need_hybrid_bit {
                if br
                    .read_bit()
                    .ok_or_else(|| DecoderError::InvalidData("truncated WMV3 HYBRIDPRED".into()))?
                {
                    (px, py) = av;
                } else {
                    (px, py) = cv;
                }
            }
        }

        let mv = (
            ((px + dx + rx) & ((rx << 1) - 1)) - rx,
            ((py + dy + ry) & ((ry << 1) - 1)) - ry,
        );
        self.vc1_mv4[idx][n] = mv;
        if mv1 {
            self.vc1_mv4[idx] = [mv; 4];
            self.vc1_current_mvs[idx] = mv;
        } else if n == 0 {
            self.vc1_current_mvs[idx] = mv;
        }
        Ok(mv)
    }

    fn vc1_decode_p_residual(
        &mut self,
        br: &mut BitReader<'_>,
        pic: &PictureHeader,
        seq: &SequenceHeader,
        mquant: i32,
        ttmb: i32,
        first_block: bool,
    ) -> Result<([i32; 64], u8, u8)> {
        let tt_index = ((pic.pquant > 4) as usize) + ((pic.pquant > 12) as usize);
        let mut tt = if ttmb < 0 {
            let sym = self.vc1_ttblk[tt_index]
                .decode(br)
                .ok_or_else(|| DecoderError::InvalidData("invalid WMV3 TTBLK".into()))?
                as usize;
            TTBLK_TO_TT[tt_index][sym.min(7)]
        } else {
            (ttmb as u8) & 7
        };

        let mut sub = 0u8;
        if tt == TT_4X4 {
            let sym = self.vc1_subblkpat[tt_index]
                .decode(br)
                .ok_or_else(|| DecoderError::InvalidData("invalid WMV3 SUBBLKPAT".into()))?
                as u8;
            sub = (!(sym.wrapping_add(1))) & 0x0f;
        }
        if tt != TT_8X8
            && tt != TT_4X4
            && ((pic.ttmbf || (ttmb >= 0 && ((ttmb as u8) & 8) != 0 && !first_block))
                || (!seq.res_rtm_flag && !first_block))
        {
            sub = vc1_decode012_bits(br)?;
            if sub != 0 {
                sub ^= 3;
            }
            if matches!(tt, TT_8X4_TOP | TT_8X4_BOTTOM) {
                tt = TT_8X4;
            }
            if matches!(tt, TT_4X8_LEFT | TT_4X8_RIGHT) {
                tt = TT_4X8;
            }
        }
        if tt == TT_8X4_TOP || tt == TT_8X4_BOTTOM {
            sub = 2 - (tt == TT_8X4_TOP) as u8;
            tt = TT_8X4;
        }
        if tt == TT_4X8_LEFT || tt == TT_4X8_RIGHT {
            sub = 2 - (tt == TT_4X8_LEFT) as u8;
            tt = TT_4X8;
        }

        let (_, set) = Self::vc1_coding_sets(pic);
        let mut out = [0i32; 64];
        let mut decode_part = |this: &mut Self,
                               scan: &[usize],
                               base: usize,
                               limit: usize,
                               already_last: bool,
                               transpose_scan: bool|
         -> Result<()> {
            if already_last {
                return Ok(());
            }
            let mut pos = 0usize;
            let mut last = false;
            while !last && pos < limit {
                let (run, lev, l) =
                    this.vc1_decode_ac_coeff(br, set, pic.pquant as i32, pic.dquant.enabled)?;
                let at = pos.saturating_add(run);
                if at >= limit {
                    break;
                }
                let scan_index = if transpose_scan {
                    vc1_transpose_scan_index(scan[at])
                } else {
                    scan[at]
                };
                let idx = base + scan_index;
                if idx < 64 {
                    out[idx] = vc1_scale_level(lev, mquant, pic.halfqp, pic.pqual_mode != 0);
                }
                pos = at + 1;
                last = l;
            }
            Ok(())
        };

        let pat = match tt {
            TT_8X8 => {
                decode_part(
                    self,
                    &FF_WMV1_SCANTABLE[0],
                    0,
                    64,
                    false,
                    seq.uses_transposed_scans(),
                )?;
                0x0f
            }
            TT_4X4 => {
                for j in 0..4usize {
                    let base = (j & 1) * 4 + (j & 2) * 16;
                    decode_part(
                        self,
                        &VC1_ZZ_4X4,
                        base,
                        16,
                        (sub & (1 << (3 - j))) != 0,
                        false,
                    )?;
                }
                (!sub) & 0x0f
            }
            TT_8X4 => {
                let scan: &[usize] = if seq.profile == Profile::Advanced {
                    &VC1_ZZ_8X4
                } else {
                    &FF_WMV2_SCANTABLE_A
                };
                for j in 0..2usize {
                    decode_part(self, scan, j * 32, 32, (sub & (1 << (1 - j))) != 0, false)?;
                }
                (!((sub & 2) * 6 + (sub & 1) * 3)) & 0x0f
            }
            TT_4X8 => {
                let scan: &[usize] = if seq.profile == Profile::Advanced {
                    &VC1_ZZ_4X8
                } else {
                    &FF_WMV2_SCANTABLE_B
                };
                for j in 0..2usize {
                    decode_part(self, scan, j * 4, 32, (sub & (1 << (1 - j))) != 0, false)?;
                }
                (!(sub * 5)) & 0x0f
            }
            _ => 0,
        };

        apply_wmv3_idct(&mut out, tt, seq.uses_transposed_scans());
        Ok((out, tt, pat))
    }

    fn vc1_mc_single(
        &self,
        frame: &mut YuvFrame,
        r: usize,
        c: usize,
        reference: &YuvFrame,
        mv: (i32, i32),
        pic: &PictureHeader,
        seq: &SequenceHeader,
        block: Option<usize>,
        interpolated_next: bool,
    ) {
        let (quarter, mspel, _, _, _, _) = self.vc1_mv_params(pic);
        let mx = mv.0;
        let my = mv.1;
        if !quarter {
            // Motion vectors are already stored internally in qpel units by
            // vc1_predict_p_mv(). Nothing to rescale here.
        }

        let (bx, by, bw, bh) = if let Some(n) = block {
            (c * 16 + (n & 1) * 8, r * 16 + (n >> 1) * 8, 8usize, 8usize)
        } else {
            (c * 16, r * 16, 16usize, 16usize)
        };

        // The caller prepares picture-level intensity compensation once.
        // Motion compensation itself must never rebuild a full reference frame
        // per macroblock.
        let src_y = reference.y.as_slice();

        let sx = bx as i32 + (mx >> 2);
        let sy = by as i32 + (my >> 2);
        let fx = mx & 3;
        let fy = my & 3;
        // A macroblock prediction is at most 16x16. Avoid one heap
        // allocation for every 1MV/4MV luma prediction.
        let mut tmp_storage = [0u8; 16 * 16];
        let tmp = &mut tmp_storage[..bw * bh];
        if mspel {
            vc1_mspel_block(
                tmp,
                bw,
                src_y,
                self.width as usize,
                self.width as usize,
                self.height as usize,
                sx,
                sy,
                bw,
                bh,
                fx,
                fy,
                self.vc1_rnd,
            );
        } else {
            vc1_bilinear_qpel_block(
                tmp,
                bw,
                src_y,
                self.width as usize,
                self.width as usize,
                self.height as usize,
                sx,
                sy,
                bw,
                bh,
                fx & 2,
                fy & 2,
                4,
                self.vc1_rnd,
            );
        }
        for y in 0..bh {
            let dy = by + y;
            if dy >= self.height as usize {
                break;
            }
            for x in 0..bw {
                let dx = bx + x;
                if dx >= self.width as usize {
                    break;
                }
                frame.y[dy * self.width as usize + dx] = tmp[y * bw + x];
            }
        }

        if block.is_some() {
            return;
        }

        self.vc1_mc_chroma(frame, r, c, reference, mv, seq, interpolated_next);
    }

    fn vc1_mc_chroma(
        &self,
        frame: &mut YuvFrame,
        r: usize,
        c: usize,
        reference: &YuvFrame,
        mv: (i32, i32),
        seq: &SequenceHeader,
        interpolated_next: bool,
    ) {
        let mx = mv.0;
        let my = mv.1;
        let cw = self.width as usize / 2;
        let ch = self.height as usize / 2;
        if cw == 0 || ch == 0 {
            return;
        }
        let mut uvmx = (mx + ((mx & 3) == 3) as i32) >> 1;
        let mut uvmy = (my + ((my & 3) == 3) as i32) >> 1;
        if seq.fastuvmc {
            if interpolated_next {
                // ff_vc1_interp_mc(): the backward/next predictor of an
                // interpolated B macroblock uses the opposite FASTUVMC
                // adjustment from ff_vc1_mc_1mv().  Reusing the forward rule
                // shifts U/V by one chroma quarter-pel and appears as green or
                // magenta motion trails.
                uvmx += if uvmx < 0 { -(uvmx & 1) } else { uvmx & 1 };
                uvmy += if uvmy < 0 { -(uvmy & 1) } else { uvmy & 1 };
            } else {
                uvmx += if uvmx < 0 { uvmx & 1 } else { -(uvmx & 1) };
                uvmy += if uvmy < 0 { uvmy & 1 } else { -(uvmy & 1) };
            }
        }
        let csx = (c * 8) as i32 + (uvmx >> 2);
        let csy = (r * 8) as i32 + (uvmy >> 2);
        let cfx = (uvmx & 3) << 1;
        let cfy = (uvmy & 3) << 1;

        let cb = reference.cb.as_slice();
        let cr = reference.cr.as_slice();
        let mut tb = [0u8; 64];
        let mut tr = [0u8; 64];
        vc1_bilinear_qpel_block(
            &mut tb,
            8,
            cb,
            cw,
            cw,
            ch,
            csx,
            csy,
            8,
            8,
            cfx,
            cfy,
            8,
            self.vc1_rnd,
        );
        vc1_bilinear_qpel_block(
            &mut tr,
            8,
            cr,
            cw,
            cw,
            ch,
            csx,
            csy,
            8,
            8,
            cfx,
            cfy,
            8,
            self.vc1_rnd,
        );
        for y in 0..8 {
            for x in 0..8 {
                let dx = c * 8 + x;
                let dy = r * 8 + y;
                if dx < cw && dy < ch {
                    frame.cb[dy * cw + dx] = tb[y * 8 + x];
                    frame.cr[dy * cw + dx] = tr[y * 8 + x];
                }
            }
        }
    }

    fn vc1_mc_4mv_chroma(
        &self,
        frame: &mut YuvFrame,
        r: usize,
        c: usize,
        reference: &YuvFrame,
        mvs: [(i32, i32); 4],
        intra: [bool; 4],
        pic: &PictureHeader,
        seq: &SequenceHeader,
    ) -> (i32, i32) {
        // VC-1 derives a single chroma MV from the non-intra luma blocks.
        // This follows FFmpeg get_chroma_mv(): median of 4, median of the
        // three valid vectors, or the arithmetic mean of the two valid ones.
        let mut valid = [(0i32, 0i32); 4];
        let mut count = 0usize;
        for i in 0..4 {
            if !intra[i] {
                valid[count] = mvs[i];
                count += 1;
            }
        }
        if count < 2 {
            // get_chroma_mv() returns zero for 0/1 valid luma MVs; chroma is
            // not motion compensated for this macroblock.
            for i in 0..4 {
                if !intra[i] {
                    self.vc1_mc_single(frame, r, c, reference, mvs[i], pic, seq, Some(i), false);
                }
            }
            return (0, 0);
        }
        #[inline]
        fn median4(a: i32, b: i32, c: i32, d: i32) -> i32 {
            if a < b {
                if c < d {
                    (b.min(d) + a.max(c)) / 2
                } else {
                    (b.min(c) + a.max(d)) / 2
                }
            } else if c < d {
                (a.min(d) + b.max(c)) / 2
            } else {
                (a.min(c) + b.max(d)) / 2
            }
        }
        let mv = match count {
            2 => ((valid[0].0 + valid[1].0) / 2, (valid[0].1 + valid[1].1) / 2),
            3 => (
                median3(valid[0].0, valid[1].0, valid[2].0),
                median3(valid[0].1, valid[1].1, valid[2].1),
            ),
            _ => (
                median4(valid[0].0, valid[1].0, valid[2].0, valid[3].0),
                median4(valid[0].1, valid[1].1, valid[2].1, valid[3].1),
            ),
        };
        // 4MV derives one chroma MV, but it must not run a fake 16x16 luma
        // prediction just to obtain U/V. The old path did that and then
        // restored all four 8x8 luma blocks, nearly doubling MC work.
        self.vc1_mc_chroma(frame, r, c, reference, mv, seq, false);
        // Now generate the four independently compensated luma blocks once.
        for i in 0..4 {
            if !intra[i] {
                self.vc1_mc_single(frame, r, c, reference, mvs[i], pic, seq, Some(i), false);
            }
        }
        vc1_loop_chroma_mv(mv)
    }

    fn vc1_b_predict(
        &self,
        r: usize,
        c: usize,
        dmv: [(i32, i32); 2],
        direct: bool,
        mode: u8,
        intra: bool,
        pic: &PictureHeader,
        seq: &SequenceHeader,
        fwd_hist: &[(i32, i32)],
        bwd_hist: &[(i32, i32)],
    ) -> [(i32, i32); 2] {
        let (quarter, _, rx, ry, _, _) = self.vc1_mv_params(pic);
        let idx = r * self.width_mb as usize + c;
        if intra {
            return [(0, 0); 2];
        }

        // ff_vc1_pred_b_mv() always begins with the direct-mode vectors
        // derived from the future anchor.  A non-direct MB only replaces the
        // direction(s) selected by BMVTYPE; the inactive direction remains
        // this scaled anchor vector and is still stored in B-picture MV
        // history for later predictors.
        let base = self
            .vc1_bwd_anchor_mvs
            .as_ref()
            .and_then(|v| v.get(idx))
            .copied()
            .unwrap_or((0, 0));
        let mut out = [
            (
                vc1_scale_b_mv(base.0, pic.bfrac_num, pic.bfrac_den, false, quarter),
                vc1_scale_b_mv(base.1, pic.bfrac_num, pic.bfrac_den, false, quarter),
            ),
            (
                vc1_scale_b_mv(base.0, pic.bfrac_num, pic.bfrac_den, true, quarter),
                vc1_scale_b_mv(base.1, pic.bfrac_num, pic.bfrac_den, true, quarter),
            ),
        ];

        // Direct vectors use the progressive 1-MV qpel pullback (8.4.5.4):
        // [-60-qx, X-qx] / [-60-qy, Y-qy].  The previous port returned the
        // scaled vectors without this clipping.
        let qx6 = (c as i32) << 6;
        let qy6 = (r as i32) << 6;
        let x6 = ((self.width_mb as i32) << 6) - 4;
        let y6 = ((self.height_mb as i32) << 6) - 4;
        for mv in &mut out {
            mv.0 = mv.0.clamp(-60 - qx6, x6 - qx6);
            mv.1 = mv.1.clamp(-60 - qy6, y6 - qy6);
        }
        if direct {
            return out;
        }

        let pred = |hist: &[(i32, i32)]| -> (i32, i32) {
            let w = self.width_mb as usize;
            if r > 0 {
                let a = hist[idx - w];
                if w == 1 {
                    return a;
                }
                let b = if c + 1 < w {
                    hist[idx - w + 1]
                } else {
                    hist[idx - w - 1]
                };
                // ff_vc1_pred_b_mv() sets C to a real zero vector on the
                // first macroblock column; it does not mark C unavailable.
                // That zero must therefore participate in median(A,B,C).
                let cc = if c > 0 { hist[idx - 1] } else { (0, 0) };
                (median3(a.0, b.0, cc.0), median3(a.1, b.1, cc.1))
            } else if c > 0 {
                hist[idx - 1]
            } else {
                (0, 0)
            }
        };

        // For progressive Main Profile B pictures, only the direction(s)
        // selected by BMVTYPE are re-predicted.  This matches the two guarded
        // branches in ff_vc1_pred_b_mv().
        for dir in 0..2 {
            let active =
                (dir == 0 && (mode == 0 || mode == 2)) || (dir == 1 && (mode == 1 || mode == 2));
            if !active {
                continue;
            }
            let p = if dir == 0 {
                pred(fwd_hist)
            } else {
                pred(bwd_hist)
            };
            let (mut dx, mut dy) = dmv[dir];
            if !quarter {
                dx *= 2;
                dy *= 2;
            }

            // ff_vc1_pred_b_mv(): Advanced Profile uses the 64-unit
            // macroblock coordinate domain (sh=6, MV=-60); older profiles use
            // sh=5 / MV=-28 for non-direct B prediction.
            let sh = if seq.profile == Profile::Advanced {
                6
            } else {
                5
            };
            let lim = 4 - (1 << sh);
            let qx = (c as i32) << sh;
            let qy = (r as i32) << sh;
            let xx = ((self.width_mb as i32) << sh) - 4;
            let yy = ((self.height_mb as i32) << sh) - 4;
            let mut px = p.0;
            let mut py = p.1;
            if qx + px < lim {
                px = lim - qx;
            }
            if qy + py < lim {
                py = lim - qy;
            }
            if qx + px > xx {
                px = xx - qx;
            }
            if qy + py > yy {
                py = yy - qy;
            }
            out[dir] = (
                ((px + dx + rx) & ((rx << 1) - 1)) - rx,
                ((py + dy + ry) & ((ry << 1) - 1)) - ry,
            );
        }
        out
    }

    // ─── Intra frame ─────────────────────────────────────────────────────────

    fn decode_intra(
        &mut self,
        payload: &[u8],
        pic: &PictureHeader,
        seq: &SequenceHeader,
        _frame: &mut YuvFrame,
    ) -> Result<()> {
        let mut br = BitReader::new_at(payload, pic.header_bits);
        self.vc1_reset_picture_state();
        let (intra_set, chroma_set) = Self::vc1_coding_sets(pic);
        let mb_w = self.width_mb as usize;
        let mb_h = self.height_mb as usize;
        let advanced = seq.profile == Profile::Advanced;

        if advanced && !pic.overflags_raw {
            if let Some(ref flags) = pic.overflags_plane {
                let n = self.vc1_overflags.len().min(flags.len());
                self.vc1_overflags[..n].copy_from_slice(&flags[..n]);
            }
        }

        for r in 0..mb_h {
            for c in 0..mb_w {
                let cbp = self
                    .wmv2_mb_i_vlc
                    .decode(&mut br)
                    .ok_or_else(|| DecoderError::InvalidData("invalid WMV3 I CBPCY".into()))?
                    as u8;
                let mbi = r * mb_w + c;

                let acpred = if advanced {
                    if pic.acpred_raw {
                        br.read_bit().ok_or_else(|| {
                            DecoderError::InvalidData(
                                "truncated VC-1 Advanced raw ACPRED bit".into(),
                            )
                        })?
                    } else {
                        pic.acpred_plane
                            .as_ref()
                            .and_then(|plane| plane.get(mbi))
                            .copied()
                            .unwrap_or(0)
                            != 0
                    }
                } else {
                    br.read_bit()
                        .ok_or_else(|| DecoderError::InvalidData("truncated WMV3 ACPRED".into()))?
                };

                if advanced && pic.condover == 2 && pic.overflags_raw {
                    self.vc1_overflags[mbi] = br.read_bit().ok_or_else(|| {
                        DecoderError::InvalidData(
                            "truncated VC-1 Advanced raw OVERFLAGS bit".into(),
                        )
                    })? as u8;
                }

                // Advanced I/BI pictures use the same GET_MQUANT() macroblock
                // syntax as FFmpeg's vc1_decode_i_blocks_adv(). Simple/Main I
                // pictures keep the picture quantizer throughout the frame.
                let mquant = if advanced {
                    read_mquant(
                        &mut br,
                        &pic.dquant,
                        pic.pquant as i32,
                        c as u32,
                        r as u32,
                        self.width_mb,
                        self.height_mb,
                    )?
                } else {
                    pic.pquant as i32
                };
                self.vc1_qscale[mbi] = mquant;

                for blk in 0..6usize {
                    let mut coded = ((cbp >> (5 - blk)) & 1) != 0;
                    if blk < 4 {
                        coded = self.vc1_coded_block_pred(r, c, blk, coded);
                    }
                    let mut coeff = self.vc1_decode_intra_coeffs(
                        &mut br,
                        pic,
                        seq,
                        r,
                        c,
                        blk,
                        coded,
                        mquant,
                        acpred,
                        true,
                        if blk < 4 { intra_set } else { chroma_set },
                    )?;
                    apply_wmv3_idct(&mut coeff, TT_8X8, seq.uses_transposed_scans());
                    let ri = vc1_recon_index(mb_w, r, c, blk);
                    self.vc1_recon_blocks[ri] = coeff;
                    self.vc1_intra_blocks[mbi][blk] = true;
                    self.vc1_block_cbp[mbi][blk] = 0x0f;
                }
            }
        }
        Ok(())
    }

    fn decode_p(
        &mut self,
        payload: &[u8],
        pic: &PictureHeader,
        seq: &SequenceHeader,
        frame: &mut YuvFrame,
    ) -> Result<()> {
        let mut reference = match self.ref_frame.clone() {
            Some(v) => v,
            None => return Ok(()),
        };

        // FFmpeg vc1_mc.c applies range reduction to the reference samples
        // when the *current* picture has RANGEREDFRM set.  It never mutates
        // the stored reference frame.  The operation precedes intensity
        // compensation.
        if pic.rangeredfrm {
            vc1_range_reduce_reference(&mut reference);
        }

        // Intensity compensation is picture-level state.  Build the transformed
        // reference once for this P picture and reuse it for every macroblock.
        if pic.mv_mode == MvMode::IntensityComp {
            vc1_apply_ic_reference(&mut reference, pic.lumscale, pic.lumshift);
        }

        let mut br = BitReader::new_at(payload, pic.header_bits);
        self.vc1_reset_picture_state();
        let (intra_set, _) = Self::vc1_coding_sets(pic);
        let (quarter, _, _, _, _, _) = self.vc1_mv_params(pic);
        let mixed = pic.mv_mode == MvMode::MixedMv
            || (pic.mv_mode == MvMode::IntensityComp && pic.mv_mode2 == MvMode::MixedMv);
        let skip_plane = pic.skipmb_plane.as_ref();
        let mvtype_plane = pic.mvtypemb_plane.as_ref();
        let mb_w = self.width_mb as usize;

        for r in 0..self.height_mb as usize {
            for c in 0..mb_w {
                let mbi = r * mb_w + c;
                let fourmv = if mixed {
                    if pic.mvtypemb_raw {
                        br.read_bit().ok_or_else(|| {
                            DecoderError::InvalidData("truncated WMV3 MVTYPE".into())
                        })?
                    } else {
                        mvtype_plane.and_then(|p| p.get(mbi)).copied().unwrap_or(0) != 0
                    }
                } else {
                    false
                };
                let skipped = if pic.skipmb_raw {
                    br.read_bit()
                        .ok_or_else(|| DecoderError::InvalidData("truncated WMV3 SKIPMB".into()))?
                } else {
                    skip_plane.and_then(|p| p.get(mbi)).copied().unwrap_or(0) != 0
                };

                if !fourmv {
                    if skipped {
                        let mv =
                            self.vc1_predict_p_mv(&mut br, pic, seq, r, c, 0, (0, 0), true, false)?;
                        self.vc1_chroma_mvs[mbi] = vc1_loop_chroma_mv(mv);
                        self.vc1_mc_single(frame, r, c, &reference, mv, pic, seq, None, false);
                        continue;
                    }

                    let md = self.vc1_read_mvdata(&mut br, pic, quarter)?;
                    let mv = self.vc1_predict_p_mv(
                        &mut br,
                        pic,
                        seq,
                        r,
                        c,
                        0,
                        (md.dx, md.dy),
                        true,
                        md.intra,
                    )?;
                    let mut mquant = pic.pquant as i32;
                    let mut cbp = 0u8;
                    let mut acpred = false;

                    if md.intra && !md.has_coeffs {
                        mquant = read_mquant(
                            &mut br,
                            &pic.dquant,
                            pic.pquant as i32,
                            c as u32,
                            r as u32,
                            self.width_mb,
                            self.height_mb,
                        )?;
                        acpred = br.read_bit().ok_or_else(|| {
                            DecoderError::InvalidData("truncated WMV3 ACPRED".into())
                        })?;
                    } else if md.has_coeffs {
                        if md.intra {
                            acpred = br.read_bit().ok_or_else(|| {
                                DecoderError::InvalidData("truncated WMV3 ACPRED".into())
                            })?;
                        }
                        cbp = self.vc1_cbpcy[(pic.cbptab as usize).min(3)]
                            .decode(&mut br)
                            .ok_or_else(|| {
                                DecoderError::InvalidData("invalid WMV3 P CBPCY".into())
                            })? as u8;
                        mquant = read_mquant(
                            &mut br,
                            &pic.dquant,
                            pic.pquant as i32,
                            c as u32,
                            r as u32,
                            self.width_mb,
                            self.height_mb,
                        )?;
                    }

                    self.vc1_qscale[mbi] = mquant;
                    if md.intra {
                        self.vc1_intra_blocks[mbi] = [true; 6];
                        self.vc1_chroma_mvs[mbi] = (0, 0);
                    } else {
                        self.vc1_chroma_mvs[mbi] = vc1_loop_chroma_mv(mv);
                        self.vc1_mc_single(frame, r, c, &reference, mv, pic, seq, None, false);
                    }

                    let mut ttmb = pic.ttfrm as i32;
                    if !pic.ttmbf && !md.intra && md.has_coeffs {
                        ttmb = self.vc1_ttmb
                            [((pic.pquant > 4) as usize) + ((pic.pquant > 12) as usize)]
                            .decode(&mut br)
                            .ok_or_else(|| DecoderError::InvalidData("invalid WMV3 TTMB".into()))?;
                    }

                    let mut first = true;
                    for blk in 0..6usize {
                        let coded = ((cbp >> (5 - blk)) & 1) != 0;
                        if md.intra {
                            let mut co = self.vc1_decode_intra_coeffs(
                                &mut br,
                                pic,
                                seq,
                                r,
                                c,
                                blk,
                                coded,
                                mquant,
                                acpred,
                                false,
                                if blk < 4 {
                                    intra_set
                                } else {
                                    Self::vc1_coding_sets(pic).1
                                },
                            )?;
                            apply_wmv3_idct(&mut co, TT_8X8, seq.uses_transposed_scans());
                            if pic.rangeredfrm {
                                for v in &mut co {
                                    *v *= 2;
                                }
                            }
                            self.vc1_recon_blocks[vc1_recon_index(mb_w, r, c, blk)] = co;
                            self.vc1_block_cbp[mbi][blk] = 0x0f;
                        } else if coded {
                            let (co, tt, pat) =
                                self.vc1_decode_p_residual(&mut br, pic, seq, mquant, ttmb, first)?;
                            add_residual_block(frame, r as u32, c as u32, blk, &co);
                            self.vc1_block_tt[mbi][blk] = tt;
                            self.vc1_block_cbp[mbi][blk] = pat;
                            if !pic.ttmbf && ttmb < 8 {
                                ttmb = -1;
                            }
                            first = false;
                        }
                    }
                } else {
                    if skipped {
                        let mut mvs = [(0, 0); 4];
                        for (i, mv) in mvs.iter_mut().enumerate() {
                            *mv = self.vc1_predict_p_mv(
                                &mut br,
                                pic,
                                seq,
                                r,
                                c,
                                i,
                                (0, 0),
                                false,
                                false,
                            )?;
                            // FFmpeg vc1_decode_p_mb(), skipped 4MV path:
                            // every luma 8x8 block is motion-compensated before
                            // deriving the single chroma MV.  The previous Rust
                            // port only predicted/stored the four MVs and ran
                            // chroma MC, leaving Y as the freshly allocated
                            // frame contents.
                            self.vc1_mc_single(
                                frame,
                                r,
                                c,
                                &reference,
                                *mv,
                                pic,
                                seq,
                                Some(i),
                                false,
                            );
                        }
                        self.vc1_current_mvs[mbi] = mvs[0];
                        self.vc1_chroma_mvs[mbi] = self
                            .vc1_mc_4mv_chroma(frame, r, c, &reference, mvs, [false; 4], pic, seq);
                        continue;
                    }

                    let cbp0 = self.vc1_cbpcy[(pic.cbptab as usize).min(3)]
                        .decode(&mut br)
                        .ok_or_else(|| DecoderError::InvalidData("invalid WMV3 4MV CBPCY".into()))?
                        as u8;
                    let mut intra = [false; 4];
                    let mut coded = [false; 6];
                    let mut mvs = [(0, 0); 4];
                    for i in 0..4usize {
                        let present = ((cbp0 >> (5 - i)) & 1) != 0;
                        let md = if present {
                            self.vc1_read_mvdata(&mut br, pic, quarter)?
                        } else {
                            Vc1MvData::default()
                        };
                        intra[i] = md.intra;
                        coded[i] = if present { md.has_coeffs } else { false };
                        mvs[i] = self.vc1_predict_p_mv(
                            &mut br,
                            pic,
                            seq,
                            r,
                            c,
                            i,
                            (md.dx, md.dy),
                            false,
                            md.intra,
                        )?;
                        // ff_vc1_pred_mv() is immediately followed by
                        // ff_vc1_mc_4mv_luma() for every non-intra luma block.
                        // Chroma MC below is not a substitute for this: it only
                        // fills Cb/Cr.  Missing this call leaves 4MV P-picture
                        // luma blocks without their reference prediction, so
                        // residuals alone appear as block-shaped ghosts/trails.
                        if !md.intra {
                            self.vc1_mc_single(
                                frame,
                                r,
                                c,
                                &reference,
                                mvs[i],
                                pic,
                                seq,
                                Some(i),
                                false,
                            );
                        }
                    }
                    coded[4] = (cbp0 & 2) != 0;
                    coded[5] = (cbp0 & 1) != 0;

                    let ni = intra.iter().filter(|x| **x).count();
                    let chroma_intra = ni >= 3;
                    self.vc1_intra_blocks[mbi] = [
                        intra[0],
                        intra[1],
                        intra[2],
                        intra[3],
                        chroma_intra,
                        chroma_intra,
                    ];
                    let coded_inter = (0..4).any(|i| coded[i] && !intra[i])
                        || (!chroma_intra && (coded[4] || coded[5]));
                    let mut mquant = pic.pquant as i32;
                    if ni > 0 || coded_inter {
                        mquant = read_mquant(
                            &mut br,
                            &pic.dquant,
                            pic.pquant as i32,
                            c as u32,
                            r as u32,
                            self.width_mb,
                            self.height_mb,
                        )?;
                    }
                    self.vc1_qscale[mbi] = mquant;

                    let need_acpred =
                        self.vc1_intra_blocks[mbi].iter().enumerate().any(|(i, v)| {
                            *v && (self
                                .vc1_intra_neighbour(r, c, i, true)
                                .map(|(mi, b)| self.vc1_intra_blocks[mi][b])
                                .unwrap_or(false)
                                || self
                                    .vc1_intra_neighbour(r, c, i, false)
                                    .map(|(mi, b)| self.vc1_intra_blocks[mi][b])
                                    .unwrap_or(false))
                        });
                    let acpred = if need_acpred {
                        br.read_bit().ok_or_else(|| {
                            DecoderError::InvalidData("truncated WMV3 4MV ACPRED".into())
                        })?
                    } else {
                        false
                    };

                    let mut ttmb = pic.ttfrm as i32;
                    if !pic.ttmbf && coded_inter {
                        ttmb = self.vc1_ttmb
                            [((pic.pquant > 4) as usize) + ((pic.pquant > 12) as usize)]
                            .decode(&mut br)
                            .ok_or_else(|| {
                                DecoderError::InvalidData("invalid WMV3 4MV TTMB".into())
                            })?;
                    }

                    self.vc1_chroma_mvs[mbi] =
                        self.vc1_mc_4mv_chroma(frame, r, c, &reference, mvs, intra, pic, seq);

                    let mut first = true;
                    for blk in 0..6usize {
                        let is_intra = if blk < 4 { intra[blk] } else { chroma_intra };
                        if is_intra {
                            let mut co = self.vc1_decode_intra_coeffs(
                                &mut br,
                                pic,
                                seq,
                                r,
                                c,
                                blk,
                                coded[blk],
                                mquant,
                                acpred,
                                false,
                                if blk < 4 {
                                    intra_set
                                } else {
                                    Self::vc1_coding_sets(pic).1
                                },
                            )?;
                            apply_wmv3_idct(&mut co, TT_8X8, seq.uses_transposed_scans());
                            if pic.rangeredfrm {
                                for v in &mut co {
                                    *v *= 2;
                                }
                            }
                            self.vc1_recon_blocks[vc1_recon_index(mb_w, r, c, blk)] = co;
                            self.vc1_block_cbp[mbi][blk] = 0x0f;
                        } else if coded[blk] {
                            let (co, tt, pat) =
                                self.vc1_decode_p_residual(&mut br, pic, seq, mquant, ttmb, first)?;
                            add_residual_block(frame, r as u32, c as u32, blk, &co);
                            self.vc1_block_tt[mbi][blk] = tt;
                            self.vc1_block_cbp[mbi][blk] = pat;
                            if !pic.ttmbf && ttmb < 8 {
                                ttmb = -1;
                            }
                            first = false;
                        }
                    }
                    self.vc1_current_mvs[mbi] = mvs[0];
                }
            }
        }
        Ok(())
    }

    fn decode_b(
        &mut self,
        payload: &[u8],
        pic: &PictureHeader,
        seq: &SequenceHeader,
        frame: &mut YuvFrame,
    ) -> Result<()> {
        let mut fwd = match self.fwd_ref.clone() {
            Some(v) => v,
            None => return Ok(()),
        };
        let mut bwd = match self.bwd_ref.clone() {
            Some(v) => v,
            None => return Ok(()),
        };
        // FFmpeg vc1_mc.c range-reduces the reference samples used by the
        // current picture before interpolation.  Work on local reference
        // copies so the stored anchors remain untouched.
        if pic.rangeredfrm {
            vc1_range_reduce_reference(&mut fwd);
            vc1_range_reduce_reference(&mut bwd);
        }
        // The future P anchor may have declared intensity compensation for the
        // older anchor. FFmpeg keeps that transform in `last_luty` and applies
        // it whenever a B picture samples the forward/last reference.  The
        // previous port applied IC while decoding the P anchor itself but threw
        // this state away before the intervening B pictures, producing chroma
        // trails during fades.
        if let Some((lumscale, lumshift)) = self.vc1_b_fwd_ic {
            vc1_apply_ic_reference(&mut fwd, lumscale, lumshift);
        }
        let mut br = BitReader::new_at(payload, pic.header_bits);
        self.vc1_reset_picture_state();
        let (intra_set, _) = Self::vc1_coding_sets(pic);
        let (quarter, _, _, _, _, _) = self.vc1_mv_params(pic);
        let nmb = self.width_mb as usize * self.height_mb as usize;
        let mut fhist = vec![(0, 0); nmb];
        let mut bhist = vec![(0, 0); nmb];
        for r in 0..self.height_mb as usize {
            for c in 0..self.width_mb as usize {
                let idx = r * self.width_mb as usize + c;
                let direct = if pic.directmb_raw {
                    br.read_bit().ok_or_else(|| {
                        DecoderError::InvalidData("truncated WMV3 DIRECTMB".into())
                    })?
                } else {
                    pic.directmb_plane
                        .as_ref()
                        .and_then(|p| p.get(idx))
                        .copied()
                        .unwrap_or(0)
                        != 0
                };
                let skipped = if pic.skipmb_raw {
                    br.read_bit().ok_or_else(|| {
                        DecoderError::InvalidData("truncated WMV3 B SKIPMB".into())
                    })?
                } else {
                    pic.skipmb_plane
                        .as_ref()
                        .and_then(|p| p.get(idx))
                        .copied()
                        .unwrap_or(0)
                        != 0
                };
                let mut dmv = [(0, 0); 2];
                let mut md = Vc1MvData::default();
                if !direct && !skipped {
                    md = self.vc1_read_mvdata(&mut br, pic, quarter)?;
                    dmv[0] = (md.dx, md.dy);
                    dmv[1] = dmv[0];
                }
                let mut mode = 1u8;
                if !direct && (skipped || !md.intra) {
                    let t = vc1_decode012_bits(&mut br)?;
                    mode = match t {
                        0 => {
                            if pic.bfrac_num * 2 >= pic.bfrac_den {
                                1
                            } else {
                                0
                            }
                        }
                        1 => {
                            if pic.bfrac_num * 2 >= pic.bfrac_den {
                                0
                            } else {
                                1
                            }
                        }
                        _ => {
                            dmv[0] = (0, 0);
                            2
                        }
                    };
                }
                if skipped {
                    if direct {
                        mode = 2;
                    }
                    let mv = self
                        .vc1_b_predict(r, c, dmv, direct, mode, false, pic, seq, &fhist, &bhist);
                    fhist[idx] = mv[0];
                    bhist[idx] = mv[1];
                    if direct || mode == 2 {
                        self.vc1_mc_blend(frame, r, c, &fwd, &bwd, mv[0], mv[1], pic, seq);
                    } else if mode == 0 {
                        self.vc1_mc_single(frame, r, c, &fwd, mv[0], pic, seq, None, false);
                    } else {
                        self.vc1_mc_single(frame, r, c, &bwd, mv[1], pic, seq, None, false);
                    }
                    continue;
                }
                let mut cbp = 0u8;
                let mut mquant = pic.pquant as i32;
                let mut acpred = false;
                let mut ttmb = pic.ttfrm as i32;
                let mv;
                if direct {
                    cbp = self.vc1_cbpcy[(pic.cbptab as usize).min(3)]
                        .decode(&mut br)
                        .ok_or_else(|| {
                            DecoderError::InvalidData("invalid WMV3 direct CBPCY".into())
                        })? as u8;
                    mquant = read_mquant(
                        &mut br,
                        &pic.dquant,
                        pic.pquant as i32,
                        c as u32,
                        r as u32,
                        self.width_mb,
                        self.height_mb,
                    )?;
                    if !pic.ttmbf {
                        ttmb = self.vc1_ttmb
                            [((pic.pquant > 4) as usize) + ((pic.pquant > 12) as usize)]
                            .decode(&mut br)
                            .ok_or_else(|| {
                                DecoderError::InvalidData("invalid WMV3 B TTMB".into())
                            })?;
                    }
                    dmv = [(0, 0); 2];
                    mv = self.vc1_b_predict(r, c, dmv, true, 2, false, pic, seq, &fhist, &bhist);
                    self.vc1_mc_blend(frame, r, c, &fwd, &bwd, mv[0], mv[1], pic, seq);
                } else if !md.has_coeffs && !md.intra {
                    mv = self
                        .vc1_b_predict(r, c, dmv, false, mode, md.intra, pic, seq, &fhist, &bhist);
                    if mode == 2 {
                        self.vc1_mc_blend(frame, r, c, &fwd, &bwd, mv[0], mv[1], pic, seq);
                    } else if mode == 0 {
                        self.vc1_mc_single(frame, r, c, &fwd, mv[0], pic, seq, None, false);
                    } else {
                        self.vc1_mc_single(frame, r, c, &bwd, mv[1], pic, seq, None, false);
                    }
                    fhist[idx] = mv[0];
                    bhist[idx] = mv[1];
                    continue;
                } else if md.intra && !md.has_coeffs {
                    mquant = read_mquant(
                        &mut br,
                        &pic.dquant,
                        pic.pquant as i32,
                        c as u32,
                        r as u32,
                        self.width_mb,
                        self.height_mb,
                    )?;
                    acpred = br.read_bit().ok_or_else(|| {
                        DecoderError::InvalidData("truncated WMV3 B ACPRED".into())
                    })?;
                    mv = self
                        .vc1_b_predict(r, c, dmv, false, mode, md.intra, pic, seq, &fhist, &bhist);
                } else {
                    if mode == 2 {
                        let md2 = self.vc1_read_mvdata(&mut br, pic, quarter)?;
                        dmv[0] = (md2.dx, md2.dy);
                        if !md2.has_coeffs {
                            let mv2 = self.vc1_b_predict(
                                r, c, dmv, false, mode, md2.intra, pic, seq, &fhist, &bhist,
                            );
                            self.vc1_mc_blend(frame, r, c, &fwd, &bwd, mv2[0], mv2[1], pic, seq);
                            fhist[idx] = mv2[0];
                            bhist[idx] = mv2[1];
                            continue;
                        }
                        md.has_coeffs = md2.has_coeffs;
                        md.intra = md2.intra;
                    }
                    mv = self
                        .vc1_b_predict(r, c, dmv, false, mode, md.intra, pic, seq, &fhist, &bhist);
                    if !md.intra {
                        if mode == 2 {
                            self.vc1_mc_blend(frame, r, c, &fwd, &bwd, mv[0], mv[1], pic, seq);
                        } else if mode == 0 {
                            self.vc1_mc_single(frame, r, c, &fwd, mv[0], pic, seq, None, false);
                        } else {
                            self.vc1_mc_single(frame, r, c, &bwd, mv[1], pic, seq, None, false);
                        }
                    }
                    if md.intra {
                        acpred = br.read_bit().ok_or_else(|| {
                            DecoderError::InvalidData("truncated WMV3 B ACPRED".into())
                        })?;
                    }
                    cbp = self.vc1_cbpcy[(pic.cbptab as usize).min(3)]
                        .decode(&mut br)
                        .ok_or_else(|| DecoderError::InvalidData("invalid WMV3 B CBPCY".into()))?
                        as u8;
                    mquant = read_mquant(
                        &mut br,
                        &pic.dquant,
                        pic.pquant as i32,
                        c as u32,
                        r as u32,
                        self.width_mb,
                        self.height_mb,
                    )?;
                    if !pic.ttmbf && !md.intra && md.has_coeffs {
                        ttmb = self.vc1_ttmb
                            [((pic.pquant > 4) as usize) + ((pic.pquant > 12) as usize)]
                            .decode(&mut br)
                            .ok_or_else(|| {
                                DecoderError::InvalidData("invalid WMV3 B TTMB".into())
                            })?;
                    }
                }
                self.vc1_qscale[idx] = mquant;
                fhist[idx] = mv[0];
                bhist[idx] = mv[1];
                let mut first = true;
                for blk in 0..6 {
                    let coded = ((cbp >> (5 - blk)) & 1) != 0;
                    if md.intra {
                        self.vc1_intra_blocks[idx][blk] = true;
                        let mut co = self.vc1_decode_intra_coeffs(
                            &mut br,
                            pic,
                            seq,
                            r,
                            c,
                            blk,
                            coded,
                            mquant,
                            acpred,
                            false,
                            if blk < 4 {
                                intra_set
                            } else {
                                Self::vc1_coding_sets(pic).1
                            },
                        )?;
                        apply_wmv3_idct(&mut co, TT_8X8, seq.uses_transposed_scans());
                        if pic.rangeredfrm {
                            for v in &mut co {
                                *v *= 2;
                            }
                        }
                        write_intra_block(frame, r as u32, c as u32, blk, &co);
                    } else if coded {
                        let (co, _, _) =
                            self.vc1_decode_p_residual(&mut br, pic, seq, mquant, ttmb, first)?;
                        add_residual_block(frame, r as u32, c as u32, blk, &co);
                        if !pic.ttmbf && ttmb < 8 {
                            ttmb = -1;
                        }
                        first = false;
                    }
                }
            }
        }
        Ok(())
    }

    fn vc1_mc_blend(
        &self,
        frame: &mut YuvFrame,
        r: usize,
        c: usize,
        fwd: &YuvFrame,
        bwd: &YuvFrame,
        fmv: (i32, i32),
        bmv: (i32, i32),
        pic: &PictureHeader,
        seq: &SequenceHeader,
    ) {
        // Produce the forward prediction directly into the destination and keep
        // only this macroblock, then overwrite with the backward prediction and
        // average.  The old code allocated two full-resolution YUV frames for
        // every B-picture macroblock.
        self.vc1_mc_single(frame, r, c, fwd, fmv, pic, seq, None, false);

        let x0 = c * 16;
        let y0 = r * 16;
        let mut fy = [0u8; 16 * 16];
        for y in 0..16 {
            let py = y0 + y;
            if py >= self.height as usize {
                break;
            }
            for x in 0..16 {
                let px = x0 + x;
                if px >= self.width as usize {
                    break;
                }
                fy[y * 16 + x] = frame.y[py * self.width as usize + px];
            }
        }

        let cw = self.width as usize / 2;
        let ch = self.height as usize / 2;
        let cx0 = c * 8;
        let cy0 = r * 8;
        let mut fu = [0u8; 8 * 8];
        let mut fv = [0u8; 8 * 8];
        for y in 0..8 {
            let py = cy0 + y;
            if py >= ch {
                break;
            }
            for x in 0..8 {
                let px = cx0 + x;
                if px >= cw {
                    break;
                }
                let i = py * cw + px;
                fu[y * 8 + x] = frame.cb[i];
                fv[y * 8 + x] = frame.cr[i];
            }
        }

        self.vc1_mc_single(frame, r, c, bwd, bmv, pic, seq, None, true);

        for y in 0..16 {
            let py = y0 + y;
            if py >= self.height as usize {
                break;
            }
            for x in 0..16 {
                let px = x0 + x;
                if px >= self.width as usize {
                    break;
                }
                let i = py * self.width as usize + px;
                let sum = fy[y * 16 + x] as u16 + frame.y[i] as u16;
                // FFmpeg's avg VC-1 MC operator is always
                // (dst + clip(pred) + 1) >> 1.  `v->rnd` controls the
                // interpolation filter that produced `pred`; it does not
                // change the final forward/backward average.
                frame.y[i] = ((sum + 1) >> 1) as u8;
            }
        }
        for y in 0..8 {
            let py = cy0 + y;
            if py >= ch {
                break;
            }
            for x in 0..8 {
                let px = cx0 + x;
                if px >= cw {
                    break;
                }
                let i = py * cw + px;
                let us = fu[y * 8 + x] as u16 + frame.cb[i] as u16;
                let vs = fv[y * 8 + x] as u16 + frame.cr[i] as u16;
                frame.cb[i] = ((us + 1) >> 1) as u8;
                frame.cr[i] = ((vs + 1) >> 1) as u8;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// WMV2 (MS-MPEG4 V8) Decode Entry Points
// ═══════════════════════════════════════════════════════════════════════════════
//
// Public interface: MacroblockDecoder::decode_wmv2_frame()
//
// WMV2 simplifications vs VC-1:
//   • No B-frames, no BFRACTION, no overlap filter, no loop filter flag
//   • No TRANSACFRM/CBPTAB/MVTAB in seqhdr; ttcoef from frame header
//   • DC: 8-bit absolute (no VLC), sign separate
//   • AC escape: Mode-3 only (1-bit last, 6-bit run, 8-bit level, 1-bit sign)
//   • IDCT: same VC-1 integer transform reused
//   • Motion: half-pel bilinear (same MC as VC-1)

// ─── WMV2 DC scale tables ───────────────────────────────────────────────────
// WMV2 uses MPEG-4 style DC scaling tables (much smaller than VC-1's ×128 domain
// tables). Using VC-1 DC step tables here will massively over-scale DC and
// saturate the reconstructed picture.
//
// These tables match the conventional MPEG-4 Part 2 DC scale tables.
// (They are also used by MSMPEG4/WMV1-family decoders.)
#[inline(always)]
fn wmv2_dc_scale(pquant: i32, is_luma: bool) -> i32 {
    // upstream: ff_wmv1_y_dc_scale_table / ff_wmv1_c_dc_scale_table (used for WMV1/WMV2).
    const Y: [i32; 32] = [
        0, 8, 8, 8, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17, 18,
        18, 19, 19, 20, 20, 21, 21,
    ];
    const C: [i32; 32] = [
        0, 8, 8, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17, 18,
        18, 19, 19, 20, 20, 21, 21, 22,
    ];
    let idx = pquant.clamp(1, 31) as usize;
    if is_luma { Y[idx] } else { C[idx] }
}

#[inline(always)]
fn decode012(br: &mut BitReader<'_>) -> u8 {
    // upstream get_bits.h: n=get_bits1(); if n==0 return 0; else return get_bits1()+1;
    match br.read_bit() {
        Some(false) => 0,
        Some(true) => br.read_bit().map(|b| if b { 2 } else { 1 }).unwrap_or(0),
        None => 0,
    }
}

#[inline(always)]
fn wmv2_get_cbp_table_index(qscale: i32, cbp_index: u8) -> usize {
    // upstream wmv2.h wmv2_get_cbp_table_index
    const MAP: [[u8; 3]; 3] = [[0, 2, 1], [1, 0, 2], [2, 1, 0]];
    let a = if qscale > 10 { 1 } else { 0 };
    let b = if qscale > 20 { 1 } else { 0 };
    let row = (a + b) as usize;
    MAP[row][(cbp_index as usize).min(2)] as usize
}

impl MacroblockDecoder {
    /// Decode one WMV2 frame. `hdr` is the already-parsed per-frame header.
    /// This is the public entry point called from main.rs.
    /// Parse WMV2 ext-header from ASF extradata (upstream decode_ext_header).
    ///
    /// If extradata is missing/short, we keep all flags at default false.
    pub fn wmv2_set_extradata(&mut self, extradata: &[u8]) {
        if extradata.len() < 4 {
            return;
        }
        let mut br = BitReader::new(&extradata[..4]);
        let _fps = br.read_bits(5).unwrap_or(0);
        let _bit_rate = br.read_bits(11).unwrap_or(0) * 1024;
        self.wmv2_mspel_bit = br.read_bit().unwrap_or(false);
        let _loop_filter = br.read_bit().unwrap_or(false);
        self.wmv2_abt_flag = br.read_bit().unwrap_or(false);
        self.wmv2_j_type_bit = br.read_bit().unwrap_or(false);
        self.wmv2_top_left_mv_flag = br.read_bit().unwrap_or(false);
        self.wmv2_per_mb_rl_bit = br.read_bit().unwrap_or(false);
        let code = br.read_bits(3).unwrap_or(0) as usize;
        if code == 0 {
            return;
        }
        let mb_h = self.height_mb as usize;
        self.wmv2_slice_height = mb_h / code;
    }

    pub fn wmv2_copy_ref(&self, out: &mut YuvFrame) -> bool {
        let Some(r) = self.wmv2_ref.as_ref() else {
            return false;
        };
        if out.width != r.width || out.height != r.height {
            *out = r.clone();
            return true;
        }
        if out.y.len() == r.y.len() {
            out.y.copy_from_slice(&r.y);
        } else {
            out.y = r.y.clone();
        }
        if out.cb.len() == r.cb.len() {
            out.cb.copy_from_slice(&r.cb);
        } else {
            out.cb = r.cb.clone();
        }
        if out.cr.len() == r.cr.len() {
            out.cr.copy_from_slice(&r.cr);
        } else {
            out.cr = r.cr.clone();
        }
        true
    }

    pub fn decode_wmv2_frame(
        &mut self,
        payload: &[u8],
        hdr: &Wmv2FrameHeader,
        params: &Wmv2Params,
        frame: &mut YuvFrame,
    ) -> Result<()> {
        // resize if needed
        if self.width != params.width || self.height != params.height {
            *self = MacroblockDecoder::new(params.width, params.height);
        }

        if hdr.frame_skipped {
            let _ = self.wmv2_copy_ref(frame);
            return Ok(());
        }
        match hdr.frame_type {
            Wmv2FrameType::I => self.wmv2_decode_intra(payload, hdr, frame),
            Wmv2FrameType::P => self.wmv2_decode_p(payload, hdr, frame),
        }
    }

    /// Heuristic probe: try to parse a few macroblock headers after `hdr.header_bits`.
    /// Used to disambiguate ASF framing-byte offsets when the picture header can be
    /// (mis-)parsed at multiple byte offsets.
    ///
    /// Returns a "score" = number of MB headers successfully parsed (higher is better).
    pub fn probe_wmv2_payload(&self, payload: &[u8], hdr: &Wmv2FrameHeader) -> usize {
        let mut br = BitReader::new_at(payload, hdr.header_bits);

        // upstream-aligned quick probe for I-frames: only consume secondary header + MB header + 6×DC.
        if hdr.frame_type == Wmv2FrameType::I {
            let mut br = BitReader::new_at(payload, hdr.header_bits);
            // secondary picture header (I branch)
            let j_type = if self.wmv2_j_type_bit {
                br.read_bit().unwrap_or(false)
            } else {
                false
            };
            if j_type {
                return 1;
            }
            let per_mb_rl_table = if self.wmv2_per_mb_rl_bit {
                br.read_bit().unwrap_or(false)
            } else {
                false
            };
            if !per_mb_rl_table {
                let _ = decode012(&mut br);
                let _ = decode012(&mut br);
            }
            let dc_table_index = br.read_bit().unwrap_or(false) as usize;
            let code = match self.wmv2_mb_i_vlc.decode(&mut br) {
                Some(v) => v as u32,
                None => return 0,
            };
            let _ = code;
            let _ac_pred = br.read_bit().unwrap_or(false);
            let _ = _ac_pred;
            if per_mb_rl_table && code != 0 {
                let _ = decode012(&mut br);
            }
            // DCs
            const DC_MAX: i32 = 119;
            for blk in 0..6usize {
                let is_chroma = blk >= 4;
                let tbl = &self.wmv2_dc_vlc[dc_table_index][if is_chroma { 1 } else { 0 }];
                let level = match tbl.decode(&mut br) {
                    Some(v) => v,
                    None => return 0,
                };
                if level == DC_MAX {
                    let _ = br.read_bits(8);
                    let _ = br.read_bit();
                } else if level != 0 {
                    let _ = br.read_bit();
                }
            }
            return 1;
        }

        // upstream-aligned quick probe for P-frames: consume secondary header + first MB header.
        if hdr.frame_type == Wmv2FrameType::P {
            let mut br = BitReader::new_at(payload, hdr.header_bits);
            let mb_w = self.width_mb as usize;
            let mb_h = self.height_mb as usize;
            let qscale = hdr.pquant as i32;

            // skip map (only check first MB skip flag)
            let skip_type = br.read_bits(2).unwrap_or(0) as u8;
            let first_skip = match skip_type {
                0 => false,
                1 => br.read_bit().unwrap_or(false),
                2 => {
                    let all = br.read_bit().unwrap_or(false);
                    if all {
                        true
                    } else {
                        br.read_bit().unwrap_or(false)
                    }
                }
                3 => {
                    let all = br.read_bit().unwrap_or(false);
                    if all {
                        true
                    } else {
                        br.read_bit().unwrap_or(false)
                    }
                }
                _ => false,
            };

            // Drain remaining skip bits quickly (best-effort) to reach cbp_index.
            // We only do a lightweight skip consumption to keep probe cheap.
            if skip_type == 1 {
                let _ = mb_w * mb_h;
            }

            let cbp_index = decode012(&mut br);
            let cbp_table_index = wmv2_get_cbp_table_index(qscale, cbp_index);

            let _mspel = if self.wmv2_mspel_bit {
                br.read_bit().unwrap_or(false)
            } else {
                false
            };
            if self.wmv2_abt_flag {
                let per_mb_abt = br.read_bit().unwrap_or(false) ^ true;
                if !per_mb_abt {
                    let _ = decode012(&mut br);
                }
            }
            let per_mb_rl_table = if self.wmv2_per_mb_rl_bit {
                br.read_bit().unwrap_or(false)
            } else {
                false
            };
            if !per_mb_rl_table {
                let _ = decode012(&mut br);
            }
            let dc_table_index = br.read_bit().unwrap_or(false) as usize;
            let mv_table_index = br.read_bit().unwrap_or(false) as usize;

            if first_skip {
                return 1;
            }

            let code = match self.wmv2_mb_non_intra_vlc[cbp_table_index.min(3)].decode(&mut br) {
                Some(v) => v,
                None => return 0,
            };
            let mb_intra = (code & 0x40) == 0;
            let cbp = (code & 0x3f) as u8;

            if mb_intra {
                let _ac_pred = br.read_bit().unwrap_or(false);
                if per_mb_rl_table && cbp != 0 {
                    let _ = decode012(&mut br);
                }
                // Decode one DC to validate DC VLC table.
                const DC_MAX: i32 = 119;
                let tbl = &self.wmv2_dc_vlc[dc_table_index][0];
                let level = match tbl.decode(&mut br) {
                    Some(v) => v,
                    None => return 0,
                };
                if level == DC_MAX {
                    let _ = br.read_bits(8);
                    let _ = br.read_bit();
                } else if level != 0 {
                    let _ = br.read_bit();
                }
            } else {
                // Decode one MV symbol.
                let tbl = &self.wmv2_mv_vlc[mv_table_index.min(1)];
                let sym = match tbl.decode(&mut br) {
                    Some(v) => v as u16,
                    None => return 0,
                };
                if sym == 0 {
                    let _ = br.read_bits(12);
                }
            }
            return 1;
        }

        let max_mb = (self.width_mb as usize * self.height_mb as usize).min(64);
        let mut score: usize = 0;

        // Use ttcoef=0 tables for probing; this is only a syntactic plausibility check.
        let ac_intra = &self.wmv2_intra[0];
        let ac_inter = &self.wmv2_inter[0];

        for _ in 0..max_mb {
            if br.is_empty() {
                break;
            }

            let cbpc_sym = match self.wmv2_cbpc.decode(&mut br) {
                Some(v) => v,
                None => break,
            };
            if cbpc_sym == -1 {
                score += 1;
                continue;
            }
            if !(0..=3).contains(&cbpc_sym) {
                break;
            }

            let is_intra = match br.read_bit() {
                Some(b) => b,
                None => break,
            };

            let cbpy_raw = match self.wmv2_cbpy.decode(&mut br) {
                Some(v @ 0..=15) => v as u8,
                _ => break,
            };

            let cbpy = if is_intra { cbpy_raw } else { cbpy_raw ^ 0x0F };
            let cbp: u8 = (cbpy << 2) | (cbpc_sym as u8 & 0x03);

            if cbp != 0 {
                let vlc = if is_intra { ac_intra } else { ac_inter };
                let sym = match vlc.decode(&mut br) {
                    Some(s) => s,
                    None => break,
                };
                if sym == VLC_ESCAPE {
                    // Consume escape payload (mode 1/2/3) so probing stays in sync.
                    let _ = decode_escape_coeff(&mut br, vlc);
                } else {
                    // Normal coefficient: single sign bit follows.
                    let _ = br.read_bit();
                }
            }

            score += 1;
        }

        score
    }

    // ── WMV2/MSMPEG4 helpers (upstream-aligned) ─────────────────────────────

    #[inline(always)]
    fn wmv2_coded_block_pred(&self, mb_row: usize, mb_col: usize, blk: usize) -> u8 {
        // Equivalent to upstream ff_msmpeg4_coded_block_pred(), but on a compact grid.
        let bw = (self.width_mb as usize) * 2;
        let bx = mb_col * 2 + (blk & 1);
        let by = mb_row * 2 + (blk >> 1);
        let idx = by * bw + bx;
        let a = if bx > 0 {
            self.wmv2_coded_block[idx - 1]
        } else {
            0
        };
        let b = if bx > 0 && by > 0 {
            self.wmv2_coded_block[idx - 1 - bw]
        } else {
            0
        };
        let c = if by > 0 {
            self.wmv2_coded_block[idx - bw]
        } else {
            0
        };
        if b == c { a } else { c }
    }

    #[inline(always)]
    fn wmv2_coded_block_store(&mut self, mb_row: usize, mb_col: usize, blk: usize, v: u8) {
        let bw = (self.width_mb as usize) * 2;
        let bx = mb_col * 2 + (blk & 1);
        let by = mb_row * 2 + (blk >> 1);
        let idx = by * bw + bx;
        if idx < self.wmv2_coded_block.len() {
            self.wmv2_coded_block[idx] = v;
        }
    }

    #[inline(always)]
    fn wmv2_decode_dc_diff(&self, br: &mut BitReader<'_>, is_chroma: bool) -> i32 {
        // upstream msmpeg4_decode_dc() for v3+/WMV2: VLC magnitude + optional sign; DC_MAX escape.
        const DC_MAX: i32 = 119;
        let tbl = &self.wmv2_dc_vlc[self.wmv2_dc_table_index][if is_chroma { 1 } else { 0 }];
        let mut level = tbl.decode(br).unwrap_or(0);
        if level == DC_MAX {
            let v = br.read_bits(8).unwrap_or(0) as i32;
            let sign = br.read_bit().unwrap_or(false);
            return if sign { -v } else { v };
        }
        if level != 0 {
            let sign = br.read_bit().unwrap_or(false);
            if sign {
                level = -level;
            }
        }
        level
    }

    #[inline(always)]
    fn wmv2_reset_picture_state(&mut self) {
        self.wmv2_esc3_level_length = 0;
        self.wmv2_esc3_run_length = 0;
        for v in self.wmv2_ac_val.iter_mut() {
            *v = [0i16; 16];
        }
    }

    #[inline(always)]
    fn wmv2_ac_val_idx(&self, mb_row: usize, mb_col: usize, blk: usize) -> usize {
        let mb_w = self.width_mb as usize;
        (mb_row * mb_w + mb_col) * 6 + blk
    }

    #[inline(always)]
    fn wmv2_get_ac_val(&self, mb_row: usize, mb_col: usize, blk: usize) -> [i16; 16] {
        let idx = self.wmv2_ac_val_idx(mb_row, mb_col, blk);
        if idx < self.wmv2_ac_val.len() {
            self.wmv2_ac_val[idx]
        } else {
            [0i16; 16]
        }
    }

    #[inline(always)]
    fn wmv2_set_ac_val(&mut self, mb_row: usize, mb_col: usize, blk: usize, v: [i16; 16]) {
        let idx = self.wmv2_ac_val_idx(mb_row, mb_col, blk);
        if idx < self.wmv2_ac_val.len() {
            self.wmv2_ac_val[idx] = v;
        }
    }

    #[inline(always)]
    fn wmv2_pred_ac(
        &mut self,
        mb_row: usize,
        mb_col: usize,
        blk: usize,
        dc_pred_dir: i32,
        ac_pred: bool,
        block: &mut [i16; 64],
    ) {
        // Direct port of upstream ff_mpeg4_pred_ac() behavior for MSMPEG4/WMV2.
        // We keep identity idct_permutation (our scan tables are already permutated).
        // ac_val stores 16 values per block: [1..7] left column, [9..15] top row.

        let mut cur = self.wmv2_get_ac_val(mb_row, mb_col, blk);

        if ac_pred {
            if dc_pred_dir == 0 {
                // Left prediction: add first column from left neighbor.
                let (src_r, src_c, src_b) = match blk {
                    1 => (mb_row, mb_col, 0),
                    3 => (mb_row, mb_col, 2),
                    0 => (mb_row, mb_col.saturating_sub(1), 1),
                    2 => (mb_row, mb_col.saturating_sub(1), 3),
                    4 | 5 => (mb_row, mb_col.saturating_sub(1), blk),
                    _ => (mb_row, mb_col.saturating_sub(1), blk),
                };
                if (blk == 1 || blk == 3) || mb_col > 0 {
                    let src = self.wmv2_get_ac_val(src_r, src_c, src_b);
                    for i in 1..8usize {
                        let idx = i << 3;
                        block[idx] = block[idx].wrapping_add(src[i]);
                    }
                }
            } else {
                // Top prediction: add first row from top neighbor.
                let (src_r, src_c, src_b) = match blk {
                    2 => (mb_row, mb_col, 0),
                    3 => (mb_row, mb_col, 1),
                    0 => (mb_row.saturating_sub(1), mb_col, 2),
                    1 => (mb_row.saturating_sub(1), mb_col, 3),
                    4 | 5 => (mb_row.saturating_sub(1), mb_col, blk),
                    _ => (mb_row.saturating_sub(1), mb_col, blk),
                };
                if (blk == 2 || blk == 3) || mb_row > 0 {
                    let src = self.wmv2_get_ac_val(src_r, src_c, src_b);
                    for i in 1..8usize {
                        block[i] = block[i].wrapping_add(src[8 + i]);
                    }
                }
            }
        }

        // Store our AC predictors for future blocks.
        for i in 1..8usize {
            cur[i] = block[i << 3];
        }
        cur[9..16].copy_from_slice(&block[1..8]);
        self.wmv2_set_ac_val(mb_row, mb_col, blk, cur);
    }

    #[inline(always)]
    fn wmv2_unquantize_h263_intra(&self, block: &mut [i16; 64], qscale: i32, dc_scale: i32) {
        // Direct port of upstream dct_unquantize_h263_intra_c().
        let qmul = qscale << 1;
        let qadd = (qscale - 1) | 1;

        block[0] = ((block[0] as i32) * dc_scale) as i16;
        for i in 1..64usize {
            let mut level = block[i] as i32;
            if level != 0 {
                if level < 0 {
                    level = level * qmul - qadd;
                } else {
                    level = level * qmul + qadd;
                }
                block[i] = level as i16;
            }
        }
    }

    fn wmv2_decode_block_intra_ref(
        &mut self,
        br: &mut BitReader<'_>,
        mb_row: usize,
        mb_col: usize,
        blk: usize,
        coded: bool,
        qscale: i32,
        ac_pred: bool,
    ) -> Result<[i16; 64]> {
        let is_luma = blk < 4;
        let dc_scale = wmv2_dc_scale(qscale, is_luma);

        // DC diff VLC + sign, predictor in DC level domain.
        let diff = self.wmv2_decode_dc_diff(br, !is_luma);
        let (pred_level, dir) = self.wmv2_dc_pred.predict(mb_row, mb_col, blk, dc_scale);
        let level = pred_level + diff;
        self.wmv2_dc_pred
            .store(mb_row, mb_col, blk, level * dc_scale);

        let mut block = [0i16; 64];
        block[0] = level as i16;

        // Choose RL table.
        let rl = if is_luma {
            &self.wmv2_rl[(self.wmv2_rl_table_index as usize).min(2)]
        } else {
            &self.wmv2_rl[3 + (self.wmv2_rl_chroma_table_index as usize).min(2)]
        };

        // Scan table selection.
        let scan = if ac_pred {
            if dir == 0 {
                &FF_WMV1_SCANTABLE[3] // intra_v
            } else {
                &FF_WMV1_SCANTABLE[2] // intra_h
            }
        } else {
            &FF_WMV1_SCANTABLE[1] // intra default
        };

        let mut i: i32 = 0;
        let qmul: i32 = 1;
        let run_diff: i32 = 1; // msmpeg4_version >= WMV1

        if coded {
            loop {
                let (mut level_uq, mut run) = rl
                    .decode_sym(br, 0)
                    .ok_or_else(|| DecoderError::InvalidData("WMV2: tcoeff VLC underrun".into()))?;

                if level_uq == 0 {
                    // escape: prefix bits decide which escape.
                    let b0 = br.peek_bits(1).unwrap_or(0);
                    if b0 == 1 {
                        // escape1: prefix '1'
                        br.skip_bits(1);
                        let (lvl2, run2) = rl.decode_sym(br, 0).ok_or_else(|| {
                            DecoderError::InvalidData("WMV2: escape1 VLC underrun".into())
                        })?;
                        level_uq = lvl2;
                        run = run2;
                        i += run;
                        let last = ((run >> 7) & 1) as usize;
                        let base_run = ((run - 1) & 63) as usize;
                        level_uq += rl.max_level_for(last, base_run) * qmul;
                        let sign = br.read_bit().unwrap_or(false);
                        if sign {
                            level_uq = -level_uq;
                        }
                    } else {
                        let b1 = br.peek_bits(2).unwrap_or(0) & 1;
                        if b1 == 1 {
                            // escape2: prefix '01'
                            br.skip_bits(2);
                            let (lvl2, run2) = rl.decode_sym(br, 0).ok_or_else(|| {
                                DecoderError::InvalidData("WMV2: escape2 VLC underrun".into())
                            })?;
                            level_uq = lvl2;
                            run = run2;
                            let last = ((run >> 7) & 1) as usize;
                            let base_level = (level_uq / qmul).unsigned_abs() as usize;
                            i += run + rl.max_run_for(last, base_level) + run_diff;
                            let sign = br.read_bit().unwrap_or(false);
                            if sign {
                                level_uq = -level_uq;
                            }
                        } else {
                            // escape3: prefix '00'
                            br.skip_bits(2);
                            let last = br.read_bit().unwrap_or(false);
                            if self.wmv2_esc3_level_length == 0 {
                                // derive esc3 lengths (WMV2: msmpeg4_version > V3)
                                let ll: u8 = if qscale < 8 {
                                    let mut x = br.read_bits(3).unwrap_or(0) as u8;
                                    if x == 0 {
                                        x = 8 + br.read_bits(1).unwrap_or(0) as u8;
                                    }
                                    x
                                } else {
                                    let mut x: u8 = 2;
                                    while x < 8 && br.peek_bits(1).unwrap_or(1) == 0 {
                                        br.skip_bits(1);
                                        x += 1;
                                    }
                                    if x < 8 {
                                        br.skip_bits(1);
                                    }
                                    x
                                };
                                self.wmv2_esc3_level_length = ll;
                                self.wmv2_esc3_run_length =
                                    (br.read_bits(2).unwrap_or(0) as u8) + 3;
                            }
                            let run_abs =
                                br.read_bits(self.wmv2_esc3_run_length).unwrap_or(0) as i32;
                            let sign = br.read_bit().unwrap_or(false);
                            let mut lvl_abs =
                                br.read_bits(self.wmv2_esc3_level_length).unwrap_or(0) as i32;
                            if sign {
                                lvl_abs = -lvl_abs;
                            }
                            level_uq = lvl_abs;
                            i += run_abs + 1;
                            if last {
                                i += 192;
                            }
                        }
                    }
                } else {
                    i += run;
                    let sign = br.read_bit().unwrap_or(false);
                    if sign {
                        level_uq = -level_uq;
                    }
                }

                if i > 62 {
                    i -= 192;
                    if (i & !63) != 0 {
                        i = 63;
                    }
                    if i < 0 {
                        return Err(DecoderError::InvalidData(
                            "WMV2: negative coeff index (bitstream damaged)".into(),
                        ));
                    }
                    let pos = scan[i as usize];
                    if pos < 64 {
                        block[pos] = level_uq as i16;
                    }
                    break;
                }

                if i < 0 {
                    return Err(DecoderError::InvalidData(
                        "WMV2: negative coeff index (bitstream damaged)".into(),
                    ));
                }
                let pos = scan[i as usize];
                if pos < 64 {
                    block[pos] = level_uq as i16;
                }
            }
        }

        // AC prediction always runs (even if not coded).
        self.wmv2_pred_ac(mb_row, mb_col, blk, dir, ac_pred, &mut block);

        // H.263 intra unquantization to match upstream pipeline.
        self.wmv2_unquantize_h263_intra(&mut block, qscale, dc_scale);

        Ok(block)
    }
    fn wmv2_decode_block_inter_ref(
        &mut self,
        br: &mut BitReader<'_>,
        blk: usize,
        coded: bool,
        qscale: i32,
        scan: &[usize; 64],
    ) -> Result<[i16; 64]> {
        let mut block = [0i16; 64];
        if !coded {
            return Ok(block);
        }

        let rl = &self.wmv2_rl[3 + (self.wmv2_rl_table_index as usize).min(2)];

        let qmul = qscale << 1;
        let qadd = (qscale - 1) | 1;
        let run_diff: i32 = 1; // wmv2 != v2

        let mut i: i32 = -1;

        loop {
            let (mut level_uq, mut run) = rl.decode_sym(br, qscale).ok_or_else(|| {
                DecoderError::InvalidData("WMV2: inter tcoeff VLC underrun".into())
            })?;

            if level_uq == 0 {
                // escape
                let b0 = br.peek_bits(1).unwrap_or(0);
                if b0 == 1 {
                    // escape1
                    br.skip_bits(1);
                    let (lvl2, run2) = rl.decode_sym(br, qscale).ok_or_else(|| {
                        DecoderError::InvalidData("WMV2: inter escape1 VLC underrun".into())
                    })?;
                    level_uq = lvl2;
                    run = run2;
                    i += run;
                    let last = ((run >> 7) & 1) as usize;
                    let base_run = ((run - 1) & 63) as usize;
                    level_uq += rl.max_level_for(last, base_run) * qmul;
                    let sign = br.read_bit().unwrap_or(false);
                    if sign {
                        level_uq = -level_uq;
                    }
                } else {
                    let b1 = br.peek_bits(2).unwrap_or(0) & 1;
                    if b1 == 1 {
                        // escape2
                        br.skip_bits(2);
                        let (lvl2, run2) = rl.decode_sym(br, qscale).ok_or_else(|| {
                            DecoderError::InvalidData("WMV2: inter escape2 VLC underrun".into())
                        })?;
                        level_uq = lvl2;
                        run = run2;
                        let last = ((run >> 7) & 1) as usize;
                        let base_level = (level_uq / qmul).unsigned_abs() as usize;
                        i += run + rl.max_run_for(last, base_level) + run_diff;
                        let sign = br.read_bit().unwrap_or(false);
                        if sign {
                            level_uq = -level_uq;
                        }
                    } else {
                        // escape3
                        br.skip_bits(2);
                        let last = br.read_bit().unwrap_or(false);
                        if self.wmv2_esc3_level_length == 0 {
                            let ll: u8 = if qscale < 8 {
                                let mut x = br.read_bits(3).unwrap_or(0) as u8;
                                if x == 0 {
                                    x = 8 + br.read_bits(1).unwrap_or(0) as u8;
                                }
                                x
                            } else {
                                let mut x: u8 = 2;
                                while x < 8 && br.peek_bits(1).unwrap_or(1) == 0 {
                                    br.skip_bits(1);
                                    x += 1;
                                }
                                if x < 8 {
                                    br.skip_bits(1);
                                }
                                x
                            };
                            self.wmv2_esc3_level_length = ll;
                            self.wmv2_esc3_run_length = (br.read_bits(2).unwrap_or(0) as u8) + 3;
                        }
                        let run_abs = br.read_bits(self.wmv2_esc3_run_length).unwrap_or(0) as i32;
                        let sign = br.read_bit().unwrap_or(false);
                        let mut lvl_abs =
                            br.read_bits(self.wmv2_esc3_level_length).unwrap_or(0) as i32;
                        if sign {
                            lvl_abs = -lvl_abs;
                        }
                        if lvl_abs > 0 {
                            level_uq = lvl_abs * qmul + qadd;
                        } else {
                            level_uq = lvl_abs * qmul - qadd;
                        }
                        i += run_abs + 1;
                        if last {
                            i += 192;
                        }
                    }
                }
            } else {
                i += run;
                let sign = br.read_bit().unwrap_or(false);
                if sign {
                    level_uq = -level_uq;
                }
            }

            if i > 62 {
                i -= 192;
                if (i & !63) != 0 {
                    i = 63;
                }
                if i < 0 {
                    return Err(DecoderError::InvalidData(
                        "WMV2: negative coeff index (bitstream damaged)".into(),
                    ));
                }
                let pos = scan[i as usize];
                if pos < 64 {
                    block[pos] = level_uq as i16;
                }
                break;
            }

            if i < 0 {
                return Err(DecoderError::InvalidData(
                    "WMV2: negative coeff index (bitstream damaged)".into(),
                ));
            }
            let pos = scan[i as usize];
            if pos < 64 {
                block[pos] = level_uq as i16;
            }
        }

        let _ = blk;
        Ok(block)
    }
    fn wmv2_parse_mb_skip(
        &mut self,
        br: &mut BitReader<'_>,
        mb_w: usize,
        mb_h: usize,
    ) -> Result<()> {
        // upstream wmv2dec.c parse_mb_skip
        let skip_type = br
            .read_bits(2)
            .ok_or_else(|| DecoderError::InvalidData("WMV2: missing skip_type".into()))?
            as u8;
        self.wmv2_skip_type = skip_type;
        if self.wmv2_mb_skip.len() != mb_w * mb_h {
            self.wmv2_mb_skip.resize(mb_w * mb_h, false);
        }
        for v in self.wmv2_mb_skip.iter_mut() {
            *v = false;
        }

        match skip_type {
            0 => {
                // SKIP_TYPE_NONE
            }
            1 => {
                // SKIP_TYPE_MPEG: 1 bit per MB
                if br.bits_left() < (mb_w * mb_h) as isize {
                    return Err(DecoderError::InvalidData("WMV2: skip map truncated".into()));
                }
                for y in 0..mb_h {
                    for x in 0..mb_w {
                        let b = br.read_bit().unwrap_or(false);
                        self.wmv2_mb_skip[y * mb_w + x] = b;
                    }
                }
            }
            2 => {
                // SKIP_TYPE_ROW
                for y in 0..mb_h {
                    let all = br.read_bit().ok_or_else(|| {
                        DecoderError::InvalidData("WMV2: skip row flag missing".into())
                    })?;
                    if all {
                        for x in 0..mb_w {
                            self.wmv2_mb_skip[y * mb_w + x] = true;
                        }
                    } else {
                        for x in 0..mb_w {
                            let b = br.read_bit().unwrap_or(false);
                            self.wmv2_mb_skip[y * mb_w + x] = b;
                        }
                    }
                }
            }
            3 => {
                // SKIP_TYPE_COL
                for x in 0..mb_w {
                    let all = br.read_bit().ok_or_else(|| {
                        DecoderError::InvalidData("WMV2: skip col flag missing".into())
                    })?;
                    if all {
                        for y in 0..mb_h {
                            self.wmv2_mb_skip[y * mb_w + x] = true;
                        }
                    } else {
                        for y in 0..mb_h {
                            let b = br.read_bit().unwrap_or(false);
                            self.wmv2_mb_skip[y * mb_w + x] = b;
                        }
                    }
                }
            }
            _ => {}
        }

        // upstream also checks coded_mb_count against bits_left; keep a light version.
        let coded = self.wmv2_mb_skip.iter().filter(|s| !**s).count();
        if coded as isize > br.bits_left() {
            return Err(DecoderError::InvalidData(
                "WMV2: coded MB count exceeds remaining bits".into(),
            ));
        }
        Ok(())
    }

    #[inline(always)]
    fn wmv2_motion_get(&self, mb_row: isize, mb_col: isize) -> (i32, i32) {
        if mb_row < 0 || mb_col < 0 {
            return (0, 0);
        }
        let mb_w = self.width_mb as isize;
        let mb_h = self.height_mb as isize;
        if mb_row >= mb_h || mb_col >= mb_w {
            return (0, 0);
        }
        let idx = (mb_row as usize) * (mb_w as usize) + (mb_col as usize);
        if idx < self.wmv2_motion.len() {
            self.wmv2_motion[idx]
        } else {
            (0, 0)
        }
    }

    #[inline(always)]
    fn wmv2_motion_set(&mut self, mb_row: usize, mb_col: usize, mv: (i32, i32)) {
        let mb_w = self.width_mb as usize;
        let idx = mb_row * mb_w + mb_col;
        if self.wmv2_motion.len() != mb_w * (self.height_mb as usize) {
            self.wmv2_motion
                .resize(mb_w * (self.height_mb as usize), (0, 0));
        }
        if idx < self.wmv2_motion.len() {
            self.wmv2_motion[idx] = mv;
        }
    }

    #[inline(always)]
    fn wmv2_pred_motion(
        &self,
        br: &mut BitReader<'_>,
        mb_row: usize,
        mb_col: usize,
        first_slice_line: bool,
    ) -> (i32, i32) {
        // upstream wmv2dec.c wmv2_pred_motion (MB-level approximation).
        let a = self.wmv2_motion_get(mb_row as isize, mb_col as isize - 1);
        let b = self.wmv2_motion_get(mb_row as isize - 1, mb_col as isize);
        let c = self.wmv2_motion_get(mb_row as isize - 1, mb_col as isize + 1);

        let diff =
            if mb_col != 0 && !first_slice_line && !self.wmv2_mspel && self.wmv2_top_left_mv_flag {
                let dx = (a.0 - b.0).abs();
                let dy = (a.1 - b.1).abs();
                dx.max(dy)
            } else {
                0
            };

        let t = if diff >= 8 {
            if br.read_bit().unwrap_or(false) { 1 } else { 0 }
        } else {
            2
        };

        match t {
            0 => a,
            1 => b,
            _ => {
                if first_slice_line {
                    a
                } else {
                    (mid_pred(a.0, b.0, c.0), mid_pred(a.1, b.1, c.1))
                }
            }
        }
    }

    #[inline(always)]
    fn wmv2_decode_motion_ref(&self, br: &mut BitReader<'_>, pred: (i32, i32)) -> (i32, i32) {
        // Direct port of upstream msmpeg4dec.c ff_msmpeg4_decode_motion.
        let tbl = &self.wmv2_mv_vlc[self.wmv2_mv_table_index.min(1)];
        let sym = tbl.decode(br).unwrap_or(0) as u16;
        let (mut mx, mut my) = if sym != 0 {
            ((sym >> 8) as i32, (sym & 0xff) as i32)
        } else {
            // Escape: 6-bit mx + 6-bit my.
            (
                br.read_bits(6).unwrap_or(0) as i32,
                br.read_bits(6).unwrap_or(0) as i32,
            )
        };

        mx += pred.0 - 32;
        my += pred.1 - 32;
        // WARNING: they do not do exactly modulo encoding.
        if mx <= -64 {
            mx += 64;
        } else if mx >= 64 {
            mx -= 64;
        }
        if my <= -64 {
            my += 64;
        } else if my >= 64 {
            my -= 64;
        }
        (mx, my)
    }

    // ── WMV2 I-frame ──────────────────────────────────────────────────────────

    fn wmv2_decode_intra(
        &mut self,
        payload: &[u8],
        hdr: &Wmv2FrameHeader,
        frame: &mut YuvFrame,
    ) -> Result<()> {
        // Start at picture header end.
        let mut br = BitReader::new_at(payload, hdr.header_bits);

        // upstream: ff_wmv2_decode_secondary_picture_header() (I-picture branch).
        // We parse/consume the fields that affect alignment and DC VLC selection.
        self.wmv2_j_type = if self.wmv2_j_type_bit {
            br.read_bit().unwrap_or(false)
        } else {
            false
        };
        if self.wmv2_j_type {
            // IntraX8 (j_type) is not handled in this A build.
            return Ok(());
        }

        self.wmv2_per_mb_rl_table = if self.wmv2_per_mb_rl_bit {
            br.read_bit().unwrap_or(false)
        } else {
            false
        };
        if !self.wmv2_per_mb_rl_table {
            self.wmv2_rl_chroma_table_index = decode012(&mut br);
            self.wmv2_rl_table_index = decode012(&mut br);
        }
        self.wmv2_dc_table_index = br.read_bit().unwrap_or(false) as usize;

        let mb_w = self.width_mb as usize;
        let mb_h = self.height_mb as usize;

        // Reset predictors.
        self.wmv2_dc_pred = Wmv2DcPredBuffer::new(mb_w, mb_h);
        for v in self.wmv2_coded_block.iter_mut() {
            *v = 0;
        }

        self.wmv2_reset_picture_state();

        let qscale = hdr.pquant as i32;
        // WMV2 picture header variant used here (upstream-min) does not carry ttcoef;
        // keep using intra VLC set 0 to get the stream back in sync.

        for mb_row in 0..mb_h {
            for mb_col in 0..mb_w {
                if br.is_empty() {
                    break;
                }

                // upstream: code = get_vlc2(ff_msmp4_mb_i_vlc)
                let code = self.wmv2_mb_i_vlc.decode(&mut br).unwrap_or(0) as u32;

                // Predict coded block pattern.
                let mut cbp: u8 = 0;
                for i in 0..6usize {
                    let mut val = ((code >> (5 - i)) & 1) as u8;
                    if i < 4 {
                        let pred = self.wmv2_coded_block_pred(mb_row, mb_col, i);
                        val ^= pred;
                        self.wmv2_coded_block_store(mb_row, mb_col, i, val);
                    }
                    cbp |= val << (5 - i);
                }

                // upstream: h->c.ac_pred = get_bits1();
                let ac_pred = br.read_bit().unwrap_or(false);

                // upstream: if (per_mb_rl_table && cbp) rl_table_index = decode012();
                if self.wmv2_per_mb_rl_table && cbp != 0 {
                    let rl_idx = decode012(&mut br);
                    self.wmv2_rl_table_index = rl_idx;
                    self.wmv2_rl_chroma_table_index = rl_idx;
                }

                for blk in 0..6usize {
                    let coded = ((cbp >> (5 - blk)) & 1) != 0;
                    let mut block = self.wmv2_decode_block_intra_ref(
                        &mut br, mb_row, mb_col, blk, coded, qscale, ac_pred,
                    )?;

                    let (is_luma, bx, by, stride, _ph) =
                        block_coords(mb_row as u32, mb_col as u32, blk, frame.width, frame.height);
                    let plane: &mut Vec<u8> = if is_luma {
                        &mut frame.y
                    } else if blk == 4 {
                        &mut frame.cb
                    } else {
                        &mut frame.cr
                    };
                    let dst_off = by * stride + bx;
                    wmv2dsp::wmv2_idct_put(plane, dst_off, stride, &mut block);
                }
            }
        }

        self.wmv2_ref = Some(frame.clone());
        Ok(())
    }
    // ── WMV2 P-frame ──────────────────────────────────────────────────────────

    fn wmv2_decode_p(
        &mut self,
        payload: &[u8],
        hdr: &Wmv2FrameHeader,
        frame: &mut YuvFrame,
    ) -> Result<()> {
        // Start at picture header end.
        let mut br = BitReader::new_at(payload, hdr.header_bits);

        let mb_w = self.width_mb as usize;
        let mb_h = self.height_mb as usize;
        let qscale = hdr.pquant as i32;

        // upstream: ff_wmv2_decode_secondary_picture_header() (P-picture branch).
        self.wmv2_j_type = false;
        self.wmv2_parse_mb_skip(&mut br, mb_w, mb_h)?;
        let cbp_index = decode012(&mut br);
        self.wmv2_cbp_table_index = wmv2_get_cbp_table_index(qscale, cbp_index);

        self.wmv2_mspel = if self.wmv2_mspel_bit {
            br.read_bit().unwrap_or(false)
        } else {
            false
        };

        if self.wmv2_abt_flag {
            self.wmv2_per_mb_abt = br.read_bit().unwrap_or(false) ^ true;
            if !self.wmv2_per_mb_abt {
                self.wmv2_abt_type = decode012(&mut br);
            }
        } else {
            self.wmv2_per_mb_abt = false;
            self.wmv2_abt_type = 0;
        }

        self.wmv2_per_mb_rl_table = if self.wmv2_per_mb_rl_bit {
            br.read_bit().unwrap_or(false)
        } else {
            false
        };
        if !self.wmv2_per_mb_rl_table {
            self.wmv2_rl_table_index = decode012(&mut br);
            self.wmv2_rl_chroma_table_index = self.wmv2_rl_table_index;
        }
        if br.bits_left() < 2 {
            return Err(DecoderError::InvalidData(
                "WMV2: truncated secondary header".into(),
            ));
        }
        self.wmv2_dc_table_index = br.read_bit().unwrap_or(false) as usize;
        self.wmv2_mv_table_index = br.read_bit().unwrap_or(false) as usize;

        // Reset predictors for this picture.
        self.wmv2_dc_pred = Wmv2DcPredBuffer::new(mb_w, mb_h);
        if self.wmv2_motion.len() != mb_w * mb_h {
            self.wmv2_motion.resize(mb_w * mb_h, (0, 0));
        }
        for v in self.wmv2_motion.iter_mut() {
            *v = (0, 0);
        }

        self.wmv2_reset_picture_state();

        let reference = match &self.wmv2_ref {
            Some(r) => r.clone(),
            None => YuvFrame::new(frame.width, frame.height),
        };

        for mb_row in 0..mb_h {
            let first_slice_line =
                self.wmv2_slice_height != 0 && (mb_row % self.wmv2_slice_height == 0);
            for mb_col in 0..mb_w {
                if br.bits_left() <= 0 {
                    break;
                }
                let mi = mb_row * mb_w + mb_col;
                if mi < self.wmv2_mb_skip.len() && self.wmv2_mb_skip[mi] {
                    if self.wmv2_mspel {
                        wmv2_mspel_motion_mb(frame, &reference, mb_row, mb_col, 0, 0, 0);
                    } else {
                        motion_compensate_mb(frame, &reference, mb_row, mb_col, 0, 0);
                    }
                    self.wmv2_motion_set(mb_row, mb_col, (0, 0));
                    continue;
                }

                let code = self.wmv2_mb_non_intra_vlc[self.wmv2_cbp_table_index.min(3)]
                    .decode(&mut br)
                    .ok_or_else(|| {
                        DecoderError::InvalidData("WMV2: MB header VLC underrun".into())
                    })?;

                let mb_intra = (code & 0x40) == 0;
                let cbp = (code & 0x3f) as u8;

                if !mb_intra {
                    let pred = self.wmv2_pred_motion(&mut br, mb_row, mb_col, first_slice_line);

                    if cbp != 0 && self.wmv2_per_mb_rl_table {
                        self.wmv2_rl_table_index = decode012(&mut br);
                        self.wmv2_rl_chroma_table_index = self.wmv2_rl_table_index;
                    }

                    let mut per_block_abt = false;
                    let mut abt_type = self.wmv2_abt_type;
                    if cbp != 0 && self.wmv2_abt_flag && self.wmv2_per_mb_abt {
                        per_block_abt = br.read_bit().unwrap_or(false);
                        if !per_block_abt {
                            abt_type = decode012(&mut br);
                        }
                    }

                    let (mx, my) = self.wmv2_decode_motion_ref(&mut br, pred);
                    self.wmv2_hshift = if (((mx | my) & 1) != 0) && self.wmv2_mspel {
                        br.read_bit().unwrap_or(false) as u8
                    } else {
                        0
                    };
                    self.wmv2_motion_set(mb_row, mb_col, (mx, my));

                    if self.wmv2_mspel {
                        wmv2_mspel_motion_mb(
                            frame,
                            &reference,
                            mb_row,
                            mb_col,
                            mx,
                            my,
                            self.wmv2_hshift,
                        );
                    } else {
                        motion_compensate_mb(frame, &reference, mb_row, mb_col, mx, my);
                    }

                    for blk in 0..6usize {
                        if (cbp >> (5 - blk)) & 1 == 0 {
                            continue;
                        }

                        let mut cur_abt = abt_type;
                        if per_block_abt {
                            cur_abt = decode012(&mut br);
                        }

                        // upstream: wmv2_decode_inter_block + wmv2_add_block

                        if cur_abt == 0 {
                            let scan = &FF_WMV1_SCANTABLE[0];

                            let mut block =
                                self.wmv2_decode_block_inter_ref(&mut br, blk, true, qscale, scan)?;

                            let (is_luma, bx, by, stride, _ph) = block_coords(
                                mb_row as u32,
                                mb_col as u32,
                                blk,
                                frame.width,
                                frame.height,
                            );

                            let plane: &mut Vec<u8> = if is_luma {
                                &mut frame.y
                            } else if blk == 4 {
                                &mut frame.cb
                            } else {
                                &mut frame.cr
                            };

                            let dst_off = by * stride + bx;

                            wmv2dsp::wmv2_idct_add(plane, dst_off, stride, &mut block);
                        } else {
                            const SUB_CBP_TABLE: [u8; 3] = [2, 3, 1];

                            let scantable = if cur_abt == 1 {
                                &FF_WMV2_SCANTABLE_A
                            } else {
                                &FF_WMV2_SCANTABLE_B
                            };

                            let sub_cbp = SUB_CBP_TABLE[decode012(&mut br) as usize];

                            let mut block1 = [0i16; 64];

                            let mut block2 = [0i16; 64];

                            if (sub_cbp & 1) != 0 {
                                block1 = self.wmv2_decode_block_inter_ref(
                                    &mut br, blk, true, qscale, scantable,
                                )?;
                            }

                            if (sub_cbp & 2) != 0 {
                                block2 = self.wmv2_decode_block_inter_ref(
                                    &mut br, blk, true, qscale, scantable,
                                )?;
                            }

                            let (is_luma, bx, by, stride, _ph) = block_coords(
                                mb_row as u32,
                                mb_col as u32,
                                blk,
                                frame.width,
                                frame.height,
                            );

                            let plane: &mut Vec<u8> = if is_luma {
                                &mut frame.y
                            } else if blk == 4 {
                                &mut frame.cb
                            } else {
                                &mut frame.cr
                            };

                            let dst_off = by * stride + bx;

                            match cur_abt {
                                1 => {
                                    // 8x4 + 8x4 (top/bottom)

                                    ffidct::ff_simple_idct84_add(
                                        plane,
                                        dst_off,
                                        stride,
                                        &mut block1,
                                    );

                                    ffidct::ff_simple_idct84_add(
                                        plane,
                                        dst_off + 4 * stride,
                                        stride,
                                        &mut block2,
                                    );
                                }

                                2 => {
                                    // 4x8 + 4x8 (left/right)

                                    ffidct::ff_simple_idct48_add(
                                        plane,
                                        dst_off,
                                        stride,
                                        &mut block1,
                                    );

                                    ffidct::ff_simple_idct48_add(
                                        plane,
                                        dst_off + 4,
                                        stride,
                                        &mut block2,
                                    );
                                }

                                _ => {}
                            }
                        }
                    }
                } else {
                    // Intra MB in P-picture.
                    let ac_pred = br.read_bit().unwrap_or(false);
                    if self.wmv2_per_mb_rl_table && cbp != 0 {
                        let rl_idx = decode012(&mut br);
                        self.wmv2_rl_table_index = rl_idx;
                        self.wmv2_rl_chroma_table_index = rl_idx;
                    }

                    for blk in 0..6usize {
                        let coded = ((cbp >> (5 - blk)) & 1) != 0;
                        let mut block = self.wmv2_decode_block_intra_ref(
                            &mut br, mb_row, mb_col, blk, coded, qscale, ac_pred,
                        )?;

                        let (is_luma, bx, by, stride, _ph) = block_coords(
                            mb_row as u32,
                            mb_col as u32,
                            blk,
                            frame.width,
                            frame.height,
                        );
                        let plane: &mut Vec<u8> = if is_luma {
                            &mut frame.y
                        } else if blk == 4 {
                            &mut frame.cb
                        } else {
                            &mut frame.cr
                        };
                        let dst_off = by * stride + bx;
                        wmv2dsp::wmv2_idct_put(plane, dst_off, stride, &mut block);
                    }
                    self.wmv2_motion_set(mb_row, mb_col, (0, 0));
                }
            }
        }

        self.wmv2_ref = Some(frame.clone());
        Ok(())
    }
}

// ─── WMV2 AC block decoder ────────────────────────────────────────────────────
// Decodes AC coefficients using WMV2 TCOEF VLC.
// For intra: fills coeff[1..63] (coeff[0] is DC, already set by caller).
// For inter: fills coeff[0..63] (all AC).
// Escape is Mode-3 only: 1-bit LAST + 6-bit RUN + 8-bit |LEVEL| + 1-bit SIGN.

fn wmv2_decode_ac_block(
    br: &mut BitReader<'_>,
    ac_vlc: &VlcTable,
    pquant: i32,
    coeff: &mut [i32; 64],
    is_intra: bool,
) {
    // WMV2/MSMPEG4 uses the standard zig-zag scan by default.
    // (AC prediction, if implemented, switches to horizontal/vertical scans.)
    let scan = &ZIGZAG;
    let mut idx = if is_intra { 1usize } else { 0 };

    while let Some(sym) = ac_vlc.decode(br) {
        let (run, signed_level, last) = if sym == VLC_ESCAPE {
            // WMV2/MSMPEG4 uses the same 3-mode escape structure as VC-1:
            //   0  -> mode1 (level offset)
            //   10 -> mode2 (run offset)
            //   11 -> mode3 (absolute)
            decode_escape_coeff(br, ac_vlc)
        } else {
            let (r, l, last) = unpack_rl(sym);
            let sign = br.read_bit().unwrap_or(false);
            (r, if sign { -(l as i32) } else { l as i32 }, last)
        };

        idx = idx.saturating_add(run as usize);
        if idx >= 64 {
            break;
        }

        // Uniform quantization.
        let q = iquant_uniform(signed_level, pquant, false);
        coeff[scan[idx]] = q;

        idx += 1;
        if last || br.is_empty() {
            break;
        }
    }
}

// ─── WMV2 MV reader ───────────────────────────────────────────────────────────
// Reads a differential MV using a fixed 7-bit Huffman code (simplified from
// H.263 MVD table) then adds the median predictor.

fn wmv2_read_mv(
    br: &mut BitReader<'_>,
    mv_pred: &MvPredictor,
    mb_row: usize,
    mb_col: usize,
    mv_range: i32,
) -> (i32, i32) {
    let (px, py) = mv_pred.predict(mb_row, mb_col);
    let dx = wmv2_read_mv_component(br, mv_range);
    let dy = wmv2_read_mv_component(br, mv_range);
    (px + dx, py + dy)
}

/// Read one MV component using H.263-style VLC differential coding.
/// Values are half-pel units in range [-mv_range, mv_range-1].
fn wmv2_read_mv_component(br: &mut BitReader<'_>, mv_range: i32) -> i32 {
    // H.263 MVD VLC: unary + suffix
    // Code for 0:     "1"         (1 bit)
    // Code for ±1:    "010"/"011" (3 bits)
    // Code for ±2:    "00110"/"00111"
    // etc.  — this is a simple magnitude + sign scheme
    let mag = {
        let mut m = 0i32;
        loop {
            if br.read_bit().unwrap_or(true) {
                break;
            }
            m += 1;
            if m >= mv_range {
                break;
            }
        }
        m
    };
    if mag == 0 {
        return 0;
    }
    let sign = br.read_bit().unwrap_or(false);
    if sign { -mag } else { mag }
}

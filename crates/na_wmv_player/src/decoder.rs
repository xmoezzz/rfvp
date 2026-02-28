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
use crate::vc1::{FrameType, PictureHeader, SequenceHeader};
use crate::vlc::{
    cbpcy_i_vlc, cbpcy_p_vlc, dc_chroma_vlc, dc_luma_vlc, inter_tcoef_vlc, intra_tcoef_vlc,
    mv_diff_vlc, ttblk_vlc, ttmb_vlc, unpack_rl, VlcTable, VLC_ESCAPE,
    SCAN_INTRA, SCAN_VERT, ZIGZAG,
    wmv2_tcoef_inter_vlc, wmv2_tcoef_intra_vlc, wmv2_cbpy_vlc, wmv2_cbpc_p_vlc,
};
use crate::wmv2::{Wmv2FrameHeader, Wmv2FrameType, Wmv2Params};
use crate::vlc_tree::VlcTree;
use crate::na_wmv2_tables::{FF_MSMP4_MB_I_TABLE, FF_MSMP4_DC_TABLES};
use crate::na_msmpeg4_tables::{FF_MB_NON_INTRA_TABLES};
use crate::na_msmpeg4_mv_tables::{
    FF_MSMP4_MV_TABLE0, FF_MSMP4_MV_TABLE0_LENS,
    FF_MSMP4_MV_TABLE1, FF_MSMP4_MV_TABLE1_LENS,
};
use crate::na_rl_tables::{FF_RL_BASES, FF_WMV1_SCANTABLE, FF_WMV2_SCANTABLE_A, FF_WMV2_SCANTABLE_B};
use crate::na_simple_idct as ffidct;
use crate::na_wmv2dsp as wmv2dsp;

// ─── Frame buffer ────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct YuvFrame {
    pub width:  u32,
    pub height: u32,
    pub y:      Vec<u8>,
    pub cb:     Vec<u8>,
    pub cr:     Vec<u8>,
}

impl YuvFrame {
    pub fn new(width: u32, height: u32) -> Self {
        let y_sz  = (width * height) as usize;
        let uv_sz = y_sz / 4;
        YuvFrame {
            width, height,
            y:  vec![16u8;  y_sz],
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

// ─── VC-1 IDCT ───────────────────────────────────────────────────────────────
// SMPTE 421M §4.4.1 — Simple/Main profile integer IDCT.
//
// Exact butterfly constants: 12, 6, 16, 15, 9, 4  (no floating point).
// Row pass produces values × 8; column pass divides by 128 (>>7).
// Total normalization: ×8 / 128 = 1/16  per spatial pixel per coefficient.
//

/// One-dimensional 8-point VC-1 inverse DCT row kernel.
/// Input/output in-place.  Output is NOT yet shifted (caller does >>7 in col pass).
#[inline(always)]
fn idct_row8(b: &mut [i32; 8]) {
    // Even part
    let t1 = 12 * b[2] + 6 * b[6];
    let t2 =  6 * b[2] - 12 * b[6];
    let mut s0 = b[0] + b[4];
    let mut s1 = b[0] - b[4];
    let s2 = s1 + (t2 >> 3);
    let s3 = s0 - (t1 >> 3);
    s0    += t1 >> 3;
    s1    -= t2 >> 3;

    // Odd part
    let t0 = 16 * b[1] + 15 * b[3] +  9 * b[5] + 4 * b[7];
    let t1 = 15 * b[1] -  4 * b[3] - 16 * b[5] - 9 * b[7];
    let t2 =  9 * b[1] - 16 * b[3] +  4 * b[5] + 15 * b[7];
    let t3 =  4 * b[1] -  9 * b[3] + 15 * b[5] - 16 * b[7];

    b[0] = s0 + (t0 >> 3);
    b[1] = s2 + (t2 >> 3);
    b[2] = s1 + (t3 >> 3);
    b[3] = s3 + (t1 >> 3);
    b[4] = s3 - (t1 >> 3);
    b[5] = s1 - (t3 >> 3);
    b[6] = s2 - (t2 >> 3);
    b[7] = s0 - (t0 >> 3);
}

/// One-dimensional 4-point VC-1 inverse DCT row kernel (SMPTE 421M §4.4.2).
#[inline(always)]
fn idct_row4(b: &[i32; 4]) -> [i32; 4] {
    let t0 = 17 * b[0] + 17 * b[2];
    let t1 = 17 * b[0] - 17 * b[2];
    let t2 = 22 * b[1] + 10 * b[3];
    let t3 = 10 * b[1] - 22 * b[3];
    [
        t0 + t2,
        t1 + t3,
        t1 - t3,
        t0 - t2,
    ]
}

pub fn idct8x8(blk: &mut [i32; 64]) {
    // Row pass (no shift — values grow by ×8 nominal)
    for r in 0..8 {
        let o = r * 8;
        let mut row = [blk[o], blk[o+1], blk[o+2], blk[o+3],
                       blk[o+4], blk[o+5], blk[o+6], blk[o+7]];
        idct_row8(&mut row);
        blk[o..o+8].copy_from_slice(&row);
    }
    // Column pass + final >>7 rounding shift
    for c in 0..8 {
        let mut col = [
            blk[c],    blk[c+8],  blk[c+16], blk[c+24],
            blk[c+32], blk[c+40], blk[c+48], blk[c+56],
        ];
        idct_row8(&mut col);
        for r in 0..8 {
            blk[c + r*8] = (col[r] + 64) >> 7;
        }
    }
}

fn idct8x4(blk: &mut [i32; 64]) {
    // Row pass (8 wide, 4 high)
    for r in 0..4 {
        let o = r * 8;
        let mut row = [blk[o], blk[o+1], blk[o+2], blk[o+3],
                       blk[o+4], blk[o+5], blk[o+6], blk[o+7]];
        idct_row8(&mut row);
        blk[o..o+8].copy_from_slice(&row);
    }
    // Column pass (only 4 rows), with >>7
    for c in 0..8 {
        let col4 = [blk[c], blk[c+8], blk[c+16], blk[c+24]];
        let out = idct_row4(&col4);
        for r in 0..4 {
            blk[c + r*8] = (out[r] + 64) >> 7;
        }
        let _ = col4[0]; // suppress unused warning
    }
}

fn idct4x8(blk: &mut [i32; 64]) {
    // Row pass (only 4 wide)
    for r in 0..8 {
        let o = r * 8;
        let col4 = [blk[o], blk[o+1], blk[o+2], blk[o+3]];
        let out = idct_row4(&col4);
        for c in 0..4 { blk[o+c] = out[c]; }
    }
    // Column pass (8 rows), with >>7
    for c in 0..4 {
        let mut col = [
            blk[c],    blk[c+8],  blk[c+16], blk[c+24],
            blk[c+32], blk[c+40], blk[c+48], blk[c+56],
        ];
        idct_row8(&mut col);
        for r in 0..8 {
            blk[c + r*8] = (col[r] + 64) >> 7;
        }
    }
}

fn idct4x4(blk: &mut [i32; 64]) {
    // Row pass (4 wide)
    for r in 0..4 {
        let o = r * 8;
        let col4 = [blk[o], blk[o+1], blk[o+2], blk[o+3]];
        let out = idct_row4(&col4);
        for c in 0..4 { blk[o+c] = out[c]; }
    }
    // Column pass (4 rows), with >>7
    for c in 0..4 {
        let col4 = [blk[c], blk[c+8], blk[c+16], blk[c+24]];
        let out = idct_row4(&col4);
        for r in 0..4 {
            blk[c + r*8] = (out[r] + 64) >> 7;
        }
    }
}

/// Apply IDCT according to transform type.
/// tt: 0=8x8, 1=8x4_top, 2=8x4_bot, 3=4x8_left, 4=4x8_right, 5=4x4, 6=per_block
pub fn apply_idct(blk: &mut [i32; 64], tt: u8) {
    match tt {
        0 => idct8x8(blk),
        1 | 2 => idct8x4(blk),
        3 | 4 => idct4x8(blk),
        5 | 6 => idct4x4(blk),
        _ => idct8x8(blk),
    }
}

// ─── Inverse quantization ────────────────────────────────────────────────────
// SMPTE 421M §8.1.4.  Two modes: uniform and non-uniform.

fn iquant_uniform(level: i32, pquant: i32, halfqp: bool) -> i32 {
    if level == 0 { return 0; }
    let step = 2 * pquant;
    let base = step * level.abs() + pquant;
    let delta = if halfqp { pquant } else { 0 };
    let result = if level > 0 { base + delta } else { -(base + delta) };
    result.clamp(-2048, 2047)
}

fn iquant_nonuniform(level: i32, pquant: i32) -> i32 {
    if level == 0 { return 0; }
    // Non-uniform quantizer step table from SMPTE 421M Table 3
    const STEP: [i32; 32] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        17, 18, 19, 20, 21, 22, 23, 24, 25, 27, 29, 31, 33, 35, 37, 63,
    ];
    let step = STEP[(pquant as usize).min(31)];
    let result = step * level.abs() + pquant;
    if level > 0 { result.clamp(-2048, 2047) }
    else         { (-result).clamp(-2048, 2047) }
}

// ─── DC step-size tables (SMPTE 421M Table 3) ───────────────────────────────
// Indexed by pquant (0 unused, 1..31).
// Luma and chroma have separate tables.
// The value is multiplied by 128 here to match the IDCT normalization domain
// (IDCT output = input / 128, so DC_recon must be in the ×128 domain).

const DC_STEP_LUMA: [i32; 32] = [
    0,   // 0: unused
    128, 256, 384, 512, 640, 768, 896, 1024,  // pquant 1-8:  step = pquant
    1152, 1280, 1408, 1536, 1664, 1792, 1920, 2048, // 9-16
    2176, 2304, 2432, 2560, 2688, 2816, 2944, 3072, // 17-24
    3328, 3584, 3840, 4096, 4352, 4608, 8192,        // 25-31
];

const DC_STEP_CHROMA: [i32; 32] = [
    0,   // 0: unused
    128, 128, 128, 256, 256, 384, 384, 512,    // pquant 1-8
    512, 640, 640, 768, 768, 896, 896, 1024,   // 9-16
    1024, 1152, 1152, 1280, 1280, 1408, 1408, 1536, // 17-24
    1664, 1792, 1920, 2048, 2176, 2304, 4096,       // 25-31
];

#[inline]
fn dc_step(pquant: i32, is_luma: bool) -> i32 {
    let idx = pquant.clamp(1, 31) as usize;
    if is_luma { DC_STEP_LUMA[idx] } else { DC_STEP_CHROMA[idx] }
}


// ─── Loop filter (deblocking) ────────────────────────────────────────────────
// SMPTE 421M §8.6 — Simple/Main profile deblocking filter.
//
// Applied at every 8-pixel block boundary in the decoded frame.
// Modifies the two pixels straddling each boundary to reduce blocking artefacts.
//
//   d   = (p1 - 2*p2 + 2*p3 - p4 + 4) >> 3
//   d   = clamp(d, -p2, 255 - p3)
//   p2 += d;  p3 -= d

#[inline(always)]
fn lf_filter4(p: &mut [u8], a: usize, b: usize, c: usize, d: usize) {
    let p1 = p[a] as i32;
    let p2 = p[b] as i32;
    let p3 = p[c] as i32;
    let p4 = p[d] as i32;
    let mut delta = (p1 - 2*p2 + 2*p3 - p4 + 4) >> 3;
    delta = delta.clamp(-p2, 255 - p3);
    p[b] = (p2 + delta) as u8;
    p[c] = (p3 - delta) as u8;
}

/// Apply deblocking loop filter to one plane.
/// `stride`: number of pixels per row (= width for luma, width/2 for chroma).
/// `block_size`: 8 for luma, 8 for chroma (chroma plane is already half-size).
fn loop_filter_plane(plane: &mut Vec<u8>, stride: usize, height: usize) {
    let w = stride;
    let h = height;
    if w < 16 || h < 16 { return; } // nothing to filter

    // ── Vertical boundaries (filter horizontal rows) ───────────────────────
    // At column boundaries x = 8, 16, 24, ...
    for x in (8..w-1).step_by(8) {
        for y in 0..h {
            let base = y * w;
            // Pixels: x-2, x-1, x, x+1
            if x + 1 < w {
                lf_filter4(plane, base + x - 2, base + x - 1, base + x, base + x + 1);
            }
        }
    }

    // ── Horizontal boundaries (filter vertical columns) ────────────────────
    // At row boundaries y = 8, 16, 24, ...
    for y in (8..h-1).step_by(8) {
        for x in 0..w {
            // Pixels in column x at rows y-2, y-1, y, y+1
            let a = (y-2)*w + x;
            let b = (y-1)*w + x;
            let c =  y   *w + x;
            let d = (y+1)*w + x;
            lf_filter4(plane, a, b, c, d);
        }
    }
}

/// Apply loop filter to a decoded YUV frame (luma + both chroma planes).
pub fn apply_loop_filter(frame: &mut YuvFrame) {
    let w  = frame.width  as usize;
    let h  = frame.height as usize;
    let cw = (w + 1) / 2;
    let ch = (h + 1) / 2;
    loop_filter_plane(&mut frame.y,  w,  h);
    loop_filter_plane(&mut frame.cb, cw, ch);
    loop_filter_plane(&mut frame.cr, cw, ch);
}

// ─── Coefficient decoder ─────────────────────────────────────────────────────

/// Read raw DC differential (before prediction).
/// Returns the signed differential value (NOT yet scaled / predicted).
fn read_dc_diff(br: &mut BitReader<'_>, dc_vlc: &VlcTable) -> i32 {
    let dc_size = match dc_vlc.decode(br) {
        Some(s) if s >= 0 => s as u8,
        _ => return 0,
    };
    if dc_size == 0 { return 0; }
    let raw = br.read_bits(dc_size).unwrap_or(0) as i32;
    // MSB=0 → negative (one's complement offset per SMPTE 421M §8.1.4.4)
    if raw & (1 << (dc_size - 1)) != 0 { raw } else { raw - (1 << dc_size) + 1 }
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
    mb_w:  usize,
    /// Stored as reconstructed DC * 8 / pquant to stay in coeff domain.
    /// Layout: [mb_row * mb_w + mb_col][blk 0..6]
    dc: Vec<[i32; 6]>,
}

impl DcPredBuffer {
    pub fn new(mb_w: usize, mb_h: usize) -> Self {
        DcPredBuffer { mb_w, dc: vec![[1024i32; 6]; mb_w * mb_h] }
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
        let grad_h = (dc_left   - dc_topleft).unsigned_abs();
        let grad_v = (dc_top    - dc_topleft).unsigned_abs();

        if grad_v <= grad_h {
            // Predict from top (vertical predictor)
            (dc_top, false)
        } else {
            // Predict from left (horizontal predictor)
            (dc_left, true)
        }
    }

    fn dc_neighbours(&self, mb_row: usize, mb_col: usize, blk: usize)
        -> (i32, i32, i32)
    {
        // Helper: get stored DC for a possibly-out-of-bounds MB/blk
        let get = |r: isize, c: isize, b: usize| -> i32 {
            if r < 0 || c < 0 { return 1024; } // mid-gray default
            let idx = r as usize * self.mb_w + c as usize;
            if idx >= self.dc.len() { return 1024; }
            self.dc[idx][b]
        };

        let r = mb_row as isize;
        let c = mb_col as isize;

        match blk {
            // ── luma ───────────────────────────────────────────────────────
            0 => {
                let left     = get(r, c-1, 1); // right half of left MB
                let top      = get(r-1, c, 2); // bottom-left of top MB
                let topleft  = get(r-1, c-1, 3);
                (left, top, topleft)
            }
            1 => {
                // SMPTE 421M §8.1.4.6 Fig 8-4: topleft = blk3 of top-left MB
                let left     = get(r, c, 0);
                let top      = get(r-1, c, 3);
                let topleft  = get(r-1, c-1, 3);
                (left, top, topleft)
            }
            2 => {
                let left     = get(r, c-1, 3);
                let top      = get(r, c, 0);   // blk0 of same MB
                let topleft  = get(r, c-1, 1);
                (left, top, topleft)
            }
            3 => {
                let left     = get(r, c, 2);
                let top      = get(r, c, 1);
                let topleft  = get(r, c, 0);
                (left, top, topleft)
            }
            // ── chroma ─────────────────────────────────────────────────────
            _ => {
                let left    = get(r, c-1, blk);
                let top     = get(r-1, c, blk);
                let topleft = get(r-1, c-1, blk);
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
        Wmv2DcPredBuffer { mb_w, dc: vec![[1024i32; 6]; mb_w * mb_h] }
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
        fn get_luma(dc: &Vec<[i32; 6]>, mb_w: usize, bx: isize, by: isize) -> i32 {
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
        fn get_chroma(dc: &Vec<[i32; 6]>, mb_w: usize, mb_x: isize, mb_y: isize, blk: usize) -> i32 {
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
fn decode_escape_coeff(
    br:     &mut BitReader<'_>,
    ac_vlc: &VlcTable,
) -> (u8, i32, bool) {
    let mode = {
        let b0 = br.read_bit().unwrap_or(false);
        if !b0 { 1u8 } else {
            let b1 = br.read_bit().unwrap_or(false);
            if b1 { 3 } else { 2 }
        }
    };
    match mode {
        1 => {
            // Mode 1: level offset — VLC gives (run, base_level, last)
            let sym = ac_vlc.decode(br).unwrap_or(0);
            if sym == VLC_ESCAPE { return (0, 0, true); }
            let (run, base_level, last) = unpack_rl(sym);
            let sign   = br.read_bit().unwrap_or(false);
            let offset = ac_vlc.max_level(run as usize, last) as i32 + 1;
            let level  = base_level as i32 + offset;
            (run, if sign { -level } else { level }, last)
        }
        2 => {
            // Mode 2: run offset — VLC gives (base_run, level, last)
            let sym = ac_vlc.decode(br).unwrap_or(0);
            if sym == VLC_ESCAPE { return (0, 0, true); }
            let (base_run, level, last) = unpack_rl(sym);
            let sign   = br.read_bit().unwrap_or(false);
            let offset = ac_vlc.max_run(level as usize, last) as i32 + 1;
            let run    = (base_run as i32 + offset).min(63) as u8;
            let sl     = level as i32;
            (run, if sign { -sl } else { sl }, last)
        }
        _ => {
            // Mode 3: absolute — 1-bit LAST + 6-bit RUN + 8-bit |LEVEL| + 1-bit SIGN
            let last  = br.read_bit().unwrap_or(false);
            let run   = br.read_bits(6).unwrap_or(0) as u8;
            let level = br.read_bits(8).unwrap_or(1).max(1) as i32;
            let sign  = br.read_bit().unwrap_or(false);
            (run, if sign { -level } else { level }, last)
        }
    }
}

/// Decode one 8×8 block of AC+DC coefficients.
/// `is_intra`: use intra table / scan, otherwise inter.
/// `is_luma`:  use luma DC VLC.
/// Returns filled `[i32; 64]` in natural order (not zigzag).
fn decode_block(
    br:       &mut BitReader<'_>,
    is_intra: bool,
    is_luma:  bool,
    pquant:   i32,
    halfqp:   bool,
    uniform:  bool,
    tt:       u8,
    dc_luma:  &VlcTable,
    dc_chroma:&VlcTable,
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

    loop {
        let sym = match ac_vlc.decode(br) {
            Some(s) => s,
            None    => break,
        };

        let (run, signed_level, last) = if sym == VLC_ESCAPE {
            decode_escape_coeff(br, ac_vlc)
        } else {
            let (r, l, last) = unpack_rl(sym);
            let sign = br.read_bit().unwrap_or(false);
            (r, if sign { -(l as i32) } else { l as i32 }, last)
        };

        idx += run as usize;
        if idx >= 64 { break; }

        let mag   = signed_level.abs();
        let qval  = if uniform {
            iquant_uniform(mag, pquant, halfqp)
        } else {
            iquant_nonuniform(mag, pquant)
        };
        let signed_val = if signed_level < 0 { -qval } else { qval };

        // Use appropriate scan based on transform type
        let scan_order: &[usize; 64] = match tt {
            3 | 4 => &SCAN_VERT,
            _     => scan,
        };
        let pos = scan_order.get(idx).copied().unwrap_or(idx);
        blk[pos] = signed_val;
        idx += 1;

        if last || br.is_empty() { break; }
    }

    blk
}

/// Decode only the AC coefficients of one intra block (DC is handled separately).
fn decode_block_ac(
    br:       &mut BitReader<'_>,
    _is_luma: bool,
    pquant:   i32,
    halfqp:   bool,
    uniform:  bool,
    tt:       u8,
    ac_vlc:   &VlcTable,
) -> [i32; 64] {
    let mut blk   = [0i32; 64];
    let mut idx   = 1usize; // start at 1, skip DC slot

    loop {
        let sym = match ac_vlc.decode(br) {
            Some(s) => s,
            None    => break,
        };

        let (run, signed_level, last) = if sym == VLC_ESCAPE {
            decode_escape_coeff(br, ac_vlc)
        } else {
            let (r, l, last) = unpack_rl(sym);
            let sign = br.read_bit().unwrap_or(false);
            (r, if sign { -(l as i32) } else { l as i32 }, last)
        };

        idx += run as usize;
        if idx >= 64 { break; }

        let mag  = signed_level.abs();
        let qval = if uniform { iquant_uniform(mag, pquant, halfqp) }
                   else       { iquant_nonuniform(mag, pquant)       };
        let sval = if signed_level < 0 { -qval } else { qval };

        let scan_order: &[usize; 64] = match tt {
            3 | 4 => &SCAN_VERT,
            _     => &SCAN_INTRA,
        };
        let pos = scan_order.get(idx).copied().unwrap_or(idx);
        blk[pos] = sval;
        idx += 1;

        if last || br.is_empty() { break; }
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

    /// Get the AC predictor row (indices 1..7 of the reconstructed block).
    /// Returns the first row of the left neighbour (for horizontal prediction).
    pub fn pred_row(&self, mb_row: usize, mb_col: usize, blk: usize) -> [i32; 7] {
        let (src_mb_r, src_mb_c, src_blk) = Self::left_neighbour(mb_row, mb_col, blk);
        if src_mb_r as isize >= 0 && src_mb_c as isize >= 0 {
            let idx = src_mb_r * self.mb_w + src_mb_c;
            if idx < self.row.len() { return self.row[idx][src_blk]; }
        }
        [0i32; 7]
    }

    /// Get the AC predictor column (rows 1..7 of the reconstructed block).
    /// Returns the first column of the top neighbour (for vertical prediction).
    pub fn pred_col(&self, mb_row: usize, mb_col: usize, blk: usize) -> [i32; 7] {
        let (src_mb_r, src_mb_c, src_blk) = Self::top_neighbour(mb_row, mb_col, blk);
        if src_mb_r as isize >= 0 && src_mb_c as isize >= 0 {
            let idx = src_mb_r * self.mb_w + src_mb_c;
            if idx < self.col.len() { return self.col[idx][src_blk]; }
        }
        [0i32; 7]
    }

    pub fn store_row(&mut self, mb_row: usize, mb_col: usize, blk: usize, row: [i32; 7]) {
        let idx = mb_row * self.mb_w + mb_col;
        if idx < self.row.len() { self.row[idx][blk] = row; }
    }

    pub fn store_col(&mut self, mb_row: usize, mb_col: usize, blk: usize, col: [i32; 7]) {
        let idx = mb_row * self.mb_w + mb_col;
        if idx < self.col.len() { self.col[idx][blk] = col; }
    }

    /// Left neighbour source: same logic as DC prediction neighbour mapping.
    fn left_neighbour(mb_row: usize, mb_col: usize, blk: usize)
        -> (usize, usize, usize)
    {
        match blk {
            0 => (mb_row, mb_col.wrapping_sub(1), 1),
            1 => (mb_row, mb_col, 0),
            2 => (mb_row, mb_col.wrapping_sub(1), 3),
            3 => (mb_row, mb_col, 2),
            _ => (mb_row, mb_col.wrapping_sub(1), blk),
        }
    }

    fn top_neighbour(mb_row: usize, mb_col: usize, blk: usize)
        -> (usize, usize, usize)
    {
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
    mb_w:  usize,
    /// Stored MVs per MB: (mvx, mvy) in half-pixel units
    mvs:   Vec<(i32, i32)>,
    /// Whether each MB was skipped (skipped MBs propagate MV=0)
    skipped: Vec<bool>,
}

impl MvPredictor {
    pub fn new(mb_w: usize, mb_h: usize) -> Self {
        let n = mb_w * mb_h;
        MvPredictor { mb_w, mvs: vec![(0,0); n], skipped: vec![true; n] }
    }

    /// Compute the predicted MV for (mb_row, mb_col) from three neighbours.
    pub fn predict(&self, mb_row: usize, mb_col: usize) -> (i32, i32) {
        let get = |r: isize, c: isize| -> (i32, i32) {
            if r < 0 || c < 0 { return (0, 0); }
            let idx = r as usize * self.mb_w + c as usize;
            if idx >= self.mvs.len() || self.skipped[idx] { return (0, 0); }
            self.mvs[idx]
        };

        let r = mb_row as isize;
        let c = mb_col as isize;

        let (ax, ay) = get(r,   c-1);   // left
        let (bx, by) = get(r-1, c  );   // top
        let (cx, cy) = get(r-1, c+1);   // top-right (or top-left if rightmost)
        // If top-right is out of bounds, use top-left instead (per spec)
        let (cx, cy) = if c + 1 >= self.mb_w as isize {
            get(r-1, c-1)
        } else {
            (cx, cy)
        };

        (median3(ax, bx, cx), median3(ay, by, cy))
    }

    pub fn store(&mut self, mb_row: usize, mb_col: usize, mv: (i32, i32), skipped: bool) {
        let idx = mb_row * self.mb_w + mb_col;
        if idx < self.mvs.len() {
            self.mvs[idx]    = mv;
            self.skipped[idx] = skipped;
        }
    }
}

#[inline]
fn median3(a: i32, b: i32, c: i32) -> i32 {
    // Returns the median of three values
    if (a <= b && b <= c) || (c <= b && b <= a) { b }
    else if (b <= a && a <= c) || (c <= a && a <= b) { a }
    else { c }
}

#[inline(always)]
fn mid_pred(a: i32, b: i32, c: i32) -> i32 {
    // upstream mid_pred() helper.
    median3(a, b, c)
}

// ─── Overlap smoothing filter (Main profile) ─────────────────────────────────
// SMPTE 421M §6.2.1.  Applied at 8×8 block boundaries post-IDCT.

#[allow(dead_code)]
fn overlap_filter_h(a: &mut [i16], b: &mut [i16]) {
    // 4-tap filter across horizontal boundary between a[] and b[]
    // a holds last 4 samples of left block, b holds first 4 of right block
    for i in 0..4 {
        let x0 = a[i] as i32;
        let x1 = b[i] as i32;
        a[i] = ((9 * x0 + 3 * x1 + 8) >> 4) as i16;
        b[i] = ((3 * x0 + 9 * x1 + 8) >> 4) as i16;
    }
}

#[allow(dead_code)]
fn overlap_filter_v(a: &mut i16, b: &mut i16) {
    let x0 = *a as i32;
    let x1 = *b as i32;
    *a = ((9 * x0 + 3 * x1 + 8) >> 4) as i16;
    *b = ((3 * x0 + 9 * x1 + 8) >> 4) as i16;
}

pub fn apply_overlap_filter(frame: &mut YuvFrame) {
    let w  = frame.width  as usize;
    let h  = frame.height as usize;
    let cw = w / 2;
    let ch = h / 2;

    // Convert to i16 for filtering, then back to u8
    let mut y_i16: Vec<i16> = frame.y.iter().map(|&v| v as i16).collect();

    // Horizontal block boundaries (every 8 columns)
    for row in 0..h {
        for col in (8..w).step_by(8) {
            for k in 0..4 {
                let ia = row*w + col - 4 + k;
                let ib = row*w + col     + k;
                if ib < y_i16.len() {
                    let a = y_i16[ia] as i32;
                    let b = y_i16[ib] as i32;
                    y_i16[ia] = ((9*a + 3*b + 8) >> 4) as i16;
                    y_i16[ib] = ((3*a + 9*b + 8) >> 4) as i16;
                }
            }
        }
    }

    // Vertical block boundaries
    for row in (8..h).step_by(8) {
        for col in 0..w {
            let a = y_i16[(row-1)*w + col] as i32;
            let b = y_i16[row    *w + col] as i32;
            y_i16[(row-1)*w + col] = ((9*a + 3*b + 8) >> 4) as i16;
            y_i16[row    *w + col] = ((3*a + 9*b + 8) >> 4) as i16;
        }
    }

    // Write back
    for (dst, src) in frame.y.iter_mut().zip(y_i16.iter()) {
        *dst = (*src).clamp(0, 255) as u8;
    }

    // Chroma (same logic, half size)
    let mut cb_i16: Vec<i16> = frame.cb.iter().map(|&v| v as i16).collect();
    let mut cr_i16: Vec<i16> = frame.cr.iter().map(|&v| v as i16).collect();
    for plane in [&mut cb_i16, &mut cr_i16] {
        for row in 0..ch {
            for col in (8..cw).step_by(8) {
                for k in 0..4 {
                    let a = plane[row*cw + col - 4 + k] as i32;
                    let b = plane[row*cw + col     + k] as i32;
                    plane[row*cw + col - 4 + k] = ((9*a + 3*b + 8) >> 4) as i16;
                    plane[row*cw + col     + k] = ((3*a + 9*b + 8) >> 4) as i16;
                }
            }
        }
        for row in (8..ch).step_by(8) {
            for col in 0..cw {
                let a = plane[(row-1)*cw + col] as i32;
                let b = plane[row    *cw + col] as i32;
                plane[(row-1)*cw + col] = ((9*a + 3*b + 8) >> 4) as i16;
                plane[row    *cw + col] = ((3*a + 9*b + 8) >> 4) as i16;
            }
        }
    }
    for (dst, src) in frame.cb.iter_mut().zip(cb_i16.iter()) { *dst = (*src).clamp(0, 255) as u8; }
    for (dst, src) in frame.cr.iter_mut().zip(cr_i16.iter()) { *dst = (*src).clamp(0, 255) as u8; }
}

// ─── Motion compensation ─────────────────────────────────────────────────────
// Half-pixel bilinear interpolation per SMPTE 421M §7.3.

fn mc_luma(
    dst: &mut [u8], dst_stride: usize,
    src: &[u8],     src_stride: usize,
    src_w: usize, src_h: usize,
    x: i32, y: i32,
    w: usize, h: usize,
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

            let p00 = src[sy  * src_stride + sx ] as i32;
            let p10 = src[sy  * src_stride + sx1] as i32;
            let p01 = src[sy1 * src_stride + sx ] as i32;
            let p11 = src[sy1 * src_stride + sx1] as i32;

            let val = match (xh, yh) {
                (false, false) => p00,
                (true,  false) => (p00 + p10 + 1) >> 1,
                (false, true ) => (p00 + p01 + 1) >> 1,
                (true,  true ) => (p00 + p10 + p01 + p11 + 2) >> 2,
            };
            dst[dy * dst_stride + dx] = val.clamp(0, 255) as u8;
        }
    }
}

// ─── DQUANT: macroblock-level differential quantizer ────────────────────────
// SMPTE 421M §8.1.4.10 / §8.3.7.
//
// When seq.dquant != 0, each macroblock may override the frame-level PQUANT.
// dquant=1: 1-bit flag; if set, read 2-bit MQUANT (absolute value).
// dquant=2: always present 2-bit MQDIFF; if == 7, read 5-bit MQUANT absolute.

fn read_mquant(br: &mut BitReader<'_>, dquant: u8, pquant: i32) -> i32 {
    match dquant {
        0 => pquant, // no per-MB quant
        1 => {
            // 1-bit DQUANT flag; if 1, read 2-bit delta
            if br.read_bit().unwrap_or(false) {
                let mqdiff = br.read_bits(2).unwrap_or(0) as i32;
                // mqdiff: 0=+2, 1=-2, 2=+4, 3=-4 relative to pquant
                let delta = match mqdiff {
                    0 => 2, 1 => -2, 2 => 4, _ => -4,
                };
                (pquant + delta).clamp(1, 31)
            } else {
                pquant
            }
        }
        _ => {
            // dquant==2: always read 3-bit MQDIFF
            let mqdiff = br.read_bits(3).unwrap_or(0);
            if mqdiff == 7 {
                // escape: read 5-bit absolute MQUANT
                br.read_bits(5).unwrap_or(pquant as u32) as i32
            } else {
                // relative to pquant: +1..+6
                (pquant + mqdiff as i32).clamp(1, 31)
            }
        }
    }
}

// ─── Range Reduction / Expansion ─────────────────────────────────────────────
// SMPTE 421M §7.1.1.9.
//
// RANGEREDFRM=1 means the encoder reduced the dynamic range before coding.
// The decoder must expand it back.  Applied to the reconstructed frame.

pub fn apply_rangered_expand(frame: &mut YuvFrame) {
    // Expand: x' = (x - 128) * 2 + 128  (clamp 0..255)
    for p in frame.y.iter_mut() {
        *p = ((*p as i32 - 128) * 2 + 128).clamp(0, 255) as u8;
    }
    for p in frame.cb.iter_mut() {
        *p = ((*p as i32 - 128) * 2 + 128).clamp(0, 255) as u8;
    }
    for p in frame.cr.iter_mut() {
        *p = ((*p as i32 - 128) * 2 + 128).clamp(0, 255) as u8;
    }
}

/// Compress: applied to reference frame before motion compensation when
/// the current frame does NOT have RANGEREDFRM but the reference did.
pub fn apply_rangered_compress(frame: &mut YuvFrame) {
    for p in frame.y.iter_mut() {
        *p = ((*p as i32 - 128).div_euclid(2) + 128).clamp(0, 255) as u8;
    }
    for p in frame.cb.iter_mut() {
        *p = ((*p as i32 - 128).div_euclid(2) + 128).clamp(0, 255) as u8;
    }
    for p in frame.cr.iter_mut() {
        *p = ((*p as i32 - 128).div_euclid(2) + 128).clamp(0, 255) as u8;
    }
}

// ─── Write helpers ───────────────────────────────────────────────────────────

/// Block (mb_row, mb_col, blk_idx) → (plane ref, x, y, stride, plane_h)
fn block_coords(mb_row: u32, mb_col: u32, blk: usize, width: u32, height: u32)
    -> (bool, usize, usize, usize, usize)
{
    // Returns (is_luma, px, py, stride, plane_height)
    let (is_luma, bx, by) = match blk {
        0 => (true,  (mb_col*16) as usize,     (mb_row*16) as usize    ),
        1 => (true,  (mb_col*16+8) as usize,   (mb_row*16) as usize    ),
        2 => (true,  (mb_col*16) as usize,     (mb_row*16+8) as usize  ),
        3 => (true,  (mb_col*16+8) as usize,   (mb_row*16+8) as usize  ),
        _ => (false, (mb_col*8) as usize,      (mb_row*8) as usize     ),
    };
    let stride = if is_luma { width as usize } else { (width/2) as usize };
    let ph     = if is_luma { height as usize } else { (height/2) as usize };
    (is_luma, bx, by, stride, ph)
}

fn write_intra_block(frame: &mut YuvFrame, mb_row: u32, mb_col: u32, blk: usize,
                     coeff: &[i32; 64]) {
    let (is_luma, bx, by, stride, ph) =
        block_coords(mb_row, mb_col, blk, frame.width, frame.height);
    let plane: &mut Vec<u8> = if is_luma { &mut frame.y }
                               else if blk == 4 { &mut frame.cb }
                               else { &mut frame.cr };
    for r in 0..8 {
        if by + r >= ph { break; }
        for c in 0..8 {
            if bx + c >= stride { break; }
            let idx = (by + r) * stride + (bx + c);
            plane[idx] = (128 + coeff[r*8 + c]).clamp(0, 255) as u8;
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
fn write_intra_block_i16(frame: &mut YuvFrame, mb_row: u32, mb_col: u32, blk: usize, coeff: &[i16; 64]) {
    let (is_luma, bx, by, stride, ph) = block_coords(mb_row, mb_col, blk, frame.width, frame.height);
    let plane: &mut Vec<u8> = if is_luma { &mut frame.y } else if blk == 4 { &mut frame.cb } else { &mut frame.cr };
    for r in 0..8usize {
        if by + r >= ph { break; }
        for c in 0..8usize {
            if bx + c >= stride { break; }
            let idx = (by + r) * stride + (bx + c);
            let v = coeff[r * 8 + c] as i32;
            plane[idx] = (v + 128).clamp(0, 255) as u8;
        }
    }
}

fn add_residual_block_i16(frame: &mut YuvFrame, mb_row: u32, mb_col: u32, blk: usize, coeff: &[i16; 64]) {
    let (is_luma, bx, by, stride, ph) = block_coords(mb_row, mb_col, blk, frame.width, frame.height);
    let plane: &mut Vec<u8> = if is_luma { &mut frame.y } else if blk == 4 { &mut frame.cb } else { &mut frame.cr };
    for r in 0..8usize {
        if by + r >= ph { break; }
        for c in 0..8usize {
            if bx + c >= stride { break; }
            let idx = (by + r) * stride + (bx + c);
            let v = plane[idx] as i32 + coeff[r * 8 + c] as i32;
            plane[idx] = v.clamp(0, 255) as u8;
        }
    }
}

/// Motion compensate one 16×16 macroblock from `reference` into `dst`.
/// Motion vectors are in half-pel units (like H.263/MSMPEG4/WMV2).
fn motion_compensate_mb(
    dst:       &mut YuvFrame,
    reference: &YuvFrame,
    mb_row:    usize,
    mb_col:    usize,
    mvx:       i32,
    mvy:       i32,
) {
    let fw = dst.width as usize;
    let fh = dst.height as usize;
    if fw == 0 || fh == 0 { return; }
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
            if dst_y + r >= fh { break; }
            if dst_x >= fw { break; }
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
        mc_luma(&mut tmp_cb, 8, &reference.cb, cw, cw, ch, src_xc, src_yc, 8, 8);
        mc_luma(&mut tmp_cr, 8, &reference.cr, cw, cw, ch, src_xc, src_yc, 8, 8);

        for r in 0..8 {
            if dst_yc + r >= ch { break; }
            if dst_xc >= cw { break; }
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
    if v < 0 { 0 } else if v > 255 { 255 } else { v as u8 }
}

#[inline]
fn rnd_avg_u8(a: u8, b: u8) -> u8 {
    ((a as u16 + b as u16 + 1) >> 1) as u8
}

#[inline]
fn no_rnd_avg_u8(a: u8, b: u8) -> u8 {
    ((a as u16 + b as u16) >> 1) as u8
}

fn wmv2_mspel8_h_lowpass(dst: &mut [u8], dst_off: usize, dst_stride: usize,
                         src: &[u8], src_off: usize, src_stride: usize, h: usize) {
    for i in 0..h {
        let so = src_off + i * src_stride;
        let doff = dst_off + i * dst_stride;
        // dst[0..8]
        dst[doff + 0] = clip_u8(((9 * (src[so + 0] as i32 + src[so + 1] as i32)
            - (src[so - 1] as i32 + src[so + 2] as i32) + 8) >> 4));
        dst[doff + 1] = clip_u8(((9 * (src[so + 1] as i32 + src[so + 2] as i32)
            - (src[so + 0] as i32 + src[so + 3] as i32) + 8) >> 4));
        dst[doff + 2] = clip_u8(((9 * (src[so + 2] as i32 + src[so + 3] as i32)
            - (src[so + 1] as i32 + src[so + 4] as i32) + 8) >> 4));
        dst[doff + 3] = clip_u8(((9 * (src[so + 3] as i32 + src[so + 4] as i32)
            - (src[so + 2] as i32 + src[so + 5] as i32) + 8) >> 4));
        dst[doff + 4] = clip_u8(((9 * (src[so + 4] as i32 + src[so + 5] as i32)
            - (src[so + 3] as i32 + src[so + 6] as i32) + 8) >> 4));
        dst[doff + 5] = clip_u8(((9 * (src[so + 5] as i32 + src[so + 6] as i32)
            - (src[so + 4] as i32 + src[so + 7] as i32) + 8) >> 4));
        dst[doff + 6] = clip_u8(((9 * (src[so + 6] as i32 + src[so + 7] as i32)
            - (src[so + 5] as i32 + src[so + 8] as i32) + 8) >> 4));
        dst[doff + 7] = clip_u8(((9 * (src[so + 7] as i32 + src[so + 8] as i32)
            - (src[so + 6] as i32 + src[so + 9] as i32) + 8) >> 4));
    }
}

fn wmv2_mspel8_v_lowpass(dst: &mut [u8], dst_off: usize, dst_stride: usize,
                         src: &[u8], src_off: usize, src_stride: usize, w: usize) {
    for i in 0..w {
        let so = src_off + i;
        let s_1 = src[so - src_stride] as i32;
        let s0  = src[so] as i32;
        let s1  = src[so + src_stride] as i32;
        let s2  = src[so + 2 * src_stride] as i32;
        let s3  = src[so + 3 * src_stride] as i32;
        let s4  = src[so + 4 * src_stride] as i32;
        let s5  = src[so + 5 * src_stride] as i32;
        let s6  = src[so + 6 * src_stride] as i32;
        let s7  = src[so + 7 * src_stride] as i32;
        let s8  = src[so + 8 * src_stride] as i32;
        let s9  = src[so + 9 * src_stride] as i32;

        let do0 = dst_off + i + 0 * dst_stride;
        let do1 = dst_off + i + 1 * dst_stride;
        let do2 = dst_off + i + 2 * dst_stride;
        let do3 = dst_off + i + 3 * dst_stride;
        let do4 = dst_off + i + 4 * dst_stride;
        let do5 = dst_off + i + 5 * dst_stride;
        let do6 = dst_off + i + 6 * dst_stride;
        let do7 = dst_off + i + 7 * dst_stride;

        dst[do0] = clip_u8(((9 * (s0 + s1) - (s_1 + s2) + 8) >> 4));
        dst[do1] = clip_u8(((9 * (s1 + s2) - (s0  + s3) + 8) >> 4));
        dst[do2] = clip_u8(((9 * (s2 + s3) - (s1  + s4) + 8) >> 4));
        dst[do3] = clip_u8(((9 * (s3 + s4) - (s2  + s5) + 8) >> 4));
        dst[do4] = clip_u8(((9 * (s4 + s5) - (s3  + s6) + 8) >> 4));
        dst[do5] = clip_u8(((9 * (s5 + s6) - (s4  + s7) + 8) >> 4));
        dst[do6] = clip_u8(((9 * (s6 + s7) - (s5  + s8) + 8) >> 4));
        dst[do7] = clip_u8(((9 * (s7 + s8) - (s6  + s9) + 8) >> 4));
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
fn put_pixels8_l2_8_no_rnd(dst: &mut [u8], dst_off: usize,
                           src1: &[u8], src1_off: usize,
                           src2: &[u8], src2_off: usize,
                           dst_stride: usize, src1_stride: usize, src2_stride: usize,
                           h: usize) {
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
    put_pixels8_l2_8_no_rnd(dst, dst_off, src, src_off + 1, &half, 0, stride, stride, 8, 8);
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
fn wmv2_put_mspel_pixels(dxy: usize,
                         dst: &mut [u8], dst_off: usize,
                         src: &[u8], src_off: usize,
                         stride: usize) {
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

fn emulated_edge_mc(buf: &mut [u8], buf_stride: usize,
                    src: &[u8], src_stride: usize,
                    block_w: usize, block_h: usize,
                    src_x: i32, src_y: i32,
                    h_edge: usize, v_edge: usize) {
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
fn chroma_put_pixels(dst: &mut [u8], dst_off: usize,
                     src: &[u8], src_off: usize,
                     stride: usize, h: usize) {
    for y in 0..h {
        let d = dst_off + y * stride;
        let s = src_off + y * stride;
        dst[d..d + 8].copy_from_slice(&src[s..s + 8]);
    }
}

#[inline]
fn chroma_put_x2(dst: &mut [u8], dst_off: usize,
                 src: &[u8], src_off: usize,
                 stride: usize, h: usize) {
    for y in 0..h {
        let d = dst_off + y * stride;
        let s = src_off + y * stride;
        for x in 0..8 {
            dst[d + x] = rnd_avg_u8(src[s + x], src[s + x + 1]);
        }
    }
}

#[inline]
fn chroma_put_y2(dst: &mut [u8], dst_off: usize,
                 src: &[u8], src_off: usize,
                 stride: usize, h: usize) {
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
fn chroma_put_xy2(dst: &mut [u8], dst_off: usize,
                  src: &[u8], src_off: usize,
                  stride: usize, h: usize) {
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
    if fw == 0 || fh == 0 { return; }
    let cw = fw / 2;
    let ch = fh / 2;

    // ---- Luma ----
    let mut dxy = (((motion_y & 1) << 1) | (motion_x & 1)) as i32;
    dxy = 2 * dxy + hshift as i32;

    let mut src_x = mb_col as i32 * 16 + (motion_x >> 1);
    let mut src_y = mb_row as i32 * 16 + (motion_y >> 1);

    // clip to [-16, width] / [-16, height]
    if src_x < -16 { src_x = -16; }
    if src_x > dst.width as i32 { src_x = dst.width as i32; }
    if src_y < -16 { src_y = -16; }
    if src_y > dst.height as i32 { src_y = dst.height as i32; }

    if src_x <= -16 || src_x >= dst.width as i32 { dxy &= !3; }
    if src_y <= -16 || src_y >= dst.height as i32 { dxy &= !4; }

    let linesize = fw;
    let mut src_plane: &[u8] = &reference.y;
    let mut src_off: usize;

    // edge condition: same as upstream (using h_edge_pos=width, v_edge_pos=height)
    if src_x < 1 || src_y < 1 || src_x + 17 >= dst.width as i32 || src_y + 16 + 1 >= dst.height as i32 {
        let mut edge = vec![0u8; linesize * 19];
        emulated_edge_mc(
            &mut edge, linesize,
            &reference.y, linesize,
            19, 19,
            src_x - 1, src_y - 1,
            fw, fh,
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
        wmv2_put_mspel_pixels(dxyu, &mut dst.y, dst00 + 8 * linesize, src_plane, src00 + 8 * linesize, linesize);
        wmv2_put_mspel_pixels(dxyu, &mut dst.y, dst00 + 8 + 8 * linesize, src_plane, src00 + 8 + 8 * linesize, linesize);

        // ---- Chroma (still within edge scope) ----
        if dst.cb.is_empty() || reference.cb.is_empty() { return; }

        let mut cdxy = 0usize;
        if (motion_x & 3) != 0 { cdxy |= 1; }
        if (motion_y & 3) != 0 { cdxy |= 2; }
        let mx = motion_x >> 2;
        let my = motion_y >> 2;

        let mut csrc_x = mb_col as i32 * 8 + mx;
        let mut csrc_y = mb_row as i32 * 8 + my;

        if csrc_x < -8 { csrc_x = -8; }
        if csrc_x > (dst.width as i32 >> 1) { csrc_x = dst.width as i32 >> 1; }
        if csrc_y < -8 { csrc_y = -8; }
        if csrc_y > (dst.height as i32 >> 1) { csrc_y = dst.height as i32 >> 1; }

        if csrc_x == (dst.width as i32 >> 1) { cdxy &= !1; }
        if csrc_y == (dst.height as i32 >> 1) { cdxy &= !2; }

        let uvlinesize = cw;

        let mut edge_uv = vec![0u8; uvlinesize * 9];
        // cb
        emulated_edge_mc(
            &mut edge_uv, uvlinesize,
            &reference.cb, uvlinesize,
            9, 9,
            csrc_x, csrc_y,
            cw, ch,
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
            &mut edge_uv, uvlinesize,
            &reference.cr, uvlinesize,
            9, 9,
            csrc_x, csrc_y,
            cw, ch,
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
    wmv2_put_mspel_pixels(dxyu, &mut dst.y, dst00 + 8, src_plane, src_off + 8, linesize);
    wmv2_put_mspel_pixels(dxyu, &mut dst.y, dst00 + 8 * linesize, src_plane, src_off + 8 * linesize, linesize);
    wmv2_put_mspel_pixels(dxyu, &mut dst.y, dst00 + 8 + 8 * linesize, src_plane, src_off + 8 + 8 * linesize, linesize);

    // ---- Chroma ----
    if dst.cb.is_empty() || reference.cb.is_empty() { return; }

    let mut cdxy = 0usize;
    if (motion_x & 3) != 0 { cdxy |= 1; }
    if (motion_y & 3) != 0 { cdxy |= 2; }
    let mx = motion_x >> 2;
    let my = motion_y >> 2;

    let mut csrc_x = mb_col as i32 * 8 + mx;
    let mut csrc_y = mb_row as i32 * 8 + my;

    if csrc_x < -8 { csrc_x = -8; }
    if csrc_x > (dst.width as i32 >> 1) { csrc_x = dst.width as i32 >> 1; }
    if csrc_y < -8 { csrc_y = -8; }
    if csrc_y > (dst.height as i32 >> 1) { csrc_y = dst.height as i32 >> 1; }

    if csrc_x == (dst.width as i32 >> 1) { cdxy &= !1; }
    if csrc_y == (dst.height as i32 >> 1) { cdxy &= !2; }

    let uvlinesize = cw;
    let need_emu_uv = csrc_x < 0 || csrc_y < 0 || csrc_x + 9 >= cw as i32 || csrc_y + 9 >= ch as i32;
    if need_emu_uv {
        let mut edge_uv = vec![0u8; uvlinesize * 9];
        let dst_xc = mb_col * 8;
        let dst_yc = mb_row * 8;
        let dst_cb_off = dst_yc * uvlinesize + dst_xc;
        emulated_edge_mc(
            &mut edge_uv, uvlinesize,
            &reference.cb, uvlinesize,
            9, 9,
            csrc_x, csrc_y,
            cw, ch,
        );
        match cdxy {
            0 => chroma_put_pixels(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
            1 => chroma_put_x2(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
            2 => chroma_put_y2(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
            _ => chroma_put_xy2(&mut dst.cb, dst_cb_off, &edge_uv, 0, uvlinesize, 8),
        }

        let dst_cr_off = dst_yc * uvlinesize + dst_xc;
        emulated_edge_mc(
            &mut edge_uv, uvlinesize,
            &reference.cr, uvlinesize,
            9, 9,
            csrc_x, csrc_y,
            cw, ch,
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

fn add_residual_block(frame: &mut YuvFrame, mb_row: u32, mb_col: u32, blk: usize,
                      coeff: &[i32; 64]) {
    let (is_luma, bx, by, stride, ph) =
        block_coords(mb_row, mb_col, blk, frame.width, frame.height);
    let plane: &mut Vec<u8> = if is_luma { &mut frame.y }
                               else if blk == 4 { &mut frame.cb }
                               else { &mut frame.cr };
    for r in 0..8 {
        if by + r >= ph { break; }
        for c in 0..8 {
            if bx + c >= stride { break; }
            let idx = (by + r) * stride + (bx + c);
            plane[idx] = (plane[idx] as i32 + coeff[r*8 + c]).clamp(0, 255) as u8;
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
            let (start, end) = if last_flag == 0 { (0usize, base.last) } else { (base.last, base.n) };
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
        let (qmul, qadd) = if qscale == 0 { (1i32, 0i32) } else { (qscale * 2, (qscale - 1) | 1) };
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

pub struct MacroblockDecoder {
    pub width:     u32,
    pub height:    u32,
    pub width_mb:  u32,
    pub height_mb: u32,
    /// Reference frame for P/B decoding
    pub ref_frame: Option<YuvFrame>,
    // Lazily-built VLC tables
    dc_luma:    VlcTable,
    dc_chroma:  VlcTable,
    ac_inter:   [VlcTable; 4],
    ac_intra:   [VlcTable; 4],
    cbpcy_i:    VlcTable,
    cbpcy_p:    [VlcTable; 2],   // CBPTAB 0-1
    ttmb:       VlcTable,
    ttblk:      VlcTable,
    mv_vlc:     [VlcTable; 4],   // MVTAB 0-3
    dc_pred:    DcPredBuffer,
    mv_pred:    MvPredictor,
    // ── WMV2 VLC tables (built lazily; shared with VC-1 decode machinery) ─────
    wmv2_inter: [VlcTable; 2],  // ttcoef 0-1
    wmv2_intra: [VlcTable; 2],
    wmv2_cbpy:  VlcTable,
    wmv2_cbpc:  VlcTable,
    /// WMV2 reference frame (single-reference; no B-frame support)
    wmv2_ref:   Option<YuvFrame>,
    // ── WMV2/MSMPEG4 (upstream-aligned) VLCs / state ─────────────────────────
    wmv2_mb_i_vlc: VlcTree,
    wmv2_dc_vlc:   [[VlcTree; 2]; 2], // [dc_table_index][is_chroma]
    wmv2_coded_block: Vec<u8>,        // coded_block predictor grid (luma 8×8)
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
    /// Whether the last stored reference frame had RANGEREDFRM applied
    ref_rangeredfrm: bool,
    ac_pred:    AcPredBuffer,
    /// Forward reference (anchor before B-frames in display order)
    fwd_ref: Option<YuvFrame>,
    /// Backward reference (anchor after B-frames in display order)
    bwd_ref: Option<YuvFrame>,
}

impl MacroblockDecoder {
    pub fn new(width: u32, height: u32) -> Self {
        let mb_w = ((width  + 15) / 16) as usize;
        let mb_h = ((height + 15) / 16) as usize;

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
                let l = len.abs() as u8;
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
            width_mb:  (width  + 15) / 16,
            height_mb: (height + 15) / 16,
            ref_frame:  None,
            dc_luma:   dc_luma_vlc(),
            dc_chroma: dc_chroma_vlc(),
            ac_inter:  [
                inter_tcoef_vlc(0), inter_tcoef_vlc(1),
                inter_tcoef_vlc(2), inter_tcoef_vlc(3),
            ],
            ac_intra:  [
                intra_tcoef_vlc(0), intra_tcoef_vlc(1),
                intra_tcoef_vlc(2), intra_tcoef_vlc(3),
            ],
            cbpcy_i:   cbpcy_i_vlc(),
            cbpcy_p:   [cbpcy_p_vlc(0), cbpcy_p_vlc(1)],
            ttmb:      ttmb_vlc(),
            ttblk:     ttblk_vlc(),
            mv_vlc:    [
                mv_diff_vlc(0), mv_diff_vlc(1),
                mv_diff_vlc(2), mv_diff_vlc(3),
            ],
            dc_pred:   DcPredBuffer::new(mb_w, mb_h),
            mv_pred:   MvPredictor::new(mb_w, mb_h),
            ref_rangeredfrm: false,
            ac_pred: AcPredBuffer::new(mb_w, mb_h),
            fwd_ref: None,
            bwd_ref: None,
            wmv2_inter: [wmv2_tcoef_inter_vlc(0), wmv2_tcoef_inter_vlc(1)],
            wmv2_intra: [wmv2_tcoef_intra_vlc(0), wmv2_tcoef_intra_vlc(1)],
            wmv2_cbpy:  wmv2_cbpy_vlc(),
            wmv2_cbpc:  wmv2_cbpc_p_vlc(),
            wmv2_ref:   None,

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
        payload:  &[u8],
        pic_hdr:  &PictureHeader,
        seq:      &SequenceHeader,
        frame:    &mut YuvFrame,
    ) -> Result<()> {
        match pic_hdr.frame_type {
            FrameType::I | FrameType::BI => {
                self.decode_intra(payload, pic_hdr, seq, frame)?;
                if seq.overlap && pic_hdr.pquant >= 9 {
                    apply_overlap_filter(frame);
                }
                if seq.loop_filter { apply_loop_filter(frame); }
            }
            FrameType::P => {
                if seq.rangered {
                    let cur_rr   = pic_hdr.rangeredfrm;
                    let ref_rr   = self.ref_rangeredfrm;
                    if ref_rr && !cur_rr {
                        if let Some(ref mut rf) = self.ref_frame {
                            apply_rangered_compress(rf);
                        }
                    }
                }
                self.decode_p(payload, pic_hdr, seq, frame)?;
                if seq.loop_filter { apply_loop_filter(frame); }
            }
            FrameType::B => {
                self.decode_b(payload, pic_hdr, seq, frame)?;
                if seq.loop_filter { apply_loop_filter(frame); }
            }
            FrameType::Skipped => {
                if let Some(ref rf) = self.ref_frame {
                    frame.y.copy_from_slice(&rf.y);
                    frame.cb.copy_from_slice(&rf.cb);
                    frame.cr.copy_from_slice(&rf.cr);
                }
            }
        }

        // Post-decode: expand range if RANGEREDFRM
        if seq.rangered && pic_hdr.rangeredfrm {
            apply_rangered_expand(frame);
        }
        self.ref_rangeredfrm = pic_hdr.rangeredfrm;

        // Update reference frame chain.
        // Anchor frames (I/P) become forward reference for upcoming B-frames
        // and also get stored as the backward reference.
        match pic_hdr.frame_type {
            FrameType::B | FrameType::BI => {
                // B-frames don't update the anchor chain
            }
            _ => {
                // Current forward becomes previous, new frame becomes forward anchor
                self.fwd_ref = self.bwd_ref.take();
                self.bwd_ref = Some(frame.clone());
                self.ref_frame = Some(frame.clone());
            }
        }
        Ok(())
    }

    // ─── Intra frame ─────────────────────────────────────────────────────────

    fn decode_intra(
        &mut self,
        payload:  &[u8],
        pic_hdr:  &PictureHeader,
        seq:      &SequenceHeader,
        frame:    &mut YuvFrame,
    ) -> Result<()> {
        // The ASF payload includes the picture header and bitplanes.
        // Start macroblock decoding exactly at the macroblock layer.
        let mut br   = BitReader::new_at(payload, pic_hdr.header_bits);
        let pquant   = pic_hdr.pquant as i32;
        let halfqp   = pic_hdr.halfqp;
        let uniform  = seq.quantizer_mode != crate::vc1::QuantizerMode::NonUniform;

        // Reset DC and AC prediction buffers for this frame
        let mb_w = self.width_mb as usize;
        let mb_h = self.height_mb as usize;
        self.dc_pred = DcPredBuffer::new(mb_w, mb_h);
        self.ac_pred = AcPredBuffer::new(mb_w, mb_h);

        for mb_row in 0..self.height_mb {
            for mb_col in 0..self.width_mb {
                if br.is_empty() { return Ok(()); }

                // CBPCY: 6-bit coded-block pattern
                let cbp = self.cbpcy_i.decode(&mut br).unwrap_or(0) as u8;

                // Per-MB quantizer override (DQUANT)
                let mb_pquant = read_mquant(&mut br, seq.dquant, pquant);

                // Transform type for this MB
                let mb_tt = if seq.vstransform {
                    self.ttmb.decode(&mut br).unwrap_or(0) as u8
                } else { 0 };

                for blk in 0..6usize {
                    let is_luma = blk < 4;

                    // ── DC prediction (SMPTE 421M §8.1.4.6) ──────────────────
                    let dc_vlc   = if is_luma { &self.dc_luma } else { &self.dc_chroma };
                    let dc_diff  = read_dc_diff(&mut br, dc_vlc);

                    // DC step from SMPTE 421M Table 3 (×128 domain for IDCT)
                    let dc_scale = dc_step(mb_pquant, is_luma);
                    let (dc_pred_val, _use_left) =
                        self.dc_pred.predict(mb_row as usize, mb_col as usize, blk);

                    let dc_recon = dc_pred_val + dc_diff * dc_scale;
                    self.dc_pred.store(mb_row as usize, mb_col as usize, blk, dc_recon);

                    let coded = (cbp >> (5 - blk)) & 1 != 0;
                    let blk_tt = if coded && mb_tt == 6 {
                        self.ttblk.decode(&mut br).unwrap_or(0) as u8
                    } else { mb_tt };

                    let mut coeff = if coded {
                        decode_block_ac(
                            &mut br, is_luma,
                            mb_pquant, halfqp, uniform, blk_tt,
                            &self.ac_intra[seq.transacfrm2 as usize],
                        )
                    } else {
                        [0i32; 64]
                    };

                    // Place reconstructed DC into coeff[0]
                    coeff[0] = dc_recon;

                    // ── AC Prediction (SMPTE 421M §8.1.4.7) ──────────────────
                    // The direction follows the DC predictor choice:
                    // use_left=true  → horizontal: add pred_row to coeff[1..7]
                    // use_left=false → vertical:   add pred_col to coeff[8,16,..56]
                    if _use_left {
                        // Horizontal: predictor is first row of left neighbour
                        let pred = self.ac_pred.pred_row(mb_row as usize, mb_col as usize, blk);
                        for i in 0..7 { coeff[i + 1] += pred[i]; }
                        // Store our first row for future left→right neighbours
                        let our_row = [coeff[1], coeff[2], coeff[3], coeff[4],
                                       coeff[5], coeff[6], coeff[7]];
                        self.ac_pred.store_row(mb_row as usize, mb_col as usize, blk, our_row);
                    } else {
                        // Vertical: predictor is first column of top neighbour
                        let pred = self.ac_pred.pred_col(mb_row as usize, mb_col as usize, blk);
                        for i in 0..7 { coeff[(i + 1) * 8] += pred[i]; }
                        let our_col = [coeff[8], coeff[16], coeff[24], coeff[32],
                                       coeff[40], coeff[48], coeff[56]];
                        self.ac_pred.store_col(mb_row as usize, mb_col as usize, blk, our_col);
                    }

                    apply_idct(&mut coeff, blk_tt);
                    write_intra_block(frame, mb_row, mb_col, blk, &coeff);
                }
            }
        }
        Ok(())
    }

    // ─── P frame ─────────────────────────────────────────────────────────────

    fn decode_p(
        &mut self,
        payload:  &[u8],
        pic_hdr:  &PictureHeader,
        seq:      &SequenceHeader,
        frame:    &mut YuvFrame,
    ) -> Result<()> {
        // Start from reference frame copy
        if let Some(ref rf) = self.ref_frame {
            frame.y.copy_from_slice(&rf.y);
            frame.cb.copy_from_slice(&rf.cb);
            frame.cr.copy_from_slice(&rf.cr);
        }

        let mut br  = BitReader::new_at(payload, pic_hdr.header_bits);
        let pquant  = pic_hdr.pquant as i32;
        let halfqp  = pic_hdr.halfqp;
        let uniform = seq.quantizer_mode != crate::vc1::QuantizerMode::NonUniform;
        let mv_vlc = &self.mv_vlc[seq.mvtab as usize];

        // MV range (quarter-pel)
        let mv_scale = 1i32 << (pic_hdr.mvrange as i32 + 1);

        let ref_y   = self.ref_frame.as_ref().map(|f| f.y.clone())  .unwrap_or_default();
        let ref_cb  = self.ref_frame.as_ref().map(|f| f.cb.clone()) .unwrap_or_default();
        let ref_cr  = self.ref_frame.as_ref().map(|f| f.cr.clone()) .unwrap_or_default();
        let fw = self.width as usize;
        let fh = self.height as usize;

        // Reset MV predictor for this frame
        let mb_w = self.width_mb  as usize;
        let mb_h = self.height_mb as usize;
        self.mv_pred = MvPredictor::new(mb_w, mb_h);

        // Read skipped-MB bitplane from picture header
        let skip_plane = pic_hdr.skipmb_plane.clone().unwrap_or_default();

        for mb_row in 0..self.height_mb {
            for mb_col in 0..self.width_mb {
                if br.is_empty() { return Ok(()); }

                let mb_idx = mb_row as usize * mb_w + mb_col as usize;

                // Skipped macroblock (from bitplane or inline 1-bit flag)
                let skipped = if skip_plane.is_empty() {
                    br.read_bit().unwrap_or(false)
                } else {
                    skip_plane.get(mb_idx).copied().unwrap_or(0) != 0
                };

                if skipped {
                    // Use predicted MV for skipped MB (copy from reference)
                    let (pvx, pvy) = self.mv_pred.predict(mb_row as usize, mb_col as usize);
                    self.mv_pred.store(mb_row as usize, mb_col as usize, (pvx, pvy), true);
                    continue;
                }

                // Median MV predictor from A (left), B (top), C (top-right)
                let (pvx, pvy) = self.mv_pred.predict(mb_row as usize, mb_col as usize);

                // Motion vector differential
                let dmvx = Self::read_mv_diff(&mut br, mv_vlc, mv_scale);
                let dmvy = Self::read_mv_diff(&mut br, mv_vlc, mv_scale);
                let mvx  = pvx + dmvx;
                let mvy  = pvy + dmvy;

                // Store for future neighbours
                self.mv_pred.store(mb_row as usize, mb_col as usize, (mvx, mvy), false);

                // Luma motion compensation (16×16)
                {
                    let dst_x = (mb_col * 16) as usize;
                    let dst_y = (mb_row * 16) as usize;
                    let src_x = (dst_x as i32 * 2 + mvx) as i32; // half-pel
                    let src_y = (dst_y as i32 * 2 + mvy) as i32;
                    let mut tmp = [0u8; 256];
                    if ref_y.len() == fw * fh {
                        mc_luma(&mut tmp, 16, &ref_y, fw, fw, fh,
                                src_x, src_y, 16, 16);
                        let dst = &mut frame.y[dst_y * fw + dst_x..];
                        for r in 0..16 {
                            dst[r * fw..r * fw + 16].copy_from_slice(&tmp[r*16..r*16+16]);
                        }
                    }
                }

                // Chroma MC (8×8, MV /2 with optional FASTUVMC rounding)
                {
                    let cw = fw / 2;
                    let ch = fh / 2;
                    let dst_x = (mb_col * 8) as usize;
                    let dst_y = (mb_row * 8) as usize;
                    // Chroma MV is luma MV / 2 (half-pel chroma = quarter-pel luma)
                    let cmvx_raw = mvx / 2;
                    let cmvy_raw = mvy / 2;
                    // FASTUVMC: round chroma MVs so that fractional part is 0 or ½ pel
                    // i.e. strip the quarter-pel bit, rounding toward zero.
                    let (cmvx, cmvy) = if seq.fastuvmc {
                        // round: remove lowest half-pel bit, biased toward zero
                        let round = |v: i32| -> i32 {
                            if v >= 0 { v & !1 } else { -((-v) & !1) }
                        };
                        (round(cmvx_raw), round(cmvy_raw))
                    } else {
                        (cmvx_raw, cmvy_raw)
                    };
                    let src_x = (dst_x as i32 * 2 + cmvx) as i32;
                    let src_y = (dst_y as i32 * 2 + cmvy) as i32;
                    let mut tmp_cb = [0u8; 64];
                    let mut tmp_cr = [0u8; 64];
                    if ref_cb.len() == cw * ch {
                        mc_luma(&mut tmp_cb, 8, &ref_cb, cw, cw, ch, src_x, src_y, 8, 8);
                        mc_luma(&mut tmp_cr, 8, &ref_cr, cw, cw, ch, src_x, src_y, 8, 8);
                        let dcb = &mut frame.cb[dst_y * cw + dst_x..];
                        let dcr = &mut frame.cr[dst_y * cw + dst_x..];
                        for r in 0..8 {
                            dcb[r * cw..r * cw + 8].copy_from_slice(&tmp_cb[r*8..r*8+8]);
                            dcr[r * cw..r * cw + 8].copy_from_slice(&tmp_cr[r*8..r*8+8]);
                        }
                    }
                }

                // Residual (CBPCY + coefficients)
                let cbp = self.cbpcy_p[(seq.cbptab as usize).min(1)].decode(&mut br).unwrap_or(0) as u8;
                // Per-MB quantizer (DQUANT)
                let mb_pquant = read_mquant(&mut br, seq.dquant, pquant);
                let mb_tt = if seq.vstransform {
                    self.ttmb.decode(&mut br).unwrap_or(0) as u8
                } else { 0 };

                for blk in 0..6usize {
                    if (cbp >> (5 - blk)) & 1 == 0 { continue; }
                    let is_luma = blk < 4;
                    let blk_tt = if mb_tt == 6 {
                        self.ttblk.decode(&mut br).unwrap_or(0) as u8
                    } else { mb_tt };

                    let mut coeff = decode_block(
                        &mut br, false, is_luma,
                        mb_pquant, halfqp, uniform, blk_tt,
                        &self.dc_luma, &self.dc_chroma,
                        &self.ac_intra[seq.transacfrm2 as usize],
                        &self.ac_inter[seq.transacfrm  as usize],
                    );
                    apply_idct(&mut coeff, blk_tt);
                    add_residual_block(frame, mb_row as u32, mb_col as u32, blk, &coeff);
                }
            }
        }
        Ok(())
    }

    // ─── B frame ─────────────────────────────────────────────────────────────
    // SMPTE 421M §8.4.
    // Each MB can be: direct (interpolated from fwd+bwd), forward, backward,
    // bidirectional, or intra.  We decode the MB type and both MV differentials
    // then blend fwd and bwd MC results.

    fn decode_b(
        &mut self,
        payload:  &[u8],
        pic_hdr:  &PictureHeader,
        seq:      &SequenceHeader,
        frame:    &mut YuvFrame,
    ) -> Result<()> {
        let fwd = match &self.fwd_ref {
            Some(f) => f.clone(),
            None    => return Ok(()), // no anchor yet
        };
        let bwd = match &self.bwd_ref {
            Some(f) => f.clone(),
            None    => return Ok(()), // no backward anchor
        };

        let mut br  = BitReader::new(payload);
        let pquant  = pic_hdr.pquant as i32;
        let halfqp  = pic_hdr.halfqp;
        let uniform = seq.quantizer_mode != crate::vc1::QuantizerMode::NonUniform;
        let mv_vlc = &self.mv_vlc[seq.mvtab as usize];
        let mv_scale = 1i32 << (pic_hdr.mvrange as i32 + 1);

        let fw = self.width  as usize;
        let fh = self.height as usize;
        let mb_w = self.width_mb  as usize;
        let mb_h = self.height_mb as usize;
        self.mv_pred = MvPredictor::new(mb_w, mb_h);

        // Direct-mode and skip bitplanes
        let direct_plane = pic_hdr.directmb_plane.clone().unwrap_or_default();
        let skip_plane   = pic_hdr.skipmb_plane.clone().unwrap_or_default();

        // Temporal MV scaling from BFRACTION (SMPTE 421M §8.4.1.3).
        let direct_scale_num = pic_hdr.bfrac_num;
        let direct_scale_den = pic_hdr.bfrac_den;

        for mb_row in 0..self.height_mb {
            for mb_col in 0..self.width_mb {
                if br.is_empty() { return Ok(()); }

                let mb_idx = mb_row as usize * mb_w + mb_col as usize;
                let is_direct = direct_plane.get(mb_idx).copied().unwrap_or(0) != 0;
                let is_skip   = skip_plane.get(mb_idx).copied().unwrap_or(0) != 0;

                if is_skip || is_direct {
                    // Direct / skip: interpolate fwd + bwd with equal weight
                    let (pvx, pvy) = self.mv_pred.predict(mb_row as usize, mb_col as usize);
                    // Scale fwd MV and derive bwd MV
                    let fvx = pvx * direct_scale_num / direct_scale_den;
                    let fvy = pvy * direct_scale_num / direct_scale_den;
                    let bvx = fvx - pvx;
                    let bvy = fvy - pvy;

                    self.mc_blend_mb(frame, mb_row, mb_col, &fwd, &bwd,
                                     fvx, fvy, bvx, bvy, fw, fh);
                    self.mv_pred.store(mb_row as usize, mb_col as usize, (pvx, pvy), is_skip);
                    continue;
                }

                // Read MB type: 2 bits
                // 00=intra, 01=backward, 10=forward, 11=bidirectional
                let mb_type = br.read_bits(2).unwrap_or(2);

                if mb_type == 0 {
                    // Intra MB in B-frame (rare)
                    let cbp = self.cbpcy_i.decode(&mut br).unwrap_or(0) as u8;
                    let mb_pquant = read_mquant(&mut br, seq.dquant, pquant);
                    let mb_tt = if seq.vstransform {
                        self.ttmb.decode(&mut br).unwrap_or(0) as u8
                    } else { 0 };
                    for blk in 0..6usize {
                        let coded = (cbp >> (5 - blk)) & 1 != 0;
                        if coded {
                            let blk_tt = if mb_tt == 6 {
                                self.ttblk.decode(&mut br).unwrap_or(0) as u8
                            } else { mb_tt };
                            let mut coeff = decode_block(
                                &mut br, true, blk < 4,
                                mb_pquant, halfqp, uniform, blk_tt,
                                &self.dc_luma, &self.dc_chroma,
                                &self.ac_intra[seq.transacfrm2 as usize],
                                &self.ac_inter[seq.transacfrm  as usize],
                            );
                            apply_idct(&mut coeff, blk_tt);
                            write_intra_block(frame, mb_row, mb_col, blk, &coeff);
                        }
                    }
                    self.mv_pred.store(mb_row as usize, mb_col as usize, (0,0), false);
                    continue;
                }

                // Motion vectors
                let use_fwd = mb_type == 2 || mb_type == 3;
                let use_bwd = mb_type == 1 || mb_type == 3;

                let (pvx, pvy) = self.mv_pred.predict(mb_row as usize, mb_col as usize);

                let (fvx, fvy) = if use_fwd {
                    let dx = Self::read_mv_diff(&mut br, mv_vlc, mv_scale);
                    let dy = Self::read_mv_diff(&mut br, mv_vlc, mv_scale);
                    (pvx + dx, pvy + dy)
                } else { (0, 0) };

                let (bvx, bvy) = if use_bwd {
                    let dx = Self::read_mv_diff(&mut br, mv_vlc, mv_scale);
                    let dy = Self::read_mv_diff(&mut br, mv_vlc, mv_scale);
                    (pvx + dx, pvy + dy)
                } else { (0, 0) };

                self.mv_pred.store(mb_row as usize, mb_col as usize,
                                   if use_fwd { (fvx, fvy) } else { (bvx, bvy) }, false);

                if use_fwd && use_bwd {
                    self.mc_blend_mb(frame, mb_row, mb_col, &fwd, &bwd,
                                     fvx, fvy, bvx, bvy, fw, fh);
                } else if use_fwd {
                    self.mc_single_mb(frame, mb_row, mb_col, &fwd, fvx, fvy, fw, fh);
                } else {
                    self.mc_single_mb(frame, mb_row, mb_col, &bwd, bvx, bvy, fw, fh);
                }

                // Residual
                let cbp = self.cbpcy_p[(seq.cbptab as usize).min(1)].decode(&mut br).unwrap_or(0) as u8;
                let mb_pquant = read_mquant(&mut br, seq.dquant, pquant);
                let mb_tt = if seq.vstransform {
                    self.ttmb.decode(&mut br).unwrap_or(0) as u8
                } else { 0 };
                for blk in 0..6usize {
                    if (cbp >> (5 - blk)) & 1 == 0 { continue; }
                    let blk_tt = if mb_tt == 6 {
                        self.ttblk.decode(&mut br).unwrap_or(0) as u8
                    } else { mb_tt };
                    let mut coeff = decode_block(
                        &mut br, false, blk < 4,
                        mb_pquant, halfqp, uniform, blk_tt,
                        &self.dc_luma, &self.dc_chroma,
                        &self.ac_intra[seq.transacfrm2 as usize],
                        &self.ac_inter[seq.transacfrm  as usize],
                    );
                    apply_idct(&mut coeff, blk_tt);
                    add_residual_block(frame, mb_row as u32, mb_col as u32, blk, &coeff);
                }
            }
        }
        Ok(())
    }

    /// Copy one 16×16 luma + 8×8 chroma macroblock from ref with given MV.
    fn mc_single_mb(&self, frame: &mut YuvFrame, mb_row: u32, mb_col: u32,
                    refp: &YuvFrame, mvx: i32, mvy: i32, fw: usize, fh: usize) {
        let dst_x = (mb_col * 16) as usize;
        let dst_y = (mb_row * 16) as usize;
        let src_x = dst_x as i32 * 2 + mvx;
        let src_y = dst_y as i32 * 2 + mvy;
        let mut tmp = [0u8; 256];
        mc_luma(&mut tmp, 16, &refp.y, fw, fw, fh, src_x, src_y, 16, 16);
        let dst = &mut frame.y[dst_y * fw + dst_x..];
        for r in 0..16 { dst[r * fw..r * fw + 16].copy_from_slice(&tmp[r*16..r*16+16]); }

        let cw = fw / 2;
        let ch = fh / 2;
        let cdst_x = (mb_col * 8) as usize;
        let cdst_y = (mb_row * 8) as usize;
        let csrc_x = cdst_x as i32 * 2 + mvx / 2;
        let csrc_y = cdst_y as i32 * 2 + mvy / 2;
        let mut tmp_cb = [0u8; 64];
        let mut tmp_cr = [0u8; 64];
        mc_luma(&mut tmp_cb, 8, &refp.cb, cw, cw, ch, csrc_x, csrc_y, 8, 8);
        mc_luma(&mut tmp_cr, 8, &refp.cr, cw, cw, ch, csrc_x, csrc_y, 8, 8);
        let dcb = &mut frame.cb[cdst_y * cw + cdst_x..];
        let dcr = &mut frame.cr[cdst_y * cw + cdst_x..];
        for r in 0..8 {
            dcb[r * cw..r * cw + 8].copy_from_slice(&tmp_cb[r*8..r*8+8]);
            dcr[r * cw..r * cw + 8].copy_from_slice(&tmp_cr[r*8..r*8+8]);
        }
    }

    /// Bidirectional blend: average of forward and backward MC.
    fn mc_blend_mb(&self, frame: &mut YuvFrame, mb_row: u32, mb_col: u32,
                   fwd: &YuvFrame, bwd: &YuvFrame,
                   fvx: i32, fvy: i32, bvx: i32, bvy: i32,
                   fw: usize, fh: usize) {
        let dst_x  = (mb_col * 16) as usize;
        let dst_y  = (mb_row * 16) as usize;
        let fsrc_x = dst_x as i32 * 2 + fvx;
        let fsrc_y = dst_y as i32 * 2 + fvy;
        let bsrc_x = dst_x as i32 * 2 + bvx;
        let bsrc_y = dst_y as i32 * 2 + bvy;

        let mut ftmp = [0u8; 256];
        let mut btmp = [0u8; 256];
        mc_luma(&mut ftmp, 16, &fwd.y, fw, fw, fh, fsrc_x, fsrc_y, 16, 16);
        mc_luma(&mut btmp, 16, &bwd.y, fw, fw, fh, bsrc_x, bsrc_y, 16, 16);
        let dst = &mut frame.y[dst_y * fw + dst_x..];
        for r in 0..16 {
            for c in 0..16 {
                dst[r * fw + c] = ((ftmp[r*16+c] as u16 + btmp[r*16+c] as u16 + 1) >> 1) as u8;
            }
        }

        let cw = fw / 2;
        let ch = fh / 2;
        let cdst_x  = (mb_col * 8) as usize;
        let cdst_y  = (mb_row * 8) as usize;
        let cfsrc_x = cdst_x as i32 * 2 + fvx / 2;
        let cfsrc_y = cdst_y as i32 * 2 + fvy / 2;
        let cbsrc_x = cdst_x as i32 * 2 + bvx / 2;
        let cbsrc_y = cdst_y as i32 * 2 + bvy / 2;
        let mut fcb = [0u8; 64]; let mut fcrb = [0u8; 64];
        let mut bcb = [0u8; 64]; let mut bcrb = [0u8; 64];
        mc_luma(&mut fcb,  8, &fwd.cb, cw, cw, ch, cfsrc_x, cfsrc_y, 8, 8);
        mc_luma(&mut fcrb, 8, &fwd.cr, cw, cw, ch, cfsrc_x, cfsrc_y, 8, 8);
        mc_luma(&mut bcb,  8, &bwd.cb, cw, cw, ch, cbsrc_x, cbsrc_y, 8, 8);
        mc_luma(&mut bcrb, 8, &bwd.cr, cw, cw, ch, cbsrc_x, cbsrc_y, 8, 8);
        let dcb = &mut frame.cb[cdst_y * cw + cdst_x..];
        let dcr = &mut frame.cr[cdst_y * cw + cdst_x..];
        for r in 0..8 {
            for c in 0..8 {
                dcb[r * cw + c] = ((fcb[r*8+c]  as u16 + bcb[r*8+c]  as u16 + 1) >> 1) as u8;
                dcr[r * cw + c] = ((fcrb[r*8+c] as u16 + bcrb[r*8+c] as u16 + 1) >> 1) as u8;
            }
        }
    }

        fn read_mv_diff(br: &mut BitReader<'_>, mv_vlc: &VlcTable, scale: i32) -> i32 {
        let sym = mv_vlc.decode(br).unwrap_or(0);
        if sym == i32::MIN {
            // Fixed-length escape
            br.read_bits_signed(17).unwrap_or(0)
        } else {
            sym * scale / 4  // convert to quarter-pel
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
        0,
        8, 8, 8, 8, 8, 9, 9,
        10, 10, 11, 11, 12, 12, 13, 13,
        14, 14, 15, 15, 16, 16, 17, 17,
        18, 18, 19, 19, 20, 20, 21, 21,
    ];
    const C: [i32; 32] = [
        0,
        8, 8, 8, 8, 9, 9, 10,
        10, 11, 11, 12, 12, 13, 13, 14,
        14, 15, 15, 16, 16, 17, 17, 18,
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
    const MAP: [[u8; 3]; 3] = [
        [0, 2, 1],
        [1, 0, 2],
        [2, 1, 0],
    ];
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
        let Some(r) = self.wmv2_ref.as_ref() else { return false; };
        if out.width != r.width || out.height != r.height {
            *out = r.clone();
            return true;
        }
        if out.y.len() == r.y.len() { out.y.copy_from_slice(&r.y); } else { out.y = r.y.clone(); }
        if out.cb.len() == r.cb.len() { out.cb.copy_from_slice(&r.cb); } else { out.cb = r.cb.clone(); }
        if out.cr.len() == r.cr.len() { out.cr.copy_from_slice(&r.cr); } else { out.cr = r.cr.clone(); }
        true
    }

    pub fn decode_wmv2_frame(
        &mut self,
        payload: &[u8],
        hdr:     &Wmv2FrameHeader,
        params:  &Wmv2Params,
        frame:   &mut YuvFrame,
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
        let j_type = if self.wmv2_j_type_bit { br.read_bit().unwrap_or(false) } else { false };
        if j_type { return 1; }
        let per_mb_rl_table = if self.wmv2_per_mb_rl_bit { br.read_bit().unwrap_or(false) } else { false };
        if !per_mb_rl_table {
            let _ = decode012(&mut br);
            let _ = decode012(&mut br);
        }
        let dc_table_index = br.read_bit().unwrap_or(false) as usize;
        let code = match self.wmv2_mb_i_vlc.decode(&mut br) { Some(v) => v as u32, None => return 0 };
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
            let mut level = match tbl.decode(&mut br) { Some(v) => v, None => return 0 };
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
                if all { true } else { br.read_bit().unwrap_or(false) }
            }
            3 => {
                let all = br.read_bit().unwrap_or(false);
                if all { true } else { br.read_bit().unwrap_or(false) }
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

        let _mspel = if self.wmv2_mspel_bit { br.read_bit().unwrap_or(false) } else { false };
        if self.wmv2_abt_flag {
            let per_mb_abt = br.read_bit().unwrap_or(false) ^ true;
            if !per_mb_abt {
                let _ = decode012(&mut br);
            }
        }
        let per_mb_rl_table = if self.wmv2_per_mb_rl_bit { br.read_bit().unwrap_or(false) } else { false };
        if !per_mb_rl_table {
            let _ = decode012(&mut br);
        }
        let dc_table_index = br.read_bit().unwrap_or(false) as usize;
        let mv_table_index = br.read_bit().unwrap_or(false) as usize;

        if first_skip {
            return 1;
        }

        let code = match self.wmv2_mb_non_intra_vlc[cbp_table_index.min(3)].decode(&mut br) {
            Some(v) => v as i32,
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
            let mut level = match tbl.decode(&mut br) { Some(v) => v, None => return 0 };
            if level == DC_MAX {
                let _ = br.read_bits(8);
                let _ = br.read_bit();
            } else if level != 0 {
                let _ = br.read_bit();
            }
        } else {
            // Decode one MV symbol.
            let tbl = &self.wmv2_mv_vlc[mv_table_index.min(1)];
            let sym = match tbl.decode(&mut br) { Some(v) => v as u16, None => return 0 };
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
        if br.is_empty() { break; }

        let cbpc_sym = match self.wmv2_cbpc.decode(&mut br) {
            Some(v) => v,
            None => break,
        };
        if cbpc_sym == -1 {
            score += 1;
            continue;
        }
        if cbpc_sym < 0 || cbpc_sym > 3 {
            break;
        }

        let is_intra = match br.read_bit() {
            Some(b) => b,
            None => break,
        };

        let cbpy_raw = match self.wmv2_cbpy.decode(&mut br) {
            Some(v) if v >= 0 && v <= 15 => v as u8,
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
        let a = if bx > 0 { self.wmv2_coded_block[idx - 1] } else { 0 };
        let b = if bx > 0 && by > 0 { self.wmv2_coded_block[idx - 1 - bw] } else { 0 };
        let c = if by > 0 { self.wmv2_coded_block[idx - bw] } else { 0 };
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
            if sign { level = -level; }
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
        for i in 1..8usize {
            cur[8 + i] = block[i];
        }
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
        self.wmv2_dc_pred.store(mb_row, mb_col, blk, level * dc_scale);

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
                        let (lvl2, run2) = rl
                            .decode_sym(br, 0)
                            .ok_or_else(|| DecoderError::InvalidData("WMV2: escape1 VLC underrun".into()))?;
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
                            let (lvl2, run2) = rl
                                .decode_sym(br, 0)
                                .ok_or_else(|| DecoderError::InvalidData("WMV2: escape2 VLC underrun".into()))?;
                            level_uq = lvl2;
                            run = run2;
                            let last = ((run >> 7) & 1) as usize;
                            let base_level = (level_uq / qmul).abs() as usize;
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
                                self.wmv2_esc3_run_length = (br.read_bits(2).unwrap_or(0) as u8) + 3;
                            }
                            let run_abs = br.read_bits(self.wmv2_esc3_run_length).unwrap_or(0) as i32;
                            let sign = br.read_bit().unwrap_or(false);
                            let mut lvl_abs = br.read_bits(self.wmv2_esc3_level_length).unwrap_or(0) as i32;
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
                        return Err(DecoderError::InvalidData("WMV2: negative coeff index (bitstream damaged)".into()));
                    }
                    let pos = scan[i as usize] as usize;
                    if pos < 64 {
                        block[pos] = level_uq as i16;
                    }
                    break;
                }

                if i < 0 {
                    return Err(DecoderError::InvalidData("WMV2: negative coeff index (bitstream damaged)".into()));
                }
                let pos = scan[i as usize] as usize;
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
            let (mut level_uq, mut run) = rl
                .decode_sym(br, qscale)
                .ok_or_else(|| DecoderError::InvalidData("WMV2: inter tcoeff VLC underrun".into()))?;

            if level_uq == 0 {
                // escape
                let b0 = br.peek_bits(1).unwrap_or(0);
                if b0 == 1 {
                    // escape1
                    br.skip_bits(1);
                    let (lvl2, run2) = rl
                        .decode_sym(br, qscale)
                        .ok_or_else(|| DecoderError::InvalidData("WMV2: inter escape1 VLC underrun".into()))?;
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
                        let (lvl2, run2) = rl
                            .decode_sym(br, qscale)
                            .ok_or_else(|| DecoderError::InvalidData("WMV2: inter escape2 VLC underrun".into()))?;
                        level_uq = lvl2;
                        run = run2;
                        let last = ((run >> 7) & 1) as usize;
                        let base_level = (level_uq / qmul).abs() as usize;
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
                        let mut lvl_abs = br.read_bits(self.wmv2_esc3_level_length).unwrap_or(0) as i32;
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
                    return Err(DecoderError::InvalidData("WMV2: negative coeff index (bitstream damaged)".into()));
                }
                let pos = scan[i as usize] as usize;
                if pos < 64 {
                    block[pos] = level_uq as i16;
                }
                break;
            }

            if i < 0 {
                return Err(DecoderError::InvalidData("WMV2: negative coeff index (bitstream damaged)".into()));
            }
            let pos = scan[i as usize] as usize;
            if pos < 64 {
                block[pos] = level_uq as i16;
            }
        }

        let _ = blk;
        Ok(block)
    }
    fn wmv2_parse_mb_skip(&mut self, br: &mut BitReader<'_>, mb_w: usize, mb_h: usize) -> Result<()> {
        // upstream wmv2dec.c parse_mb_skip
        let skip_type = br.read_bits(2).ok_or_else(|| DecoderError::InvalidData("WMV2: missing skip_type".into()))? as u8;
        self.wmv2_skip_type = skip_type;
        if self.wmv2_mb_skip.len() != mb_w * mb_h {
            self.wmv2_mb_skip.resize(mb_w * mb_h, false);
        }
        for v in self.wmv2_mb_skip.iter_mut() { *v = false; }

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
                    let all = br.read_bit().ok_or_else(|| DecoderError::InvalidData("WMV2: skip row flag missing".into()))?;
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
                    let all = br.read_bit().ok_or_else(|| DecoderError::InvalidData("WMV2: skip col flag missing".into()))?;
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
            return Err(DecoderError::InvalidData("WMV2: coded MB count exceeds remaining bits".into()));
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
        if idx < self.wmv2_motion.len() { self.wmv2_motion[idx] } else { (0, 0) }
    }

    #[inline(always)]
    fn wmv2_motion_set(&mut self, mb_row: usize, mb_col: usize, mv: (i32, i32)) {
        let mb_w = self.width_mb as usize;
        let idx = mb_row * mb_w + mb_col;
        if self.wmv2_motion.len() != mb_w * (self.height_mb as usize) {
            self.wmv2_motion.resize(mb_w * (self.height_mb as usize), (0, 0));
        }
        if idx < self.wmv2_motion.len() {
            self.wmv2_motion[idx] = mv;
        }
    }

    #[inline(always)]
    fn wmv2_pred_motion(&self, br: &mut BitReader<'_>, mb_row: usize, mb_col: usize, first_slice_line: bool) -> (i32, i32) {
        // upstream wmv2dec.c wmv2_pred_motion (MB-level approximation).
        let a = self.wmv2_motion_get(mb_row as isize, mb_col as isize - 1);
        let b = self.wmv2_motion_get(mb_row as isize - 1, mb_col as isize);
        let c = self.wmv2_motion_get(mb_row as isize - 1, mb_col as isize + 1);

        let diff = if mb_col != 0 && !first_slice_line && !self.wmv2_mspel && self.wmv2_top_left_mv_flag {
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
            (br.read_bits(6).unwrap_or(0) as i32, br.read_bits(6).unwrap_or(0) as i32)
        };

        mx += pred.0 - 32;
        my += pred.1 - 32;
        // WARNING: they do not do exactly modulo encoding.
        if mx <= -64 { mx += 64; } else if mx >= 64 { mx -= 64; }
        if my <= -64 { my += 64; } else if my >= 64 { my -= 64; }
        (mx, my)
    }

    // ── WMV2 I-frame ──────────────────────────────────────────────────────────

    fn wmv2_decode_intra(
        &mut self,
        payload: &[u8],
        hdr:     &Wmv2FrameHeader,
        frame:   &mut YuvFrame,
    ) -> Result<()> {
        // Start at picture header end.
        let mut br = BitReader::new_at(payload, hdr.header_bits);

        // upstream: ff_wmv2_decode_secondary_picture_header() (I-picture branch).
        // We parse/consume the fields that affect alignment and DC VLC selection.
        self.wmv2_j_type = if self.wmv2_j_type_bit { br.read_bit().unwrap_or(false) } else { false };
        if self.wmv2_j_type {
            // IntraX8 (j_type) is not handled in this A build.
            return Ok(());
        }

        self.wmv2_per_mb_rl_table = if self.wmv2_per_mb_rl_bit { br.read_bit().unwrap_or(false) } else { false };
        if !self.wmv2_per_mb_rl_table {
            self.wmv2_rl_chroma_table_index = decode012(&mut br);
            self.wmv2_rl_table_index        = decode012(&mut br);
        }
        self.wmv2_dc_table_index = br.read_bit().unwrap_or(false) as usize;

        let mb_w = self.width_mb  as usize;
        let mb_h = self.height_mb as usize;

        // Reset predictors.
        self.wmv2_dc_pred = Wmv2DcPredBuffer::new(mb_w, mb_h);
        for v in self.wmv2_coded_block.iter_mut() { *v = 0; }

        self.wmv2_reset_picture_state();

        let qscale = hdr.pquant as i32;
        // WMV2 picture header variant used here (upstream-min) does not carry ttcoef;
        // keep using intra VLC set 0 to get the stream back in sync.

        for mb_row in 0..mb_h {
            for mb_col in 0..mb_w {
                if br.is_empty() { break; }

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
                    let mut block = self.wmv2_decode_block_intra_ref(&mut br, mb_row, mb_col, blk, coded, qscale, ac_pred)?;

                    let (is_luma, bx, by, stride, _ph) = block_coords(mb_row as u32, mb_col as u32, blk, frame.width, frame.height);
                    let plane: &mut Vec<u8> = if is_luma { &mut frame.y } else if blk == 4 { &mut frame.cb } else { &mut frame.cr };
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
        hdr:     &Wmv2FrameHeader,
        frame:   &mut YuvFrame,
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

        self.wmv2_mspel = if self.wmv2_mspel_bit { br.read_bit().unwrap_or(false) } else { false };

        if self.wmv2_abt_flag {
            self.wmv2_per_mb_abt = br.read_bit().unwrap_or(false) ^ true;
            if !self.wmv2_per_mb_abt {
                self.wmv2_abt_type = decode012(&mut br);
            }
        } else {
            self.wmv2_per_mb_abt = false;
            self.wmv2_abt_type = 0;
        }

        self.wmv2_per_mb_rl_table = if self.wmv2_per_mb_rl_bit { br.read_bit().unwrap_or(false) } else { false };
        if !self.wmv2_per_mb_rl_table {
            self.wmv2_rl_table_index = decode012(&mut br);
            self.wmv2_rl_chroma_table_index = self.wmv2_rl_table_index;
        }
        if br.bits_left() < 2 {
            return Err(DecoderError::InvalidData("WMV2: truncated secondary header".into()));
        }
        self.wmv2_dc_table_index = br.read_bit().unwrap_or(false) as usize;
        self.wmv2_mv_table_index = br.read_bit().unwrap_or(false) as usize;

        // Reset predictors for this picture.
        self.wmv2_dc_pred = Wmv2DcPredBuffer::new(mb_w, mb_h);
        if self.wmv2_motion.len() != mb_w * mb_h {
            self.wmv2_motion.resize(mb_w * mb_h, (0, 0));
        }
        for v in self.wmv2_motion.iter_mut() { *v = (0, 0); }

        self.wmv2_reset_picture_state();

        let reference = match &self.wmv2_ref {
            Some(r) => r.clone(),
            None    => YuvFrame::new(frame.width, frame.height),
        };

        for mb_row in 0..mb_h {
            let first_slice_line = self.wmv2_slice_height != 0 && (mb_row % self.wmv2_slice_height == 0);
            for mb_col in 0..mb_w {
                if br.bits_left() <= 0 { break; }
                let mi = mb_row * mb_w + mb_col;
                if mi < self.wmv2_mb_skip.len() && self.wmv2_mb_skip[mi] {
                    if self.wmv2_mspel { wmv2_mspel_motion_mb(frame, &reference, mb_row, mb_col, 0, 0, 0); } else { motion_compensate_mb(frame, &reference, mb_row, mb_col, 0, 0); }
                    self.wmv2_motion_set(mb_row, mb_col, (0, 0));
                    continue;
                }

                let code = self.wmv2_mb_non_intra_vlc[self.wmv2_cbp_table_index.min(3)]
                    .decode(&mut br)
                    .ok_or_else(|| DecoderError::InvalidData("WMV2: MB header VLC underrun".into()))? as i32;

                let mb_intra = (code & 0x40) == 0;
                let cbp = (code & 0x3f) as u8;

                if !mb_intra {
                    let pred = self.wmv2_pred_motion(&mut br, mb_row, mb_col, first_slice_line);

                    if cbp != 0 {
                        if self.wmv2_per_mb_rl_table {
                            self.wmv2_rl_table_index = decode012(&mut br);
                            self.wmv2_rl_chroma_table_index = self.wmv2_rl_table_index;
                        }
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

                    if self.wmv2_mspel { wmv2_mspel_motion_mb(frame, &reference, mb_row, mb_col, mx, my, self.wmv2_hshift); } else { motion_compensate_mb(frame, &reference, mb_row, mb_col, mx, my); }

                    for blk in 0..6usize {
                        if (cbp >> (5 - blk)) & 1 == 0 { continue; }

                        let mut cur_abt = abt_type;
                        if per_block_abt {
                            cur_abt = decode012(&mut br);
                        }

                        // upstream: wmv2_decode_inter_block + wmv2_add_block

                        if cur_abt == 0 {

                            let scan = &FF_WMV1_SCANTABLE[0];

                            let mut block = self.wmv2_decode_block_inter_ref(&mut br, blk, true, qscale, scan)?;

                            let (is_luma, bx, by, stride, _ph) = block_coords(mb_row as u32, mb_col as u32, blk, frame.width, frame.height);

                            let plane: &mut Vec<u8> = if is_luma { &mut frame.y } else if blk == 4 { &mut frame.cb } else { &mut frame.cr };

                            let dst_off = by * stride + bx;

                            wmv2dsp::wmv2_idct_add(plane, dst_off, stride, &mut block);

                        } else {

                            const SUB_CBP_TABLE: [u8; 3] = [2, 3, 1];

                            let scantable = if cur_abt == 1 { &FF_WMV2_SCANTABLE_A } else { &FF_WMV2_SCANTABLE_B };

                            let sub_cbp = SUB_CBP_TABLE[decode012(&mut br) as usize];

                        

                            let mut block1 = [0i16; 64];

                            let mut block2 = [0i16; 64];

                            if (sub_cbp & 1) != 0 {

                                block1 = self.wmv2_decode_block_inter_ref(&mut br, blk, true, qscale, scantable)?;

                            }

                            if (sub_cbp & 2) != 0 {

                                block2 = self.wmv2_decode_block_inter_ref(&mut br, blk, true, qscale, scantable)?;

                            }

                        

                            let (is_luma, bx, by, stride, _ph) = block_coords(mb_row as u32, mb_col as u32, blk, frame.width, frame.height);

                            let plane: &mut Vec<u8> = if is_luma { &mut frame.y } else if blk == 4 { &mut frame.cb } else { &mut frame.cr };

                            let dst_off = by * stride + bx;

                        

                            match cur_abt {

                                1 => {

                                    // 8x4 + 8x4 (top/bottom)

                                    ffidct::ff_simple_idct84_add(plane, dst_off, stride, &mut block1);

                                    ffidct::ff_simple_idct84_add(plane, dst_off + 4 * stride, stride, &mut block2);

                                }

                                2 => {

                                    // 4x8 + 4x8 (left/right)

                                    ffidct::ff_simple_idct48_add(plane, dst_off, stride, &mut block1);

                                    ffidct::ff_simple_idct48_add(plane, dst_off + 4, stride, &mut block2);

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
                        let mut block = self.wmv2_decode_block_intra_ref(&mut br, mb_row, mb_col, blk, coded, qscale, ac_pred)?;

                        let (is_luma, bx, by, stride, _ph) = block_coords(mb_row as u32, mb_col as u32, blk, frame.width, frame.height);
                        let plane: &mut Vec<u8> = if is_luma { &mut frame.y } else if blk == 4 { &mut frame.cb } else { &mut frame.cr };
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
    br:      &mut BitReader<'_>,
    ac_vlc:  &VlcTable,
    pquant:  i32,
    coeff:   &mut [i32; 64],
    is_intra: bool,
) {
    // WMV2/MSMPEG4 uses the standard zig-zag scan by default.
    // (AC prediction, if implemented, switches to horizontal/vertical scans.)
    let scan = &ZIGZAG;
    let mut idx = if is_intra { 1usize } else { 0 };

    loop {
        let sym = match ac_vlc.decode(br) {
            Some(s) => s,
            None    => break,
        };

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
        if idx >= 64 { break; }

        // Uniform quantization.
        let q = iquant_uniform(signed_level, pquant, false);
        coeff[scan[idx]] = q;

        idx += 1;
        if last || br.is_empty() { break; }
    }
}

// ─── WMV2 MV reader ───────────────────────────────────────────────────────────
// Reads a differential MV using a fixed 7-bit Huffman code (simplified from
// H.263 MVD table) then adds the median predictor.

fn wmv2_read_mv(
    br:      &mut BitReader<'_>,
    mv_pred: &MvPredictor,
    mb_row:  usize,
    mb_col:  usize,
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
            if br.read_bit().unwrap_or(true) { break; }
            m += 1;
            if m >= mv_range { break; }
        }
        m
    };
    if mag == 0 { return 0; }
    let sign = br.read_bit().unwrap_or(false);
    if sign { -mag } else { mag }
}

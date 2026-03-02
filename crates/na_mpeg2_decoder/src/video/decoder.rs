use std::sync::Arc;

use super::bitreader::GetBits;
use super::error::{DecodeError, Result};
use super::frame::{Frame, PixelFormat};
use super::idct::{simple_idct_add, simple_idct_put};
use super::motion::{MotionCompensator, MotionOp};
use super::tables::{
    FF_ALTERNATE_VERTICAL_SCAN, FF_MPEG1_DEFAULT_INTRA_MATRIX, FF_MPEG1_DEFAULT_NON_INTRA_MATRIX,
    FF_MPEG2_NON_LINEAR_QSCALE, FF_ZIGZAG_DIRECT,
};
use super::vlc::{get_rl_vlc, get_vlc2};
use super::vlctables::{
    get_vlcs, has_cbp, is_intra, is_quant, mb_type_mv_2_mv_dir, MB_TYPE_16x16, MB_TYPE_16x8,
    MB_TYPE_BACKWARD_MV, MB_TYPE_BIDIR_MV, MB_TYPE_CBP, MB_TYPE_FORWARD_MV, MB_TYPE_INTRA,
    MB_TYPE_INTERLACED, MB_TYPE_QUANT, MB_TYPE_SKIP, MB_TYPE_ZERO_MV, MV_DIR_BACKWARD,
    MV_DIR_FORWARD,
};

const PICT_TOP_FIELD: i32 = 1;
const PICT_BOTTOM_FIELD: i32 = 2;
const PICT_FRAME: i32 = 3;

const PICT_TYPE_I: i32 = 1;
const PICT_TYPE_P: i32 = 2;
const PICT_TYPE_B: i32 = 3;

const MV_TYPE_16X16: i32 = 0;
const MV_TYPE_16X8: i32 = 1;
const MV_TYPE_FIELD: i32 = 2;
const MV_TYPE_DMV: i32 = 3;

const MT_FIELD: i32 = 1;
const MT_FRAME: i32 = 2;
const MT_DMV: i32 = 3;

#[inline]
fn clip_coeff12(v: i32) -> i16 {
    // MPEG-1/2 IDCT input is 12-bit signed ([-2048, 2047]).
    if v < -2048 {
        -2048i16
    } else if v > 2047 {
        2047i16
    } else {
        v as i16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodecKind {
    Mpeg1,
    Mpeg2,
}

#[derive(Clone, Debug)]
struct PictureParams {
    pict_type: i32,
    _temporal_reference: u16,
}

#[derive(Debug)]
pub struct Decoder {
    es_buf: Vec<u8>,

    codec: CodecKind,
    width: usize,
    height: usize,
    mb_width: usize,
    mb_height: usize,
    chroma_format: i32,
    pix_fmt: PixelFormat,
    progressive_sequence: bool,
    low_delay: bool,

    intra_matrix: [u16; 64],
    inter_matrix: [u16; 64],
    chroma_intra_matrix: [u16; 64],
    chroma_inter_matrix: [u16; 64],

    intra_dc_precision: i32,
    picture_structure: i32,
    top_field_first: bool,
    frame_pred_frame_dct: bool,
    concealment_motion_vectors: bool,
    q_scale_type: i32,
    intra_vlc_format: bool,
    alternate_scan: bool,
    repeat_first_field: bool,
    chroma_420_type: bool,
    progressive_frame: bool,

    mpeg_f_code: [[i32; 2]; 2],
    full_pel: [bool; 2],

    pic: Option<PictureParams>,
    cur: Option<Frame>,
    cur_mb_type: i16,
    mb_types: Vec<i16>,
    mb_x: usize,
    mb_y: usize,
    qscale: i32,
    mb_intra: bool,
    mb_skipped: bool,
    mv_dir: i32,
    mv_type: i32,
    field_select: [[i32; 2]; 2],
    last_dc: [i32; 3],
    last_mv: [[[i32; 2]; 2]; 2],
    mv: [[[i32; 2]; 4]; 2],
    interlaced_dct: bool,

    blocks: [[i16; 64]; 12],
    block_last_index: [i32; 12],

    ref_prev: Option<Arc<Frame>>,
    ref_cur: Option<Arc<Frame>>,

    mc: MotionCompensator,
}

impl Default for Decoder {
    fn default() -> Self {
        let mut dec = Self {
            es_buf: Vec::new(),
            codec: CodecKind::Mpeg2,
            width: 0,
            height: 0,
            mb_width: 0,
            mb_height: 0,
            chroma_format: 1,
            pix_fmt: PixelFormat::Yuv420p,
            progressive_sequence: true,
            low_delay: false,
            intra_matrix: [0u16; 64],
            inter_matrix: [0u16; 64],
            chroma_intra_matrix: [0u16; 64],
            chroma_inter_matrix: [0u16; 64],
            intra_dc_precision: 0,
            picture_structure: PICT_FRAME,
            top_field_first: false,
            frame_pred_frame_dct: true,
            concealment_motion_vectors: false,
            q_scale_type: 0,
            intra_vlc_format: false,
            alternate_scan: false,
            repeat_first_field: false,
            chroma_420_type: false,
            progressive_frame: true,
            mpeg_f_code: [[1, 1], [1, 1]],
            full_pel: [false, false],
            pic: None,
            cur: None,
            cur_mb_type: 0,
            mb_types: Vec::new(),
            mb_x: 0,
            mb_y: 0,
            qscale: 0,
            mb_intra: false,
            mb_skipped: false,
            mv_dir: 0,
            mv_type: MV_TYPE_16X16,
            field_select: [[0, 0], [0, 0]],
            last_dc: [0, 0, 0],
            last_mv: [[[0, 0], [0, 0]], [[0, 0], [0, 0]]],
            mv: [[[0, 0]; 4]; 2],
            interlaced_dct: false,
            blocks: [[0i16; 64]; 12],
            block_last_index: [-1; 12],
            ref_prev: None,
            ref_cur: None,
            mc: MotionCompensator::new(),
        };
        dec.load_default_matrices();
        dec
    }
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn decode_shared(&mut self, data: &[u8], pts_90k: Option<i64>) -> Result<Vec<Arc<Frame>>> {
        if pts_90k.is_some() {
            if let Some(cur) = self.cur.as_mut() {
                if cur.pts_90k.is_none() {
                    cur.pts_90k = pts_90k;
                }
            }
        }

        self.es_buf.extend_from_slice(data);
        let mut out: Vec<Arc<Frame>> = Vec::new();

        loop {
            let Some((unit_start, code)) = find_next_start_code(&self.es_buf, 0) else {
                if self.es_buf.len() > 3 {
                    let keep = self.es_buf.split_off(self.es_buf.len() - 3);
                    self.es_buf = keep;
                }
                break;
            };

            if unit_start > 0 {
                self.es_buf.drain(0..unit_start);
            }
            if self.es_buf.len() < 4 {
                break;
            }

            let payload_start = 4usize;
            let Some((next_start, _)) = find_next_start_code(&self.es_buf, payload_start) else {
                break;
            };

            let payload = self.es_buf[payload_start..next_start].to_vec();
            self.process_unit(code, &payload, pts_90k, &mut out)?;
            self.es_buf.drain(0..next_start);
        }

        Ok(out)
    }

    pub fn flush_shared(&mut self) -> Result<Vec<Arc<Frame>>> {
        let mut out = Vec::new();
        self.finish_picture(&mut out)?;
        if !self.low_delay {
            if let Some(r) = self.ref_cur.take() {
                out.push(r);
            }
            self.ref_prev = None;
        } else {
            self.ref_prev = None;
            self.ref_cur = None;
        }
        Ok(out)
    }

    pub fn decode(&mut self, data: &[u8], pts_90k: Option<i64>) -> Result<Vec<Frame>> {
        Ok(self
            .decode_shared(data, pts_90k)?
            .into_iter()
            .map(|f| (*f).clone())
            .collect())
    }

    pub fn flush(&mut self) -> Result<Vec<Frame>> {
        Ok(self
            .flush_shared()?
            .into_iter()
            .map(|f| (*f).clone())
            .collect())
    }

    fn process_unit(&mut self, code: u8, payload: &[u8], pts_90k: Option<i64>, out: &mut Vec<Arc<Frame>>) -> Result<()> {
        match code {
            0x00 => {
                self.finish_picture(out)?;
                self.decode_picture_header(payload, pts_90k)?;
            }
            0xB3 => {
                self.finish_picture(out)?;
                self.decode_sequence_header(payload)?;
            }
            0xB5 => {
                self.decode_extension(payload)?;
            }
            0xB7 => {
                self.finish_picture(out)?;
            }
            0xB8 | 0xB2 => {}
            0x01..=0xAF => {
                if self.cur.is_none() || self.pic.is_none() {
                    return Ok(());
                }
                // Slice start code: 0x01..=0xAF => slice_vertical_position.
                // For field pictures, slice rows address every other macroblock row.
                let mut mb_y = (code as usize).wrapping_sub(1);
                if self.picture_structure != PICT_FRAME {
                    mb_y = mb_y
                        .saturating_mul(2)
                        .saturating_add(((self.picture_structure - 1) & 1) as usize);
                }
                if mb_y >= self.mb_height {
                    return Err(DecodeError::InvalidData("slice mb_y overflow"));
                }
                if let Err(e) = self.decode_slice(mb_y, payload) {
                    // Be tolerant to slice-local bitstream damage or container junk.
                    // This mirrors the typical "error concealment" behavior: skip the
                    // remainder of the slice and continue with the next one.
                    if let DecodeError::InvalidData(tag) = e {
                        let pict_type = self.pic.as_ref().map(|p| p.pict_type).unwrap_or(0);
                        log::warn!("Video slice decode error (row={} pict_type={}): {}", mb_y, pict_type, tag);
                        return Ok(());
                    }
                    return Err(e);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn finish_picture(&mut self, out: &mut Vec<Arc<Frame>>) -> Result<()> {
        let Some(pic) = self.pic.take() else {
            self.cur = None;
            return Ok(());
        };
        let Some(cur) = self.cur.take() else {
            return Ok(());
        };

        let cur_arc = Arc::new(cur);
        match pic.pict_type {
            PICT_TYPE_B => out.push(cur_arc),
            PICT_TYPE_I | PICT_TYPE_P => {
                if self.low_delay {
                    out.push(cur_arc.clone());
                    self.ref_cur = Some(cur_arc);
                    self.ref_prev = None;
                } else {
                    if let Some(prev) = self.ref_cur.take() {
                        self.ref_prev = Some(prev.clone());
                        out.push(prev);
                    }
                    self.ref_cur = Some(cur_arc);
                }
            }
            _ => out.push(cur_arc),
        }
        Ok(())
    }

    fn decode_sequence_header(&mut self, payload: &[u8]) -> Result<()> {
        let mut gb = GetBits::init(payload);
        let w = gb.get_bits(12) as usize;
        let h = gb.get_bits(12) as usize;
        if w == 0 || h == 0 {
            return Err(DecodeError::InvalidData("sequence size"));
        }
        let _ = gb.get_bits(4);
        let _ = gb.get_bits(4);
        let _ = gb.get_bits(18);
        if gb.get_bits1() == 0 {
            return Err(DecodeError::InvalidData("sequence marker"));
        }
        let _ = gb.get_bits(10);
        let _ = gb.get_bits1();

        if gb.get_bits1() != 0 {
            self.load_matrix_from_stream(&mut gb, true)?;
        } else {
            self.load_default_intra_matrix();
        }
        if gb.get_bits1() != 0 {
            self.load_matrix_from_stream(&mut gb, false)?;
        } else {
            self.load_default_inter_matrix();
        }

        self.width = w;
        self.height = h;
        self.codec = CodecKind::Mpeg1;
        self.progressive_sequence = true;
        self.progressive_frame = true;
        self.picture_structure = PICT_FRAME;
        self.frame_pred_frame_dct = true;
        self.chroma_format = 1;
        self.pix_fmt = PixelFormat::Yuv420p;
        self.low_delay = false;
        self.recompute_frame_layout()?;
        Ok(())
    }

    fn decode_picture_header(&mut self, payload: &[u8], pts_90k: Option<i64>) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(DecodeError::InvalidData("picture before sequence"));
        }
        let mut gb = GetBits::init(payload);
        let temporal_reference = gb.get_bits(10) as u16;
        let pict_type = gb.get_bits(3) as i32;
        let _ = gb.get_bits(16);
        if pict_type != PICT_TYPE_I && pict_type != PICT_TYPE_P && pict_type != PICT_TYPE_B {
            return Err(DecodeError::Unsupported("picture type"));
        }

        if self.codec == CodecKind::Mpeg1 {
            match pict_type {
                PICT_TYPE_P => {
                    self.full_pel[0] = gb.get_bits1() != 0;
                    self.mpeg_f_code[0][0] = gb.get_bits(3) as i32;
                    self.mpeg_f_code[0][1] = self.mpeg_f_code[0][0];
                }
                PICT_TYPE_B => {
                    self.full_pel[0] = gb.get_bits1() != 0;
                    self.mpeg_f_code[0][0] = gb.get_bits(3) as i32;
                    self.mpeg_f_code[0][1] = self.mpeg_f_code[0][0];
                    self.full_pel[1] = gb.get_bits1() != 0;
                    self.mpeg_f_code[1][0] = gb.get_bits(3) as i32;
                    self.mpeg_f_code[1][1] = self.mpeg_f_code[1][0];
                }
                _ => {}
            }
        }

        gb.skip_1stop_8data_bits()?;

        self.intra_dc_precision = 0;
        self.picture_structure = PICT_FRAME;
        self.top_field_first = false;
        self.frame_pred_frame_dct = true;
        self.concealment_motion_vectors = false;
        self.q_scale_type = 0;
        self.intra_vlc_format = false;
        self.alternate_scan = false;
        self.repeat_first_field = false;
        self.chroma_420_type = false;
        self.progressive_frame = self.progressive_sequence;

        let mut f = Frame::new(self.mb_width * 16, self.mb_height * 16, self.pix_fmt);
        f.pts_90k = pts_90k;

        self.pic = Some(PictureParams { pict_type, _temporal_reference: temporal_reference });
        self.cur = Some(f);
        self.mb_types.clear();
        self.mb_types.resize(self.mb_width * self.mb_height, 0);
        Ok(())
    }

    fn decode_extension(&mut self, payload: &[u8]) -> Result<()> {
        let mut gb = GetBits::init(payload);
        if gb.bits_left() < 4 {
            return Err(DecodeError::InvalidData("extension id"));
        }
        match gb.get_bits(4) {
            1 => self.decode_sequence_extension(&mut gb)?,
            3 => self.decode_quant_matrix_extension(&mut gb)?,
            8 => self.decode_picture_coding_extension(&mut gb)?,
            _ => {}
        }
        Ok(())
    }

    fn decode_sequence_extension(&mut self, gb: &mut GetBits<'_>) -> Result<()> {
        gb.skip_bits(1);
        let _ = gb.get_bits(3);
        let _ = gb.get_bits(4);
        self.progressive_sequence = gb.get_bits1() != 0;
        self.chroma_format = gb.get_bits(2) as i32;
        if self.chroma_format == 0 {
            self.chroma_format = 1;
        }
        let h_ext = gb.get_bits(2) as usize;
        let v_ext = gb.get_bits(2) as usize;
        self.width |= h_ext << 12;
        self.height |= v_ext << 12;
        let _ = gb.get_bits(12);
        if gb.get_bits1() == 0 {
            return Err(DecodeError::InvalidData("seqext marker"));
        }
        let _ = gb.get_bits(8);
        self.low_delay = gb.get_bits1() != 0;
        let _ = gb.get_bits(2);
        let _ = gb.get_bits(5);

        self.codec = CodecKind::Mpeg2;
        self.pix_fmt = match self.chroma_format {
            1 => PixelFormat::Yuv420p,
            2 => PixelFormat::Yuv422p,
            3 => PixelFormat::Yuv444p,
            _ => PixelFormat::Yuv420p,
        };
        self.recompute_frame_layout()?;
        Ok(())
    }

    fn decode_quant_matrix_extension(&mut self, gb: &mut GetBits<'_>) -> Result<()> {
        if gb.get_bits1() != 0 {
            self.load_matrix_from_stream(gb, true)?;
        }
        if gb.get_bits1() != 0 {
            self.load_matrix_from_stream(gb, false)?;
        }
        if gb.get_bits1() != 0 {
            self.load_matrix_chroma_only(gb, true)?;
        }
        if gb.get_bits1() != 0 {
            self.load_matrix_chroma_only(gb, false)?;
        }
        Ok(())
    }

    // Fixed: no extra skip_bits1; bit layout must be read sequentially.
    fn decode_picture_coding_extension(&mut self, gb: &mut GetBits<'_>) -> Result<()> {
        self.full_pel = [false, false];
        self.mpeg_f_code[0][0] = gb.get_bits(4) as i32;
        self.mpeg_f_code[0][1] = gb.get_bits(4) as i32;
        self.mpeg_f_code[1][0] = gb.get_bits(4) as i32;
        self.mpeg_f_code[1][1] = gb.get_bits(4) as i32;
        for d in 0..2 {
            for c in 0..2 {
                if self.mpeg_f_code[d][c] == 0 {
                    self.mpeg_f_code[d][c] = 1;
                }
            }
        }
        self.intra_dc_precision = gb.get_bits(2) as i32;
        self.picture_structure = gb.get_bits(2) as i32;
        self.top_field_first = gb.get_bits1() != 0;
        self.frame_pred_frame_dct = gb.get_bits1() != 0;
        self.concealment_motion_vectors = gb.get_bits1() != 0;
        self.q_scale_type = gb.get_bits1() as i32;
        self.intra_vlc_format = gb.get_bits1() != 0;
        self.alternate_scan = gb.get_bits1() != 0;
        self.repeat_first_field = gb.get_bits1() != 0;
        self.chroma_420_type = gb.get_bits1() != 0;
        self.progressive_frame = gb.get_bits1() != 0;
        Ok(())
    }

    fn recompute_frame_layout(&mut self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }
        self.mb_width = (self.width + 15) >> 4;
        self.mb_height = (self.height + 15) >> 4;
        Ok(())
    }

    // ---------------- slice / macroblock decode ----------------

    fn decode_slice(&mut self, mb_y_start: usize, payload: &[u8]) -> Result<()> {
        let vlcs = get_vlcs();
        let mut gb = GetBits::init(payload);

        self.interlaced_dct = false;
        self.qscale = mpeg_get_qscale(&mut gb, self.q_scale_type);
        if self.qscale == 0 {
            return Err(DecodeError::InvalidData("qscale==0"));
        }

        gb.skip_1stop_8data_bits()?;

        // Initial macroblock address increment.
        let mut mb_x = 0usize;
        while gb.bits_left() > 0 {
            let code = get_vlc2(&mut gb, &vlcs.mbincr.table, vlcs.mbincr.bits, 2);
            if code < 0 {
                return Err(DecodeError::InvalidData("first mb_incr"));
            }
            if code >= 33 {
                if code == 33 {
                    mb_x = mb_x.saturating_add(33);
                }
                // stuffing/end: ignore
            } else {
                mb_x = mb_x.saturating_add(code as usize);
                break;
            }
        }

        if mb_x >= self.mb_width {
            return Err(DecodeError::InvalidData("initial mb_x overflow"));
        }

        // Reset DC and MV predictors at slice start.
        self.last_dc[0] = 128 << self.intra_dc_precision;
        self.last_dc[1] = self.last_dc[0];
        self.last_dc[2] = self.last_dc[0];
        self.last_mv = [[[0, 0], [0, 0]], [[0, 0], [0, 0]]];

        self.mb_x = mb_x;
        self.mb_y = mb_y_start;

        // Number of skipped macroblocks before the next coded one.
        let mut mb_skip_run: i32 = 0;
        loop {
            if self.mb_y >= self.mb_height {
                break;
            }
            self.mb_skipped = false;

            self.decode_mb(&mut gb, &mut mb_skip_run)?;
            self.reconstruct_mb()?;

            // Advance macroblock position.
            self.mb_x += 1;
            if self.mb_x >= self.mb_width {
                self.mb_x = 0;
                let field_pic = self.picture_structure != PICT_FRAME;
                self.mb_y += if field_pic { 2 } else { 1 };
                if self.mb_y >= self.mb_height {
                    break;
                }
            }

            // Read next macroblock_address_increment when not inside a skip run.
            if mb_skip_run == -1 {
                mb_skip_run = 0;
                loop {
                    let code = get_vlc2(&mut gb, &vlcs.mbincr.table, vlcs.mbincr.bits, 2);
                    if code < 0 {
                        return Err(DecodeError::InvalidData("mb_incr"));
                    }
                    if code >= 33 {
                        if code == 33 {
                            mb_skip_run += 33;
                        } else if code == 35 {
                            // end of slice
                            if mb_skip_run != 0 || gb.show_bits(15) != 0 {
                                return Err(DecodeError::InvalidData("slice mismatch"));
                            }
                            return Ok(());
                        }
                        // stuffing: ignore
                    } else {
                        mb_skip_run += code;
                        break;
                    }
                }

                if mb_skip_run != 0 {
                    let pict_type = self.pic.as_ref().map(|p| p.pict_type).unwrap_or(0);
                    if pict_type == PICT_TYPE_I {
                        return Err(DecodeError::InvalidData("skipped MB in I-picture"));
                    }

                    self.mb_intra = false;
                    for v in &mut self.block_last_index {
                        *v = -1;
                    }
                    self.last_dc[0] = 128 << self.intra_dc_precision;
                    self.last_dc[1] = self.last_dc[0];
                    self.last_dc[2] = self.last_dc[0];

                    if self.picture_structure == PICT_FRAME {
                        self.mv_type = MV_TYPE_16X16;
                    } else {
                        self.mv_type = MV_TYPE_FIELD;
                    }

                    if pict_type == PICT_TYPE_P {
                        self.mv_dir = MV_DIR_FORWARD;
                        self.mv[0][0] = [0, 0];
                        self.last_mv[0][0] = [0, 0];
                        self.last_mv[0][1] = [0, 0];
                        self.field_select[0][0] = (self.picture_structure - 1) & 1;
                    } else {
                        self.mv[0][0] = self.last_mv[0][0];
                        self.mv[1][0] = self.last_mv[1][0];
                        self.field_select[0][0] = (self.picture_structure - 1) & 1;
                        self.field_select[1][0] = (self.picture_structure - 1) & 1;
                    }
                }
            }
        }

        Ok(())
    }

    fn decode_mb(&mut self, gb: &mut GetBits<'_>, mb_skip_run: &mut i32) -> Result<()> {
        let vlcs = get_vlcs();
        let pict_type = self.pic.as_ref().map(|p| p.pict_type).unwrap_or(0);

        self.mb_skipped = false;

        // Skip-run fast path.
        if *mb_skip_run != 0 {
            *mb_skip_run -= 1;
            if pict_type == PICT_TYPE_P {
                self.mb_skipped = true;
                self.mb_intra = false;
                self.mv_dir = MV_DIR_FORWARD;
                self.mv_type = MV_TYPE_16X16;
                self.cur_mb_type = MB_TYPE_SKIP | MB_TYPE_FORWARD_MV | MB_TYPE_16x16;
            } else {
                let prev_mb_type = if self.mb_x > 0 {
                    self.mb_types[self.mb_x - 1 + self.mb_y * self.mb_width]
                } else if self.mb_y > 0 {
                    self.mb_types[self.mb_width - 1 + (self.mb_y - 1) * self.mb_width]
                } else {
                    0
                };
                if is_intra(prev_mb_type) {
                    return Err(DecodeError::InvalidData("skip with prev intra"));
                }
                self.cur_mb_type = prev_mb_type | MB_TYPE_SKIP;

                let z = self.mv[0][0][0] | self.mv[0][0][1] | self.mv[1][0][0] | self.mv[1][0][1];
                if z == 0 {
                    self.mb_skipped = true;
                }
                self.mb_intra = false;
            }

            self.store_mb_type();
            return Ok(());
        }

        // Decode mb_type.
        let mut mb_type: i16 = match pict_type {
            PICT_TYPE_I => {
                if gb.get_bits1() == 0 {
                    if gb.get_bits1() == 0 {
                        return Err(DecodeError::InvalidData("I mb_type"));
                    }
                    MB_TYPE_QUANT | MB_TYPE_INTRA
                } else {
                    MB_TYPE_INTRA
                }
            }
            PICT_TYPE_P => {
                let v = get_vlc2(gb, &vlcs.mb_ptype.table, vlcs.mb_ptype.bits, 1);
                if v < 0 {
                    return Err(DecodeError::InvalidData("P mb_type"));
                }
                v as i16
            }
            PICT_TYPE_B => {
                let v = get_vlc2(gb, &vlcs.mb_btype.table, vlcs.mb_btype.bits, 1);
                if v < 0 {
                    return Err(DecodeError::InvalidData("B mb_type"));
                }
                v as i16
            }
            _ => return Err(DecodeError::InvalidData("pict_type")),
        };

        self.cur_mb_type = mb_type;
        let mb_block_count = 4 + (1usize << (self.chroma_format as usize));

        if is_intra(mb_type) {
            for i in 0..mb_block_count {
                self.blocks[i] = [0i16; 64];
                self.block_last_index[i] = -1;
            }
            if self.picture_structure == PICT_FRAME && !self.frame_pred_frame_dct {
                self.interlaced_dct = gb.get_bits1() != 0;
            } else {
                self.interlaced_dct = false;
            }
            if is_quant(mb_type) {
                self.qscale = mpeg_get_qscale(gb, self.q_scale_type);
            }

            if self.concealment_motion_vectors {
                if self.picture_structure != PICT_FRAME {
                    gb.skip_bits1();
                }
                let mx = mpeg_decode_motion(gb, &vlcs.mv, self.mpeg_f_code[0][0], self.last_mv[0][0][0])?;
                let my = mpeg_decode_motion(gb, &vlcs.mv, self.mpeg_f_code[0][1], self.last_mv[0][0][1])?;
                self.mv[0][0][0] = mx;
                self.mv[0][0][1] = my;
                self.last_mv[0][0][0] = mx;
                self.last_mv[0][0][1] = my;
                self.last_mv[0][1][0] = mx;
                self.last_mv[0][1][1] = my;
                if gb.get_bits1() == 0 {
                    return Err(DecodeError::InvalidData("cmv marker"));
                }
            } else {
                self.last_mv = [[[0, 0], [0, 0]], [[0, 0], [0, 0]]];
            }

            self.mb_intra = true;
            if self.codec == CodecKind::Mpeg2 {
                for i in 0..mb_block_count {
                    self.mpeg2_decode_block_intra(gb, i)?;
                }
            } else {
                for i in 0..6 {
                    self.mpeg1_decode_block_intra(gb, i, self.qscale)?;
                }
            }
        } else {
            if (mb_type & MB_TYPE_ZERO_MV) != 0 {
                if (mb_type & MB_TYPE_CBP) == 0 {
                    return Err(DecodeError::InvalidData("zero_mv without cbp"));
                }
                self.mv_dir = MV_DIR_FORWARD;
                if self.picture_structure == PICT_FRAME {
                    if !self.frame_pred_frame_dct {
                        self.interlaced_dct = gb.get_bits1() != 0;
                    } else {
                        self.interlaced_dct = false;
                    }
                    self.mv_type = MV_TYPE_16X16;
                } else {
                    self.mv_type = MV_TYPE_FIELD;
                    mb_type |= MB_TYPE_INTERLACED;
                    self.field_select[0][0] = self.picture_structure - 1;
                }
                if is_quant(mb_type) {
                    self.qscale = mpeg_get_qscale(gb, self.q_scale_type);
                }
                self.last_mv[0][0] = [0, 0];
                self.last_mv[0][1] = [0, 0];
                self.mv[0][0] = [0, 0];
            } else {
                let motion_type = if self.picture_structure == PICT_FRAME && self.frame_pred_frame_dct {
                    MT_FRAME
                } else {
                    let mt = gb.get_bits(2) as i32;
                    if self.picture_structure == PICT_FRAME && has_cbp(mb_type) {
                        self.interlaced_dct = gb.get_bits1() != 0;
                    }
                    mt
                };
                if is_quant(mb_type) {
                    self.qscale = mpeg_get_qscale(gb, self.q_scale_type);
                }

                self.mv_dir = mb_type_mv_2_mv_dir(mb_type);
                match motion_type {
                    MT_FRAME => {
                        if self.picture_structure == PICT_FRAME {
                            mb_type |= MB_TYPE_16x16;
                            self.mv_type = MV_TYPE_16X16;
                            for dir in 0..2 {
                                if has_mv_dir(mb_type, dir) {
                                    let mx = mpeg_decode_motion(
                                        gb,
                                        &vlcs.mv,
                                        self.mpeg_f_code[dir][0],
                                        self.last_mv[dir][0][0],
                                    )?;
                                    let my = mpeg_decode_motion(
                                        gb,
                                        &vlcs.mv,
                                        self.mpeg_f_code[dir][1],
                                        self.last_mv[dir][0][1],
                                    )?;
                                    self.last_mv[dir][0][0] = mx;
                                    self.last_mv[dir][0][1] = my;
                                    self.last_mv[dir][1][0] = mx;
                                    self.last_mv[dir][1][1] = my;
                                    self.mv[dir][0][0] = if self.full_pel[dir] { mx * 2 } else { mx };
                                    self.mv[dir][0][1] = if self.full_pel[dir] { my * 2 } else { my };
                                }
                            }
                        } else {
                            mb_type |= MB_TYPE_16x8 | MB_TYPE_INTERLACED;
                            self.mv_type = MV_TYPE_16X8;
                            for dir in 0..2 {
                                if has_mv_dir(mb_type, dir) {
                                    for j in 0..2 {
                                        self.field_select[dir][j] = gb.get_bits1() as i32;
                                        for k in 0..2 {
                                            let val = mpeg_decode_motion(
                                                gb,
                                                &vlcs.mv,
                                                self.mpeg_f_code[dir][k],
                                                self.last_mv[dir][j][k],
                                            )?;
                                            self.last_mv[dir][j][k] = val;
                                            self.mv[dir][j][k] = val;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    MT_FIELD => {
                        self.mv_type = MV_TYPE_FIELD;
                        if self.picture_structure == PICT_FRAME {
                            mb_type |= MB_TYPE_16x8 | MB_TYPE_INTERLACED;
                            for dir in 0..2 {
                                if has_mv_dir(mb_type, dir) {
                                    for j in 0..2 {
                                        self.field_select[dir][j] = gb.get_bits1() as i32;
                                        let mx = mpeg_decode_motion(
                                            gb,
                                            &vlcs.mv,
                                            self.mpeg_f_code[dir][0],
                                            self.last_mv[dir][j][0],
                                        )?;
                                        self.last_mv[dir][j][0] = mx;
                                        self.mv[dir][j][0] = mx;
                                        let my = mpeg_decode_motion(
                                            gb,
                                            &vlcs.mv,
                                            self.mpeg_f_code[dir][1],
                                            self.last_mv[dir][j][1] >> 1,
                                        )?;
                                        self.last_mv[dir][j][1] = 2 * my;
                                        self.mv[dir][j][1] = my;
                                    }
                                }
                            }
                        } else {
                            mb_type |= MB_TYPE_16x16 | MB_TYPE_INTERLACED;
                            for dir in 0..2 {
                                if has_mv_dir(mb_type, dir) {
                                    self.field_select[dir][0] = gb.get_bits1() as i32;
                                    for k in 0..2 {
                                        let val = mpeg_decode_motion(
                                            gb,
                                            &vlcs.mv,
                                            self.mpeg_f_code[dir][k],
                                            self.last_mv[dir][0][k],
                                        )?;
                                        self.last_mv[dir][0][k] = val;
                                        self.last_mv[dir][1][k] = val;
                                        self.mv[dir][0][k] = val;
                                    }
                                }
                            }
                        }
                    }
                    MT_DMV => {
                        if self.progressive_sequence {
                            return Err(DecodeError::InvalidData("MT_DMV in progressive"));
                        }
                        self.mv_type = MV_TYPE_DMV;
                        for dir in 0..2 {
                            if has_mv_dir(mb_type, dir) {
                                let my_shift = if self.picture_structure == PICT_FRAME { 1 } else { 0 };
                                let mx = mpeg_decode_motion(
                                    gb,
                                    &vlcs.mv,
                                    self.mpeg_f_code[dir][0],
                                    self.last_mv[dir][0][0],
                                )?;
                                self.last_mv[dir][0][0] = mx;
                                self.last_mv[dir][1][0] = mx;
                                let dmx = get_dmv(gb);
                                let my = mpeg_decode_motion(
                                    gb,
                                    &vlcs.mv,
                                    self.mpeg_f_code[dir][1],
                                    self.last_mv[dir][0][1] >> my_shift,
                                )?;
                                let dmy = get_dmv(gb);
                                self.last_mv[dir][0][1] = my * (1 << my_shift);
                                self.last_mv[dir][1][1] = my * (1 << my_shift);
                                self.mv[dir][0][0] = mx;
                                self.mv[dir][0][1] = my;
                                self.mv[dir][1][0] = mx;
                                self.mv[dir][1][1] = my;
                                if self.picture_structure == PICT_FRAME {
                                    mb_type |= MB_TYPE_16x16 | MB_TYPE_INTERLACED;
                                    let mut m = if self.top_field_first { 1 } else { 3 };
                                    self.mv[dir][2][0] = ((mx * m + if mx > 0 { 1 } else { 0 }) >> 1) + dmx;
                                    self.mv[dir][2][1] = ((my * m + if my > 0 { 1 } else { 0 }) >> 1) + dmy - 1;
                                    m = 4 - m;
                                    self.mv[dir][3][0] = ((mx * m + if mx > 0 { 1 } else { 0 }) >> 1) + dmx;
                                    self.mv[dir][3][1] = ((my * m + if my > 0 { 1 } else { 0 }) >> 1) + dmy + 1;
                                } else {
                                    mb_type |= MB_TYPE_16x16;
                                    self.mv[dir][2][0] = ((mx + if mx > 0 { 1 } else { 0 }) >> 1) + dmx;
                                    self.mv[dir][2][1] = ((my + if my > 0 { 1 } else { 0 }) >> 1) + dmy;
                                    if self.picture_structure == PICT_TOP_FIELD {
                                        self.mv[dir][2][1] -= 1;
                                    } else {
                                        self.mv[dir][2][1] += 1;
                                    }
                                }
                            }
                        }
                    }
                    _ => return Err(DecodeError::InvalidData("motion_type")),
                }
            }

            self.mb_intra = false;
            self.last_dc[0] = 128 << self.intra_dc_precision;
            self.last_dc[1] = self.last_dc[0];
            self.last_dc[2] = self.last_dc[0];

            if (mb_type & MB_TYPE_CBP) != 0 {
                for i in 0..mb_block_count {
                    self.blocks[i] = [0i16; 64];
                }
                let cbp = get_vlc2(gb, &vlcs.mb_pat.table, vlcs.mb_pat.bits, 1);
                if cbp <= 0 {
                    return Err(DecodeError::InvalidData("cbp"));
                }
                let mut cbp_u = cbp as u32;
                if mb_block_count > 6 {
                    cbp_u <<= (mb_block_count - 6) as u32;
                    cbp_u |= gb.get_bits(mb_block_count - 6) as u32;
                }
                if self.codec == CodecKind::Mpeg2 {
                    let shift = 12usize.saturating_sub(mb_block_count);
                    cbp_u <<= shift as u32;
                    for i in 0..mb_block_count {
                        if (cbp_u & (1 << 11)) != 0 {
                            self.mpeg2_decode_block_non_intra(gb, i)?;
                        } else {
                            self.block_last_index[i] = -1;
                        }
                        cbp_u <<= 1;
                    }
                } else {
                    for i in 0..6 {
                        if (cbp_u & 32) != 0 {
                            self.mpeg1_decode_block_inter(gb, i)?;
                        } else {
                            self.block_last_index[i] = -1;
                        }
                        cbp_u <<= 1;
                    }
                }
            } else {
                for i in 0..12 {
                    self.block_last_index[i] = -1;
                }
            }
        }

        self.cur_mb_type = mb_type;
        self.store_mb_type();
        // Signal the caller to read the next increment after a coded MB.
        *mb_skip_run -= 1;
        Ok(())
    }

    fn store_mb_type(&mut self) {
        if self.mb_y < self.mb_height && self.mb_x < self.mb_width {
            let idx = self.mb_x + self.mb_y * self.mb_width;
            if idx < self.mb_types.len() {
                self.mb_types[idx] = self.cur_mb_type;
            }
        }
    }

    fn reconstruct_mb(&mut self) -> Result<()> {
        let pict_type = self.pic.as_ref().map(|p| p.pict_type).unwrap_or(0);
        // Take the current frame out temporarily to avoid holding a mutable borrow
        // of `self.cur` while calling other `&mut self` helpers.
        let mut cur = self
            .cur
            .take()
            .ok_or(DecodeError::Internal("no current frame"))?;

        if self.mb_intra {
            self.put_intra_blocks(&mut cur);
            self.cur = Some(cur);
            return Ok(());
        }

        match pict_type {
            PICT_TYPE_P => {
                let Some(ref_frame) = self.ref_cur.as_ref() else {
                    // No reference picture yet: keep the (partially) decoded frame.
                    self.cur = Some(cur);
                    return Ok(());
                };
                self.mc.mpv_motion(
                    &mut cur,
                    ref_frame,
                    0,
                    MotionOp::Put,
                    self.mv_type,
                    self.picture_structure,
                    self.mb_x,
                    self.mb_y,
                    &self.mv,
                    &self.field_select,
                    false,
                );
            }
            PICT_TYPE_B => {
                let mut did_any = false;
                if (self.mv_dir & MV_DIR_FORWARD) != 0 {
                    if let Some(ref_frame) = self.ref_prev.as_ref() {
                        self.mc.mpv_motion(
                            &mut cur,
                            ref_frame,
                            0,
                            MotionOp::Put,
                            self.mv_type,
                            self.picture_structure,
                            self.mb_x,
                            self.mb_y,
                            &self.mv,
                            &self.field_select,
                            false,
                        );
                        did_any = true;
                    }
                }
                if (self.mv_dir & MV_DIR_BACKWARD) != 0 {
                    if let Some(ref_frame) = self.ref_cur.as_ref() {
                        self.mc.mpv_motion(
                            &mut cur,
                            ref_frame,
                            1,
                            if did_any { MotionOp::Avg } else { MotionOp::Put },
                            self.mv_type,
                            self.picture_structure,
                            self.mb_x,
                            self.mb_y,
                            &self.mv,
                            &self.field_select,
                            false,
                        );
                        did_any = true;
                    }
                }
                let _ = did_any;
            }
            _ => {}
        }

        self.add_inter_blocks(&mut cur);
        self.cur = Some(cur);
        Ok(())
    }

    fn put_intra_blocks(&mut self, cur: &mut Frame) {
        let base_x = self.mb_x * 16;
        let base_y = self.mb_y * 16;

        for by in 0..2 {
            for bx in 0..2 {
                let bi = by * 2 + bx;
                let x = base_x + bx * 8;
                let y = base_y + by * 8;
                if x + 8 <= cur.width && y + 8 <= cur.height {
                    let off = y * cur.linesize_y + x;
                    simple_idct_put(&mut cur.data_y[off..], cur.linesize_y, &mut self.blocks[bi]);
                }
            }
        }

        match cur.format {
            PixelFormat::Yuv420p => {
                let x = self.mb_x * 8;
                let y = self.mb_y * 8;
                let off_u = y * cur.linesize_u + x;
                let off_v = y * cur.linesize_v + x;
                simple_idct_put(&mut cur.data_u[off_u..], cur.linesize_u, &mut self.blocks[4]);
                simple_idct_put(&mut cur.data_v[off_v..], cur.linesize_v, &mut self.blocks[5]);
            }
            PixelFormat::Yuv422p => {
                let x = self.mb_x * 8;
                let y = self.mb_y * 16;
                for by in 0..2 {
                    let off_u = (y + by * 8) * cur.linesize_u + x;
                    simple_idct_put(&mut cur.data_u[off_u..], cur.linesize_u, &mut self.blocks[4 + by]);
                }
                for by in 0..2 {
                    let off_v = (y + by * 8) * cur.linesize_v + x;
                    simple_idct_put(&mut cur.data_v[off_v..], cur.linesize_v, &mut self.blocks[6 + by]);
                }
            }
            PixelFormat::Yuv444p => {
                for plane in 0..2 {
                    for by in 0..2 {
                        for bx in 0..2 {
                            let b = by * 2 + bx;
                            let x = base_x + bx * 8;
                            let y = base_y + by * 8;
                            let (dst, stride, idx) = if plane == 0 {
                                (&mut cur.data_u, cur.linesize_u, 4 + b)
                            } else {
                                (&mut cur.data_v, cur.linesize_v, 8 + b)
                            };
                            let off = y * stride + x;
                            simple_idct_put(&mut dst[off..], stride, &mut self.blocks[idx]);
                        }
                    }
                }
            }
        }
    }

    fn add_inter_blocks(&mut self, cur: &mut Frame) {
        let base_x = self.mb_x * 16;
        let base_y = self.mb_y * 16;

        for by in 0..2 {
            for bx in 0..2 {
                let bi = by * 2 + bx;
                if self.block_last_index[bi] < 0 {
                    continue;
                }
                let x = base_x + bx * 8;
                let y = base_y + by * 8;
                let off = y * cur.linesize_y + x;
                simple_idct_add(&mut cur.data_y[off..], cur.linesize_y, &mut self.blocks[bi]);
            }
        }

        match cur.format {
            PixelFormat::Yuv420p => {
                if self.block_last_index[4] >= 0 {
                    let x = self.mb_x * 8;
                    let y = self.mb_y * 8;
                    let off_u = y * cur.linesize_u + x;
                    simple_idct_add(&mut cur.data_u[off_u..], cur.linesize_u, &mut self.blocks[4]);
                }
                if self.block_last_index[5] >= 0 {
                    let x = self.mb_x * 8;
                    let y = self.mb_y * 8;
                    let off_v = y * cur.linesize_v + x;
                    simple_idct_add(&mut cur.data_v[off_v..], cur.linesize_v, &mut self.blocks[5]);
                }
            }
            PixelFormat::Yuv422p => {
                let x = self.mb_x * 8;
                let y = self.mb_y * 16;
                for by in 0..2 {
                    let idx_u = 4 + by;
                    if self.block_last_index[idx_u] >= 0 {
                        let off_u = (y + by * 8) * cur.linesize_u + x;
                        simple_idct_add(&mut cur.data_u[off_u..], cur.linesize_u, &mut self.blocks[idx_u]);
                    }
                    let idx_v = 6 + by;
                    if self.block_last_index[idx_v] >= 0 {
                        let off_v = (y + by * 8) * cur.linesize_v + x;
                        simple_idct_add(&mut cur.data_v[off_v..], cur.linesize_v, &mut self.blocks[idx_v]);
                    }
                }
            }
            PixelFormat::Yuv444p => {
                for plane in 0..2 {
                    for by in 0..2 {
                        for bx in 0..2 {
                            let b = by * 2 + bx;
                            let x = base_x + bx * 8;
                            let y = base_y + by * 8;
                            let (dst, stride, idx) = if plane == 0 {
                                (&mut cur.data_u, cur.linesize_u, 4 + b)
                            } else {
                                (&mut cur.data_v, cur.linesize_v, 8 + b)
                            };
                            if self.block_last_index[idx] < 0 {
                                continue;
                            }
                            let off = y * stride + x;
                            simple_idct_add(&mut dst[off..], stride, &mut self.blocks[idx]);
                        }
                    }
                }
            }
        }
    }

    fn mpeg2_decode_block_intra(&mut self, gb: &mut GetBits<'_>, n: usize) -> Result<()> {
        let vlcs = get_vlcs();
        let component = self.block_component(n);
        let quant_matrix = if component == 0 { &self.intra_matrix } else { &self.chroma_intra_matrix };
        let alt = self.alternate_scan;
        let scantable: &'static [u8; 64] = if alt {
            &FF_ALTERNATE_VERTICAL_SCAN
        } else {
            &FF_ZIGZAG_DIRECT
        };
        let qscale = self.qscale;

        let diff = self.decode_dc(gb, component)?;
        let dc = self.last_dc[component] + diff;
        self.last_dc[component] = dc;
        self.blocks[n][0] = clip_coeff12(dc * (1 << (3 - self.intra_dc_precision)));

        let mut mismatch: i32 = (self.blocks[n][0] as i32) ^ 1;
        let mut i: i32 = 0;
        let rl = if self.intra_vlc_format { &vlcs.rl_mpeg2 } else { &vlcs.rl_mpeg1 };

        loop {
            let (level, run) = get_rl_vlc(gb, rl, super::vlctables::TEX_VLC_BITS, 2);
            if level == 127 {
                break;
            }
            if level != 0 {
                i += run as i32;
                if i > 63 {
                    return Err(DecodeError::InvalidData("ac"));
                }
                let j = scantable[i as usize] as usize;
                let mut lv = (level as i32 * qscale * quant_matrix[j] as i32) >> 4;
                if gb.get_bits1() != 0 {
                    lv = -lv;
                }
                let lv_c = clip_coeff12(lv) as i32;
                self.blocks[n][j] = lv_c as i16;
                mismatch ^= lv_c;
            } else {
                let run2 = gb.get_bits(6) as i32 + 1;
                let lv0 = gb.get_sbits(12);
                i += run2;
                if i > 63 {
                    return Err(DecodeError::InvalidData("ac"));
                }
                let j = scantable[i as usize] as usize;
                let mut lv = lv0 as i32;
                if lv < 0 {
                    lv = -(((-lv) * qscale * quant_matrix[j] as i32) >> 4);
                } else {
                    lv = (lv * qscale * quant_matrix[j] as i32) >> 4;
                }
                let lv_c = clip_coeff12(lv) as i32;
                self.blocks[n][j] = lv_c as i16;
                mismatch ^= lv_c;
            }
        }

        self.blocks[n][63] ^= (mismatch & 1) as i16;
        self.block_last_index[n] = i;
        Ok(())
    }

    fn mpeg2_decode_block_non_intra(&mut self, gb: &mut GetBits<'_>, n: usize) -> Result<()> {
        let vlcs = get_vlcs();
        let component = self.block_component(n);
        let quant_matrix = if component == 0 { &self.inter_matrix } else { &self.chroma_inter_matrix };
        let alt = self.alternate_scan;
        let scantable: &'static [u8; 64] = if alt {
            &FF_ALTERNATE_VERTICAL_SCAN
        } else {
            &FF_ZIGZAG_DIRECT
        };
        let qscale = self.qscale;

        let mut mismatch: i32 = 1;
        let mut i: i32 = -1;

        if gb.show_bits(1) != 0 {
            gb.skip_bits1();
            let sign = gb.get_bits1();
            let mut level = ((3 * qscale * quant_matrix[0] as i32) >> 5) as i32;
            if sign != 0 {
                level = -level;
            }
            let lv_c = clip_coeff12(level) as i32;
            self.blocks[n][0] = lv_c as i16;
            mismatch ^= lv_c;
            i += 1;
        }

        while gb.bits_left() > 0 {
            if gb.show_bits(2) == 2 {
                gb.skip_bits(2);
                break;
            }

            let (level0, run) = get_rl_vlc(gb, &vlcs.rl_mpeg1, super::vlctables::TEX_VLC_BITS, 2);
            if level0 != 0 {
                i += run as i32;
                if i > 63 {
                    return Err(DecodeError::InvalidData("ac"));
                }
                let j = scantable[i as usize] as usize;
                let mut lv = (((level0 as i32) * 2 + 1) * qscale * quant_matrix[j] as i32) >> 5;
                if gb.get_bits1() != 0 {
                    lv = -lv;
                }
                let lv_c = clip_coeff12(lv) as i32;
                self.blocks[n][j] = lv_c as i16;
                mismatch ^= lv_c;
            } else {
                let run2 = gb.get_bits(6) as i32 + 1;
                let lv0 = gb.get_sbits(12);
                i += run2;
                if i > 63 {
                    return Err(DecodeError::InvalidData("ac"));
                }
                let j = scantable[i as usize] as usize;
                let mut lv = lv0;
                if lv < 0 {
                    lv = -((((-lv) * 2 + 1) * qscale * quant_matrix[j] as i32) >> 5);
                } else {
                    lv = (((lv) * 2 + 1) * qscale * quant_matrix[j] as i32) >> 5;
                }
                let lv_c = clip_coeff12(lv) as i32;
                self.blocks[n][j] = lv_c as i16;
                mismatch ^= lv_c;
            }
        }

        self.blocks[n][63] ^= (mismatch & 1) as i16;
        self.block_last_index[n] = i;
        Ok(())
    }

    fn mpeg1_decode_block_intra(&mut self, gb: &mut GetBits<'_>, n: usize, qscale: i32) -> Result<()> {
        let component = if n <= 3 { 0 } else { (n - 4) + 1 };
        let diff = self.decode_dc(gb, component)?;
        let dc = self.last_dc[component] + diff;
        self.last_dc[component] = dc;
        self.blocks[n][0] = clip_coeff12(dc * self.intra_matrix[0] as i32);

        let alt = self.alternate_scan;
        let scantable: &'static [u8; 64] = if alt {
            &FF_ALTERNATE_VERTICAL_SCAN
        } else {
            &FF_ZIGZAG_DIRECT
        };
        let vlcs = get_vlcs();
        let mut i: i32 = 0;

        while gb.bits_left() > 0 {
            if gb.show_bits(2) == 2 {
                gb.skip_bits(2);
                break;
            }

            let (level0, run) = get_rl_vlc(gb, &vlcs.rl_mpeg1, super::vlctables::TEX_VLC_BITS, 2);
            let mut level: i32;
            let run_i = run as i32;
            if level0 != 0 {
                i += run_i;
                if i > 63 {
                    return Err(DecodeError::InvalidData("ac"));
                }
                let j = scantable[i as usize] as usize;
                level = (level0 as i32 * qscale * self.intra_matrix[j] as i32) >> 4;
                level = (level - 1) | 1;
                if gb.get_bits1() != 0 {
                    level = -level;
                }
                self.blocks[n][j] = clip_coeff12(level);
            } else {
                let run2 = gb.get_bits(6) as i32 + 1;
                let mut lv = gb.get_sbits(8);
                if lv == -128 {
                    lv = gb.get_bits(8) as i32 - 256;
                } else if lv == 0 {
                    lv = gb.get_bits(8) as i32;
                }
                i += run2;
                if i > 63 {
                    return Err(DecodeError::InvalidData("ac"));
                }
                let j = scantable[i as usize] as usize;
                if lv < 0 {
                    lv = -lv;
                    level = (lv * qscale * self.intra_matrix[j] as i32) >> 4;
                    level = (level - 1) | 1;
                    level = -level;
                } else {
                    level = (lv * qscale * self.intra_matrix[j] as i32) >> 4;
                    level = (level - 1) | 1;
                }
                self.blocks[n][j] = clip_coeff12(level);
            }
        }

        self.block_last_index[n] = i;
        Ok(())
    }

    fn mpeg1_decode_block_inter(&mut self, gb: &mut GetBits<'_>, n: usize) -> Result<()> {
        let vlcs = get_vlcs();
        let alt = self.alternate_scan;
        let scantable: &'static [u8; 64] = if alt {
            &FF_ALTERNATE_VERTICAL_SCAN
        } else {
            &FF_ZIGZAG_DIRECT
        };
        let qscale = self.qscale;
        let quant_matrix = &self.inter_matrix;

        let mut i: i32 = -1;

        if gb.show_bits(1) != 0 {
            gb.skip_bits1();
            let sign = gb.get_bits1();
            let mut level = ((3 * qscale * quant_matrix[0] as i32) >> 5) as i32;
            level = (level - 1) | 1;
            if sign != 0 {
                level = -level;
            }
            self.blocks[n][0] = clip_coeff12(level);
            i += 1;
        }

        while gb.bits_left() > 0 {
            if gb.show_bits(2) == 2 {
                gb.skip_bits(2);
                break;
            }

            let (level0, run) = get_rl_vlc(gb, &vlcs.rl_mpeg1, super::vlctables::TEX_VLC_BITS, 2);
            let mut level: i32;
            if level0 != 0 {
                i += run as i32;
                if i > 63 {
                    return Err(DecodeError::InvalidData("ac"));
                }
                let j = scantable[i as usize] as usize;
                level = (((level0 as i32) * 2 + 1) * qscale * quant_matrix[j] as i32) >> 5;
                level = (level - 1) | 1;
                if gb.get_bits1() != 0 {
                    level = -level;
                }
                self.blocks[n][j] = clip_coeff12(level);
            } else {
                let run2 = gb.get_bits(6) as i32 + 1;
                let mut lv = gb.get_sbits(8);
                if lv == -128 {
                    lv = gb.get_bits(8) as i32 - 256;
                } else if lv == 0 {
                    lv = gb.get_bits(8) as i32;
                }
                i += run2;
                if i > 63 {
                    return Err(DecodeError::InvalidData("ac"));
                }
                let j = scantable[i as usize] as usize;
                if lv < 0 {
                    lv = -lv;
                    level = (((lv) * 2 + 1) * qscale * quant_matrix[j] as i32) >> 5;
                    level = (level - 1) | 1;
                    level = -level;
                } else {
                    level = (((lv) * 2 + 1) * qscale * quant_matrix[j] as i32) >> 5;
                    level = (level - 1) | 1;
                }
                self.blocks[n][j] = clip_coeff12(level);
            }
        }

        self.block_last_index[n] = i;
        Ok(())
    }

    fn decode_dc(&self, gb: &mut GetBits<'_>, component: usize) -> Result<i32> {
        let vlcs = get_vlcs();
        let code = if component == 0 {
            get_vlc2(gb, &vlcs.dc_lum.table, vlcs.dc_lum.bits, 2)
        } else {
            get_vlc2(gb, &vlcs.dc_chroma.table, vlcs.dc_chroma.bits, 2)
        };
        if code < 0 {
            return Err(DecodeError::InvalidData("dc"));
        }
        if code == 0 {
            Ok(0)
        } else {
            Ok(gb.get_xbits(code as usize))
        }
    }

    fn scan_table(&self) -> &[u8; 64] {
        if self.alternate_scan {
            &FF_ALTERNATE_VERTICAL_SCAN
        } else {
            &FF_ZIGZAG_DIRECT
        }
    }

    fn block_component(&self, n: usize) -> usize {
        match self.pix_fmt {
            PixelFormat::Yuv420p => {
                if n < 4 { 0 } else if n == 4 { 1 } else { 2 }
            }
            PixelFormat::Yuv422p => {
                if n < 4 { 0 } else if n < 6 { 1 } else { 2 }
            }
            PixelFormat::Yuv444p => {
                if n < 4 { 0 } else if n < 8 { 1 } else { 2 }
            }
        }
    }

    fn load_default_matrices(&mut self) {
        self.load_default_intra_matrix();
        self.load_default_inter_matrix();
    }

    fn load_default_intra_matrix(&mut self) {
        for i in 0..64 {
            let j = FF_ZIGZAG_DIRECT[i] as usize;
            let v = FF_MPEG1_DEFAULT_INTRA_MATRIX[i];
            self.intra_matrix[j] = v;
            self.chroma_intra_matrix[j] = v;
        }
    }

    fn load_default_inter_matrix(&mut self) {
        for i in 0..64 {
            let j = FF_ZIGZAG_DIRECT[i] as usize;
            let v = FF_MPEG1_DEFAULT_NON_INTRA_MATRIX[i];
            self.inter_matrix[j] = v;
            self.chroma_inter_matrix[j] = v;
        }
    }

    fn load_matrix_from_stream(&mut self, gb: &mut GetBits<'_>, intra: bool) -> Result<()> {
        for i in 0..64 {
            let j = FF_ZIGZAG_DIRECT[i] as usize;
            let mut v = gb.get_bits(8) as u16;
            if v == 0 {
                return Err(DecodeError::InvalidData("matrix"));
            }
            if intra && i == 0 && v != 8 {
                v = 8;
            }
            if intra {
                self.intra_matrix[j] = v;
                self.chroma_intra_matrix[j] = v;
            } else {
                self.inter_matrix[j] = v;
                self.chroma_inter_matrix[j] = v;
            }
        }
        Ok(())
    }

    fn load_matrix_chroma_only(&mut self, gb: &mut GetBits<'_>, intra: bool) -> Result<()> {
        for i in 0..64 {
            let j = FF_ZIGZAG_DIRECT[i] as usize;
            let mut v = gb.get_bits(8) as u16;
            if v == 0 {
                return Err(DecodeError::InvalidData("matrix"));
            }
            if intra && i == 0 && v != 8 {
                v = 8;
            }
            if intra {
                self.chroma_intra_matrix[j] = v;
            } else {
                self.chroma_inter_matrix[j] = v;
            }
        }
        Ok(())
    }
}

#[inline]
fn mpeg_get_qscale(gb: &mut GetBits<'_>, q_scale_type: i32) -> i32 {
    let code = gb.get_bits(5) as usize;
    if q_scale_type == 0 {
        // MPEG-1/2 linear quantiser scale uses even values (2..62).
        // FFmpeg keeps `qscale` in that internal representation: `code << 1`.
        (code as i32) << 1
    } else {
        FF_MPEG2_NON_LINEAR_QSCALE[code] as i32
    }
}

#[inline]
fn has_mv_dir(mb_type: i16, dir: usize) -> bool {
    if dir == 0 {
        (mb_type & (MB_TYPE_FORWARD_MV | MB_TYPE_BIDIR_MV)) != 0
    } else {
        (mb_type & (MB_TYPE_BACKWARD_MV | MB_TYPE_BIDIR_MV)) != 0
    }
}

#[inline]
fn mpeg_decode_motion(
    gb: &mut GetBits<'_>,
    mv_vlc: &super::vlc::Vlc,
    fcode: i32,
    pred: i32,
) -> Result<i32> {
    let code = get_vlc2(gb, &mv_vlc.table, mv_vlc.bits, 2);
    if code == 0 {
        return Ok(pred);
    }
    if code < 0 {
        return Err(DecodeError::InvalidData("mv"));
    }
    let sign = gb.get_bits1() as i32;
    let shift = fcode - 1;
    let mut val = code;
    if shift != 0 {
        val = (val - 1) << shift;
        val |= gb.get_bits(shift as usize) as i32;
        val += 1;
    }
    if sign != 0 {
        val = -val;
    }
    val += pred;
    Ok(GetBits::sign_extend(val, (5 + shift) as usize))
}

#[inline]
fn get_dmv(gb: &mut GetBits<'_>) -> i32 {
    if gb.get_bits1() != 0 {
        1 - ((gb.get_bits1() as i32) << 1)
    } else {
        0
    }
}

fn find_next_start_code(buf: &[u8], from: usize) -> Option<(usize, u8)> {
    if buf.len() < 4 {
        return None;
    }
    let mut i = from;
    while i + 3 < buf.len() {
        if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1 {
            return Some((i, buf[i + 3]));
        }
        i += 1;
    }
    None
}

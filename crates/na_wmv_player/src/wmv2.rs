//! WMV2 (Windows Media Video 8 / MS-MPEG4 V3) picture header parsing.
//!
//! This module intentionally mirrors upstream's `wmv2dec.c` picture-header logic:
//!
//!   - `wmv2_decode_picture_header()` for the primary picture header
//!   - Frame-skipped probe for P pictures
//!
//! Any alternative/custom headers are deliberately not supported.

use crate::bitreader::BitReader;
use crate::error::{DecoderError, Result};

#[derive(Debug, Clone)]
pub struct Wmv2Params {
    pub width:  u32,
    pub height: u32,
}

impl Wmv2Params {
    pub fn new(width: u32, height: u32) -> Self {
        Wmv2Params { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wmv2FrameType {
    I,
    P,
}

#[derive(Debug, Clone)]
pub struct Wmv2FrameHeader {
    pub frame_type: Wmv2FrameType,
    /// Quantizer (upstream: `qscale`, range 1..31).
    pub pquant: u8,

    /// Whether upstream would return `FRAME_SKIPPED` from `wmv2_decode_picture_header()`.
    /// For such frames, the decoder should just output the previous reference.
    pub frame_skipped: bool,

    /// Bit offset (from the beginning of the payload slice) where the secondary picture
    /// header / macroblock layer begins.
    pub header_bits: usize,
}

impl Wmv2FrameHeader {
    /// Parse all plausible upstream-aligned picture headers for this payload.
    ///
    /// `mb_w/mb_h` are macroblock dimensions used by upstream's skipped-frame probe.
    pub fn parse_candidates(data: &[u8], mb_w: u32, mb_h: u32) -> Vec<Self> {
        let mut out = Vec::new();
        if data.is_empty() {
            return out;
        }
        if let Some(h) = Self::parse_ref_picture_header(data, mb_w, mb_h) {
            out.push(h);
        }
        out
    }

    pub fn parse(data: &[u8], mb_w: u32, mb_h: u32) -> Result<Self> {
        if data.is_empty() {
            return Err(DecoderError::InvalidData("Empty WMV2 payload".into()));
        }
        let mut cands = Self::parse_candidates(data, mb_w, mb_h);
        if cands.is_empty() {
            return Err(DecoderError::InvalidData("Could not parse WMV2 picture header".into()));
        }
        Ok(cands.remove(0))
    }

    /// upstream: `wmv2_decode_picture_header()`.
    ///
    /// Layout (MSB-first):
    ///   - 1 bit: `get_bits1()` => 0 = I, 1 = P
    ///   - if I: 7 bits "I7" (ignored)
    ///   - 5 bits: `qscale` (1..31)
    ///   - if P and the next bit is 1: run a skipped-frame probe on a cloned bitreader
    fn parse_ref_picture_header(data: &[u8], mb_w: u32, mb_h: u32) -> Option<Self> {
        const SKIP_TYPE_COL: u32 = 3;

        let mut br = BitReader::new(data);

        // upstream: h->c.pict_type = get_bits1(&gb) + 1;
        let is_p = br.read_bit()?;
        let frame_type = if is_p { Wmv2FrameType::P } else { Wmv2FrameType::I };

        if frame_type == Wmv2FrameType::I {
            let _i7 = br.read_bits(7)?;
            let _ = _i7;
        }

        let qscale = br.read_bits(5)? as u8;
        if qscale == 0 {
            return None;
        }

        let mut frame_skipped = false;

        // upstream skipped-frame probe (P only):
        // if (pict_type != I && show_bits(1)) { ...; if (!run) return FRAME_SKIPPED; }
        if frame_type == Wmv2FrameType::P {
            if br.peek_bits(1)? == 1 {
                let mut gb = br.clone();
                let skip_type = gb.read_bits(2)?;
                let mut run: i32 = if skip_type == SKIP_TYPE_COL {
                    mb_w as i32
                } else {
                    mb_h as i32
                };

                while run > 0 {
                    let block = run.min(25);
                    let bits = gb.read_bits(block as u8)?;
                    if bits != ((1u32 << block) - 1) {
                        break;
                    }
                    run -= block;
                }

                if run == 0 {
                    frame_skipped = true;
                }
            }
        }

        let header_bits = br.bits_read();

        Some(Wmv2FrameHeader {
            frame_type,
            pquant: qscale,
            frame_skipped,
            header_bits,
        })
    }
}

impl std::fmt::Display for Wmv2FrameType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Wmv2FrameType::I => write!(f, "I"),
            Wmv2FrameType::P => write!(f, "P"),
        }
    }
}

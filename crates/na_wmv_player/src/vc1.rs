/// VC-1 / WMV9 Sequence & Picture Header Parser + Bitplane Decoder
/// 

use crate::bitreader::BitReader;
use crate::error::{DecoderError, Result};

// ─── Enums ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile { Simple, Main, Advanced }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType { I, P, B, BI, Skipped }

impl std::fmt::Display for FrameType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameType::I       => write!(f, "I"),
            FrameType::P       => write!(f, "P"),
            FrameType::B       => write!(f, "B"),
            FrameType::BI      => write!(f, "BI"),
            FrameType::Skipped => write!(f, "skip"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantizerMode { Implicit, Explicit, NonUniform, Uniform }

// ─── Sequence Header ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SequenceHeader {
    pub profile:        Profile,
    pub max_b_frames:   u8,
    pub frame_rate_num: u32,
    pub frame_rate_den: u32,
    pub loop_filter:    bool,
    pub multires:       bool,
    pub fastuvmc:       bool,
    pub extended_mv:    bool,
    pub dquant:         u8,
    pub vstransform:    bool,
    pub overlap:        bool,
    pub syncmarker:     bool,
    pub rangered:       bool,
    pub quantizer_mode: QuantizerMode,
    pub finterpflag:    bool,
    pub transacfrm:     u8,   // inter AC table index 0-3 (TRANSACFRM)
    pub transacfrm2:    u8,   // intra AC table index 0-3 (TRANSACFRM2)
    pub mvtab:          u8,   // MV table index 0-3 (MVTAB)
    pub cbptab:         u8,   // CBP table index 0-3 (CBPTAB)
    pub dctab:          bool, // DC table select (DCTAB)
    pub width:          u32,
    pub height:         u32,
    pub display_width:  u32,
    pub display_height: u32,
}

impl SequenceHeader {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(DecoderError::InvalidData("Sequence header too short".into()));
        }
        let mut br = BitReader::new(data);

        // WMV9: first 2 bits = profile
        let profile_bits = br.read_bits(2).unwrap_or(0) as u8;
        let profile = match profile_bits {
            0 => Profile::Simple,
            1 => Profile::Main,
            3 => Profile::Advanced,
            _ => Profile::Main,
        };

        br.read_bits(2); // reserved

        let frmrtq_postproc = br.read_bits(3).unwrap_or(0);
        let _bitrtq_postproc= br.read_bits(5).unwrap_or(0);
        let loop_filter     = br.read_bit().unwrap_or(false);
        let _res_sm         = br.read_bit().unwrap_or(false);
        let multires        = br.read_bit().unwrap_or(false);
        let _res_fasttx     = br.read_bit().unwrap_or(true);
        let fastuvmc        = br.read_bit().unwrap_or(false);
        let extended_mv     = br.read_bit().unwrap_or(false);
        let dquant          = br.read_bits(2).unwrap_or(0) as u8;
        let vstransform     = br.read_bit().unwrap_or(false);
        let _res_transtab   = br.read_bit().unwrap_or(false);
        let overlap         = br.read_bit().unwrap_or(false);
        let _resync_marker  = br.read_bit().unwrap_or(false);
        let rangered        = br.read_bit().unwrap_or(false);
        let max_b_frames    = br.read_bits(3).unwrap_or(0) as u8;
        let quant_bits      = br.read_bits(2).unwrap_or(0) as u8;
        let finterpflag     = br.read_bit().unwrap_or(false);
        let syncmarker      = br.read_bit().unwrap_or(false);
        // Additional fields per SMPTE 421M §8.1.1 (Simple/Main)
        let transacfrm      = br.read_bits(2).unwrap_or(0) as u8;
        let transacfrm2     = br.read_bits(2).unwrap_or(0) as u8;
        let mvtab           = br.read_bits(2).unwrap_or(0) as u8;
        let cbptab          = br.read_bits(2).unwrap_or(0) as u8;
        let dctab           = br.read_bit().unwrap_or(false);

        let quantizer_mode = match quant_bits {
            0 => QuantizerMode::Implicit,
            1 => QuantizerMode::Explicit,
            2 => QuantizerMode::NonUniform,
            _ => QuantizerMode::Uniform,
        };

        let (frame_rate_num, frame_rate_den) = match frmrtq_postproc {
            0 => (6, 1), 1 => (8, 1), 2 => (10, 1), 3 => (12, 1),
            4 => (15, 1), 5 => (24000, 1001), 6 => (24, 1), 7 => (25, 1),
            _ => (30, 1),
        };

        Ok(SequenceHeader {
            profile, max_b_frames, frame_rate_num, frame_rate_den,
            loop_filter, multires, fastuvmc, extended_mv, dquant,
            vstransform, overlap, syncmarker, rangered,
            quantizer_mode, finterpflag,
            transacfrm, transacfrm2, mvtab, cbptab, dctab,
            width: 0, height: 0, display_width: 0, display_height: 0,
        })
    }
}

// ─── Bitplane ────────────────────────────────────────────────────────────────
// SMPTE 421M §8.7.  Used to signal skipped MBs and direct-mode flags.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BitplaneMode {
    Norm2, Diff2, Norm6, Diff6, RowSkip, ColSkip,
}

pub struct Bitplane {
    pub data: Vec<u8>,  // one byte per macroblock (0 or 1)
    pub is_raw: bool,
}

impl Bitplane {
    pub fn decode(br: &mut BitReader<'_>, mb_w: usize, mb_h: usize) -> Option<Self> {
        let n_mb = mb_w * mb_h;
        let mut data = vec![0u8; n_mb];

        // 3-bit mode code
        let mode_bits = br.read_bits(3)?;
        let mode = match mode_bits {
            0 => BitplaneMode::Norm2,
            1 => BitplaneMode::Norm6,
            2 => BitplaneMode::Diff2,
            3 => BitplaneMode::Diff6,
            4 => BitplaneMode::RowSkip,
            5 => BitplaneMode::ColSkip,
            _ => {
                // Raw: one bit per MB
                for i in 0..n_mb {
                    data[i] = br.read_bit()? as u8;
                }
                return Some(Bitplane { data, is_raw: true });
            }
        };

        match mode {
            BitplaneMode::Norm6 | BitplaneMode::Diff6 => {
                // Tile-coded 6 MBs per codeword
                let tile_size = 6usize;
                let mut inv = br.read_bit()? as u8; // invert flag for Diff modes
                if !matches!(mode, BitplaneMode::Diff2 | BitplaneMode::Diff6) { inv = 0; }
                let mut i = 0;
                while i < n_mb {
                    let tile = br.read_bits(tile_size as u8)? as usize;
                    for b in 0..tile_size.min(n_mb - i) {
                        data[i + b] = (((tile >> (tile_size - 1 - b)) & 1) as u8) ^ inv;
                    }
                    i += tile_size;
                }
            }
            BitplaneMode::Norm2 | BitplaneMode::Diff2 => {
                let inv = if matches!(mode, BitplaneMode::Diff2) {
                    br.read_bit()? as u8
                } else { 0 };
                let mut i = 0;
                while i < n_mb {
                    let pair = br.read_bits(2)? as u8;
                    data[i    ] = ((pair >> 1) & 1) ^ inv;
                    if i + 1 < n_mb { data[i+1] = (pair & 1) ^ inv; }
                    i += 2;
                }
            }
            BitplaneMode::RowSkip => {
                for row in 0..mb_h {
                    if br.read_bit()? { continue; }
                    for col in 0..mb_w {
                        data[row * mb_w + col] = br.read_bit()? as u8;
                    }
                }
            }
            BitplaneMode::ColSkip => {
                for col in 0..mb_w {
                    if br.read_bit()? { continue; }
                    for row in 0..mb_h {
                        data[row * mb_w + col] = br.read_bit()? as u8;
                    }
                }
            }
        }

        Some(Bitplane { data, is_raw: false })
    }
}

// ─── Picture Header ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PictureHeader {
    pub frame_type:   FrameType,
    pub pqindex:      u8,
    pub pquant:       u8,
    pub halfqp:       bool,
    pub pqual_mode:   u8,
    pub mvrange:      u8,
    pub rptfrm:       u8,
    pub pts_ms:       u32,
    pub rangeredfrm:  bool,
    /// Bit offset where the macroblock layer starts (from beginning of the frame payload).
    ///
    /// This includes the full picture header and any bitplanes decoded from it.
    pub header_bits:  usize,
    /// Skipped-MB bitplane (None if not present or raw-mode)
    pub skipmb_plane: Option<Vec<u8>>,
    /// Direct-mode bitplane for B-frames
    pub directmb_plane: Option<Vec<u8>>,
    /// B-frame temporal fraction from SMPTE 421M §7.1.3.6 Table 40.
    pub bfrac_num: i32,
    pub bfrac_den: i32,
}

impl PictureHeader {
    pub fn parse(data: &[u8], seq: &SequenceHeader, pts_ms: u32,
                 mb_w: usize, mb_h: usize) -> Result<Self> {
        let mut br = BitReader::new(data);

        // ── frame type ──────────────────────────────────────────────────────
        let frame_type = if seq.max_b_frames > 0 {
            match br.read_bits(2).unwrap_or(0xFF) {
                0b11 => FrameType::I,
                0b10 => FrameType::P,
                0b00 => FrameType::B,
                0b01 => FrameType::BI,
                _    => return Err(DecoderError::InvalidData("Unknown frame type".into())),
            }
        } else {
            match br.read_bit().unwrap_or(false) {
                false => FrameType::P,
                true  => FrameType::I,
            }
        };

        // ── range reduction ─────────────────────────────────────────────────
        let rangeredfrm = seq.rangered && br.read_bit().unwrap_or(false);

        // ── quantizer ───────────────────────────────────────────────────────
        let pqindex = br.read_bits(5).unwrap_or(1) as u8;
        let (pquant, halfqp, pqual_mode) = Self::decode_quantizer(pqindex, seq);

        // ── MV range ────────────────────────────────────────────────────────
        let mvrange = if seq.extended_mv {
            let mut r = 0u8;
            while br.read_bit().unwrap_or(false) {
                r += 1;
                if r >= 3 { break; }
            }
            r
        } else { 0 };

        // ── repeat frame count (I-frame) ─────────────────────────────────
        let rptfrm = if frame_type == FrameType::I {
            br.read_bits(2).unwrap_or(0) as u8
        } else { 0 };

        // ── bitplanes ───────────────────────────────────────────────────────
        // P-frame: skipped-MB bitplane
        let skipmb_plane = if frame_type == FrameType::P {
            Bitplane::decode(&mut br, mb_w, mb_h).map(|bp| bp.data)
        } else { None };

        // B-frame: direct-mode bitplane + skipped-MB bitplane
        let directmb_plane = if frame_type == FrameType::B {
            Bitplane::decode(&mut br, mb_w, mb_h).map(|bp| bp.data)
        } else { None };

        let skipmb_plane = if frame_type == FrameType::B {
            Bitplane::decode(&mut br, mb_w, mb_h).map(|bp| bp.data)
        } else { skipmb_plane };

        // ── BFRACTION (B-frames only, SMPTE 421M §7.1.3.6 Table 40) ──────────
        const BFRAC: [(i32,i32); 8] = [
            (1,2),(1,3),(2,3),(1,4),(3,4),(1,5),(2,5),(1,2),
        ];
        let (bfrac_num, bfrac_den) = if frame_type == FrameType::B {
            let idx = br.read_bits(3).unwrap_or(0) as usize;
            BFRAC[idx.min(7)]
        } else { (1, 2) };

        let header_bits = br.bits_read();

        Ok(PictureHeader {
            frame_type, pqindex, pquant, halfqp, pqual_mode,
            mvrange, rptfrm, pts_ms, rangeredfrm,
            header_bits,
            skipmb_plane, directmb_plane,
            bfrac_num, bfrac_den,
        })
    }

    // ── Legacy parse (no bitplane, backward compat) ─────────────────────────
    pub fn parse_simple(data: &[u8], seq: &SequenceHeader, pts_ms: u32) -> Result<Self> {
        let mb_w = ((seq.width + 15) / 16).max(1) as usize;
        let mb_h = ((seq.height + 15) / 16).max(1) as usize;
        Self::parse(data, seq, pts_ms, mb_w, mb_h)
    }

    fn decode_quantizer(pqindex: u8, seq: &SequenceHeader) -> (u8, bool, u8) {
        match seq.quantizer_mode {
            QuantizerMode::Implicit => {
                // SMPTE 421M Table 5
                let pquant = if pqindex <= 8 { pqindex }
                else {
                    const MAP: [u8; 23] = [
                        9,10,11,12,13,14,15,16,17,18,
                        19,20,21,22,23,24,25,27,29,31,33,63,0,
                    ];
                    MAP.get(pqindex as usize - 9).copied().unwrap_or(pqindex)
                };
                let halfqp = pqindex >= 9 && pquant == 0;
                (pquant, halfqp, 0)
            }
            _ => (pqindex, false, 0),
        }
    }
}

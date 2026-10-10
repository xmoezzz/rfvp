//! VC-1 / WMV9 syntax parser (Simple/Main and progressive Advanced Profile).
//!
//! The bitstream syntax in this module follows FFmpeg's `libavcodec/vc1.c`
//! and SMPTE 421M.  It is intentionally kept separate from the reconstruction
//! code in `decoder.rs` so ASF/WMV3 and ASF/WVC1 can share the same parser.

use crate::bitreader::BitReader;
use crate::error::{DecoderError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Simple,
    Main,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    I,
    P,
    B,
    BI,
    Skipped,
}

impl std::fmt::Display for FrameType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameType::I => write!(f, "I"),
            FrameType::P => write!(f, "P"),
            FrameType::B => write!(f, "B"),
            FrameType::BI => write!(f, "BI"),
            FrameType::Skipped => write!(f, "skip"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantizerMode {
    Implicit = 0,
    Explicit = 1,
    NonUniform = 2,
    Uniform = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MvMode {
    OneMvHpelBilin,
    OneMv,
    OneMvHpel,
    IntensityComp,
    MixedMv,
}

#[derive(Debug, Clone)]
pub struct SequenceHeader {
    pub profile: Profile,
    pub level: u8,
    pub max_b_frames: u8,
    pub frame_rate_num: u32,
    pub frame_rate_den: u32,
    pub loop_filter: bool,
    pub multires: bool,
    pub fastuvmc: bool,
    pub extended_mv: bool,
    pub extended_dmv: bool,
    pub dquant: u8,
    pub vstransform: bool,
    pub overlap: bool,
    pub syncmarker: bool,
    pub rangered: bool,
    pub quantizer_mode: QuantizerMode,
    pub finterpflag: bool,
    pub res_x8: bool,
    pub res_fasttx: bool,
    pub res_rtm_flag: bool,
    pub res_sprite: bool,
    pub postprocflag: bool,
    pub broadcast: bool,
    pub interlace: bool,
    pub tfcntrflag: bool,
    pub psf: bool,
    pub panscanflag: bool,
    pub refdist_flag: bool,
    pub broken_link: bool,
    pub closed_entry: bool,
    pub range_mapy: Option<u8>,
    pub range_mapuv: Option<u8>,
    pub hrd_num_leaky_buckets: u8,
    pub width: u32,
    pub height: u32,
    pub display_width: u32,
    pub display_height: u32,
}

fn find_vc1_marker(data: &[u8], from: usize) -> Option<usize> {
    if data.len() < 4 || from >= data.len().saturating_sub(3) {
        return None;
    }
    (from..=data.len() - 4).find(|&i| data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1)
}

/// Remove VC-1 start-code emulation prevention bytes, matching FFmpeg's
/// `vc1_unescape_buffer()`: 00 00 03 xx (xx < 4) becomes 00 00 xx.
pub fn vc1_unescape_buffer(data: &[u8]) -> Vec<u8> {
    if data.len() < 4 {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0usize;
    while i < data.len() {
        if data[i] == 3
            && i >= 2
            && data[i - 1] == 0
            && data[i - 2] == 0
            && i + 1 < data.len()
            && data[i + 1] < 4
        {
            out.push(data[i + 1]);
            i += 2;
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    out
}

impl SequenceHeader {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(DecoderError::InvalidData(
                "VC-1 sequence header too short".into(),
            ));
        }
        let mut br = BitReader::new(data);
        let profile = match need_bits(&mut br, 2, "PROFILE")? {
            0 => Profile::Simple,
            1 => Profile::Main,
            3 => Profile::Advanced,
            2 => {
                return Err(DecoderError::Unsupported(
                    "WMV3 Complex profile is not supported by the native decoder".into(),
                ));
            }
            _ => unreachable!(),
        };
        if profile == Profile::Advanced {
            return Err(DecoderError::Unsupported(
                "VC-1 Advanced profile is not WMV3 Simple/Main syntax".into(),
            ));
        }

        // FFmpeg ff_vc1_decode_sequence_header(), Simple/Main branch.
        let res_y411 = need_bit(&mut br, "RES_Y411")?;
        let res_sprite = need_bit(&mut br, "RES_SPRITE")?;
        if res_y411 {
            return Err(DecoderError::Unsupported(
                "VC-1 old interlaced Y411 mode is unsupported".into(),
            ));
        }

        let frmrtq_postproc = need_bits(&mut br, 3, "FRMRTQ_POSTPROC")?;
        let _bitrtq_postproc = need_bits(&mut br, 5, "BITRTQ_POSTPROC")?;
        let loop_filter = need_bit(&mut br, "LOOPFILTER")?;
        let res_x8 = need_bit(&mut br, "RES_X8")?;
        let multires = need_bit(&mut br, "MULTIRES")?;
        let res_fasttx = need_bit(&mut br, "RES_FASTTX")?;
        let fastuvmc = need_bit(&mut br, "FASTUVMC")?;
        let extended_mv = need_bit(&mut br, "EXTENDED_MV")?;
        let dquant = need_bits(&mut br, 2, "DQUANT")? as u8;
        let vstransform = need_bit(&mut br, "VSTRANSFORM")?;
        let res_transtab = need_bit(&mut br, "RES_TRANSTAB")?;
        if res_transtab {
            return Err(DecoderError::InvalidData(
                "VC-1 reserved RES_TRANSTAB bit is set".into(),
            ));
        }
        let overlap = need_bit(&mut br, "OVERLAP")?;
        let syncmarker = need_bit(&mut br, "SYNCMARKER")?;
        let rangered = need_bit(&mut br, "RANGERED")?;
        let max_b_frames = need_bits(&mut br, 3, "MAXBFRAMES")? as u8;
        let quantizer_mode = match need_bits(&mut br, 2, "QUANTIZER")? {
            0 => QuantizerMode::Implicit,
            1 => QuantizerMode::Explicit,
            2 => QuantizerMode::NonUniform,
            _ => QuantizerMode::Uniform,
        };
        let finterpflag = need_bit(&mut br, "FINTERPFLAG")?;

        // Sprite streams carry additional fields. Ordinary WMV3 movies don't use
        // this mode, but consume it correctly before rejecting unsupported sprite
        // coding so the parser never silently desynchronizes.

        let res_rtm_flag = if res_sprite {
            let _sprite_w = need_bits(&mut br, 11, "SPRITE_WIDTH")?;
            let _sprite_h = need_bits(&mut br, 11, "SPRITE_HEIGHT")?;
            let _sprite_rate = need_bits(&mut br, 5, "SPRITE_RATE")?;
            let _sprite_x8 = need_bit(&mut br, "SPRITE_X8")?;
            let sprite_dc = need_bit(&mut br, "SPRITE_DC")?;
            if sprite_dc {
                return Err(DecoderError::Unsupported(
                    "VC-1 sprite DC coding is unsupported".into(),
                ));
            }
            let _slice_code = need_bits(&mut br, 3, "SPRITE_SLICE")?;
            false
        } else {
            need_bit(&mut br, "RES_RTM_FLAG")?
        };
        if !res_fasttx {
            let _reserved = need_bits(&mut br, 16, "RES_FASTTX_PAYLOAD")?;
        }

        // FRMRTQ_POSTPROC is not the coded frame rate. Keep a deterministic
        // nominal value for legacy callers; ASF timestamps remain authoritative.
        let (frame_rate_num, frame_rate_den) = match frmrtq_postproc {
            0..=4 => (30, 1),
            5 => (24000, 1001),
            6 => (24, 1),
            _ => (30, 1),
        };

        Ok(Self {
            profile,
            level: 0,
            max_b_frames,
            frame_rate_num,
            frame_rate_den,
            loop_filter,
            multires,
            fastuvmc,
            extended_mv,
            extended_dmv: false,
            dquant,
            vstransform,
            overlap,
            syncmarker,
            rangered,
            quantizer_mode,
            finterpflag,
            res_x8,
            res_fasttx,
            res_rtm_flag,
            res_sprite,
            postprocflag: false,
            broadcast: false,
            interlace: false,
            tfcntrflag: false,
            psf: false,
            panscanflag: false,
            refdist_flag: false,
            broken_link: false,
            closed_entry: false,
            range_mapy: None,
            range_mapuv: None,
            hrd_num_leaky_buckets: 0,
            width: 0,
            height: 0,
            display_width: 0,
            display_height: 0,
        })
    }

    /// Parse VC-1 Advanced Profile private data carried by WVC1.
    ///
    /// WVC1 ASF stream properties contain VC-1 start-code-delimited sequence
    /// and entry-point units (the first byte is commonly the private-data
    /// length). This follows FFmpeg's VC1/WVC1 initialization path: locate the
    /// 0x0000010f sequence header and 0x0000010e entry point, remove VC-1
    /// emulation-prevention bytes from each payload, then parse both units.
    pub fn parse_wvc1(data: &[u8]) -> Result<Self> {
        let mut seq_payload: Option<Vec<u8>> = None;
        let mut entry_payload: Option<Vec<u8>> = None;

        let mut pos = 0usize;
        while let Some(start) = find_vc1_marker(data, pos) {
            let marker = data[start + 3];
            let next = find_vc1_marker(data, start + 4).unwrap_or(data.len());
            if next > start + 4 {
                let payload = vc1_unescape_buffer(&data[start + 4..next]);
                match marker {
                    0x0f => seq_payload = Some(payload),
                    0x0e => entry_payload = Some(payload),
                    _ => {}
                }
            }
            if next >= data.len() {
                break;
            }
            pos = next;
        }

        let seq_payload = seq_payload.ok_or_else(|| {
            DecoderError::InvalidData("WVC1 extradata has no VC-1 sequence header".into())
        })?;
        let entry_payload = entry_payload.ok_or_else(|| {
            DecoderError::InvalidData("WVC1 extradata has no VC-1 entry point".into())
        })?;

        let mut seq = Self::parse_advanced_sequence_payload(&seq_payload)?;
        seq.apply_wvc1_entry_point(&entry_payload)?;
        Ok(seq)
    }

    fn parse_advanced_sequence_payload(data: &[u8]) -> Result<Self> {
        let mut br = BitReader::new(data);
        let profile = match need_bits(&mut br, 2, "PROFILE")? {
            3 => Profile::Advanced,
            0 => Profile::Simple,
            1 => Profile::Main,
            2 => {
                return Err(DecoderError::Unsupported(
                    "WVC1 Complex profile is unsupported".into(),
                ));
            }
            _ => unreachable!(),
        };
        if profile != Profile::Advanced {
            return Err(DecoderError::InvalidData(format!(
                "WVC1 sequence header is not Advanced Profile: {profile:?}"
            )));
        }

        let level = need_bits(&mut br, 3, "LEVEL")? as u8;
        if level >= 5 {
            return Err(DecoderError::InvalidData(format!(
                "VC-1 Advanced reserved LEVEL {level}"
            )));
        }
        let chroma_format = need_bits(&mut br, 2, "CHROMAFORMAT")?;
        if chroma_format != 1 {
            return Err(DecoderError::Unsupported(format!(
                "VC-1 Advanced chroma format {chroma_format} (only 4:2:0 is supported)"
            )));
        }

        let _frmrtq_postproc = need_bits(&mut br, 3, "FRMRTQ_POSTPROC")?;
        let _bitrtq_postproc = need_bits(&mut br, 5, "BITRTQ_POSTPROC")?;
        let postprocflag = need_bit(&mut br, "POSTPROCFLAG")?;
        let width = (need_bits(&mut br, 12, "MAX_CODED_WIDTH")? + 1) << 1;
        let height = (need_bits(&mut br, 12, "MAX_CODED_HEIGHT")? + 1) << 1;
        let broadcast = need_bit(&mut br, "PULLDOWN")?;
        let interlace = need_bit(&mut br, "INTERLACE")?;
        let tfcntrflag = need_bit(&mut br, "TFCNTRFLAG")?;
        let finterpflag = need_bit(&mut br, "FINTERPFLAG")?;
        let _reserved = need_bit(&mut br, "ADV_RESERVED")?;
        let psf = need_bit(&mut br, "PSF")?;
        if psf {
            return Err(DecoderError::Unsupported(
                "VC-1 Progressive Segmented Frame mode is unsupported".into(),
            ));
        }

        let mut display_width = width;
        let mut display_height = height;
        let mut frame_rate_num = 0u32;
        let mut frame_rate_den = 1u32;
        if need_bit(&mut br, "DISPLAY_EXT")? {
            display_width = need_bits(&mut br, 14, "DISP_HORIZ_SIZE")? + 1;
            display_height = need_bits(&mut br, 14, "DISP_VERT_SIZE")? + 1;

            if need_bit(&mut br, "ASPECT_RATIO_FLAG")? {
                let ar = need_bits(&mut br, 4, "ASPECT_RATIO")?;
                if ar == 15 {
                    let _aspect_horiz = need_bits(&mut br, 8, "ASPECT_HORIZ_SIZE")? + 1;
                    let _aspect_vert = need_bits(&mut br, 8, "ASPECT_VERT_SIZE")? + 1;
                }
            }

            if need_bit(&mut br, "FRAMERATE_FLAG")? {
                if need_bit(&mut br, "FRAMERATEIND")? {
                    frame_rate_num = need_bits(&mut br, 16, "FRAMERATEEXP")? + 1;
                    frame_rate_den = 32;
                } else {
                    const FPS_NR: [u32; 7] = [24, 25, 30, 50, 60, 48, 72];
                    const FPS_DR: [u32; 2] = [1000, 1001];
                    let nr = need_bits(&mut br, 8, "FRAMERATENR")? as usize;
                    let dr = need_bits(&mut br, 4, "FRAMERATEDR")? as usize;
                    if (1..=FPS_NR.len()).contains(&nr) && (1..=FPS_DR.len()).contains(&dr) {
                        frame_rate_num = FPS_NR[nr - 1] * 1000;
                        frame_rate_den = FPS_DR[dr - 1];
                    }
                }
            }

            if need_bit(&mut br, "COLOR_FORMAT_FLAG")? {
                let _color_prim = need_bits(&mut br, 8, "COLOR_PRIM")?;
                let _transfer_char = need_bits(&mut br, 8, "TRANSFER_CHAR")?;
                let _matrix_coef = need_bits(&mut br, 8, "MATRIX_COEF")?;
            }
        }

        let hrd_param_flag = need_bit(&mut br, "HRD_PARAM_FLAG")?;
        let hrd_num_leaky_buckets = if hrd_param_flag {
            let buckets = need_bits(&mut br, 5, "HRD_NUM_LEAKY_BUCKETS")? as u8;
            need_bits(&mut br, 4, "BIT_RATE_EXPONENT")?;
            need_bits(&mut br, 4, "BUFFER_SIZE_EXPONENT")?;
            for _ in 0..buckets {
                need_bits(&mut br, 16, "HRD_RATE")?;
                need_bits(&mut br, 16, "HRD_BUFFER")?;
            }
            buckets
        } else {
            0
        };

        Ok(Self {
            profile,
            level,
            max_b_frames: 7,
            frame_rate_num,
            frame_rate_den,
            loop_filter: false,
            multires: false,
            fastuvmc: false,
            extended_mv: false,
            extended_dmv: false,
            dquant: 0,
            vstransform: false,
            overlap: false,
            syncmarker: false,
            rangered: false,
            quantizer_mode: QuantizerMode::Implicit,
            finterpflag,
            res_x8: false,
            res_fasttx: false,
            res_rtm_flag: true,
            res_sprite: false,
            postprocflag,
            broadcast,
            interlace,
            tfcntrflag,
            psf,
            panscanflag: false,
            refdist_flag: false,
            broken_link: false,
            closed_entry: false,
            range_mapy: None,
            range_mapuv: None,
            hrd_num_leaky_buckets,
            width,
            height,
            display_width,
            display_height,
        })
    }

    pub fn apply_wvc1_entry_point(&mut self, data: &[u8]) -> Result<()> {
        let mut br = BitReader::new(data);
        self.broken_link = need_bit(&mut br, "BROKEN_LINK")?;
        self.closed_entry = need_bit(&mut br, "CLOSED_ENTRY")?;
        self.panscanflag = need_bit(&mut br, "PANSCAN_FLAG")?;
        self.refdist_flag = need_bit(&mut br, "REFDIST_FLAG")?;
        self.loop_filter = need_bit(&mut br, "LOOPFILTER")?;
        self.fastuvmc = need_bit(&mut br, "FASTUVMC")?;
        self.extended_mv = need_bit(&mut br, "EXTENDED_MV")?;
        self.dquant = need_bits(&mut br, 2, "DQUANT")? as u8;
        self.vstransform = need_bit(&mut br, "VSTRANSFORM")?;
        self.overlap = need_bit(&mut br, "OVERLAP")?;
        self.quantizer_mode = match need_bits(&mut br, 2, "QUANTIZER")? {
            0 => QuantizerMode::Implicit,
            1 => QuantizerMode::Explicit,
            2 => QuantizerMode::NonUniform,
            _ => QuantizerMode::Uniform,
        };

        for _ in 0..self.hrd_num_leaky_buckets {
            need_bits(&mut br, 8, "HRD_FULL")?;
        }

        if need_bit(&mut br, "CODED_SIZE_FLAG")? {
            self.width = (need_bits(&mut br, 12, "CODED_WIDTH")? + 1) << 1;
            self.height = (need_bits(&mut br, 12, "CODED_HEIGHT")? + 1) << 1;
        }
        self.extended_dmv = self.extended_mv && need_bit(&mut br, "EXTENDED_DMV")?;
        self.range_mapy = if need_bit(&mut br, "RANGE_MAPY_FLAG")? {
            Some(need_bits(&mut br, 3, "RANGE_MAPY")? as u8)
        } else {
            None
        };
        self.range_mapuv = if need_bit(&mut br, "RANGE_MAPUV_FLAG")? {
            Some(need_bits(&mut br, 3, "RANGE_MAPUV")? as u8)
        } else {
            None
        };
        Ok(())
    }

    #[inline]
    pub fn uses_transposed_scans(&self) -> bool {
        self.profile == Profile::Advanced || self.res_fasttx
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BitplaneMode {
    Raw,
    Norm2,
    Diff2,
    Norm6,
    Diff6,
    RowSkip,
    ColSkip,
}

#[derive(Debug, Clone)]
pub struct Bitplane {
    pub data: Vec<u8>,
    pub is_raw: bool,
}

const IMODE_CODES: [u16; 7] = [0, 2, 1, 3, 1, 2, 3];
const IMODE_BITS: [u8; 7] = [4, 2, 3, 2, 4, 3, 3];
const NORM2_CODES: [u16; 4] = [0, 4, 5, 3];
const NORM2_BITS: [u8; 4] = [1, 3, 3, 2];
const NORM6_CODES: [u16; 64] = [
    0x001, 0x002, 0x003, 0x000, 0x004, 0x001, 0x002, 0x047, 0x005, 0x003, 0x004, 0x04B, 0x005,
    0x04D, 0x04E, 0x30E, 0x006, 0x006, 0x007, 0x053, 0x008, 0x055, 0x056, 0x30D, 0x009, 0x059,
    0x05A, 0x30C, 0x05C, 0x30B, 0x30A, 0x037, 0x007, 0x00A, 0x00B, 0x043, 0x00C, 0x045, 0x046,
    0x309, 0x00D, 0x049, 0x04A, 0x308, 0x04C, 0x307, 0x306, 0x036, 0x00E, 0x051, 0x052, 0x305,
    0x054, 0x304, 0x303, 0x035, 0x058, 0x302, 0x301, 0x034, 0x300, 0x033, 0x032, 0x007,
];
const NORM6_BITS: [u8; 64] = [
    1, 4, 4, 8, 4, 8, 8, 10, 4, 8, 8, 10, 8, 10, 10, 13, 4, 8, 8, 10, 8, 10, 10, 13, 8, 10, 10, 13,
    10, 13, 13, 9, 4, 8, 8, 10, 8, 10, 10, 13, 8, 10, 10, 13, 10, 13, 13, 9, 8, 10, 10, 13, 10, 13,
    13, 9, 10, 13, 13, 9, 13, 9, 9, 6,
];

fn decode_prefix(br: &mut BitReader<'_>, codes: &[u16], bits: &[u8]) -> Option<usize> {
    let max_len = bits.iter().copied().max()?;
    for len in 1..=max_len {
        let Some(peek) = br.peek_bits(len) else {
            continue;
        };
        for (sym, (&code, &nb)) in codes.iter().zip(bits.iter()).enumerate() {
            if nb == len && peek == code as u32 {
                br.skip_bits(len);
                return Some(sym);
            }
        }
    }
    None
}

fn decode_rowskip(
    br: &mut BitReader<'_>,
    data: &mut [u8],
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
    stride: usize,
) -> Option<()> {
    for y in 0..height {
        let row = (y0 + y) * stride + x0;
        if br.read_bit()? {
            for x in 0..width {
                data[row + x] = br.read_bit()? as u8;
            }
        } else {
            data[row..row + width].fill(0);
        }
    }
    Some(())
}

fn decode_colskip(
    br: &mut BitReader<'_>,
    data: &mut [u8],
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
    stride: usize,
) -> Option<()> {
    for x in 0..width {
        if br.read_bit()? {
            for y in 0..height {
                data[(y0 + y) * stride + x0 + x] = br.read_bit()? as u8;
            }
        } else {
            for y in 0..height {
                data[(y0 + y) * stride + x0 + x] = 0;
            }
        }
    }
    Some(())
}

impl Bitplane {
    /// Decode one VC-1 bitplane exactly as FFmpeg `bitplane_decoding()`.
    /// RAW mode intentionally consumes no macroblock bits here; those bits are
    /// interleaved in the macroblock layer and are consumed by `decoder.rs`.
    pub fn decode(br: &mut BitReader<'_>, mb_w: usize, mb_h: usize) -> Option<Self> {
        let mut data = vec![0u8; mb_w.checked_mul(mb_h)?];
        let invert = br.read_bit()? as u8;
        let imode = decode_prefix(br, &IMODE_CODES, &IMODE_BITS)?;
        let mode = match imode {
            0 => BitplaneMode::Raw,
            1 => BitplaneMode::Norm2,
            2 => BitplaneMode::Diff2,
            3 => BitplaneMode::Norm6,
            4 => BitplaneMode::Diff6,
            5 => BitplaneMode::RowSkip,
            6 => BitplaneMode::ColSkip,
            _ => return None,
        };

        if mode == BitplaneMode::Raw {
            return Some(Self { data, is_raw: true });
        }

        match mode {
            BitplaneMode::Norm2 | BitplaneMode::Diff2 => {
                let n = mb_w * mb_h;
                let mut pos = 0usize;
                if n & 1 != 0 {
                    data[0] = br.read_bit()? as u8;
                    pos = 1;
                }
                while pos + 1 < n {
                    let code = decode_prefix(br, &NORM2_CODES, &NORM2_BITS)? as u8;
                    data[pos] = code & 1;
                    data[pos + 1] = code >> 1;
                    pos += 2;
                }
            }
            BitplaneMode::Norm6 | BitplaneMode::Diff6 => {
                // FFmpeg uses 2x3 tiles when height is a multiple of 3 and
                // width is not, otherwise 3x2 tiles.
                if mb_h.is_multiple_of(3) && !mb_w.is_multiple_of(3) {
                    for y in (0..mb_h).step_by(3) {
                        for x in ((mb_w & 1)..mb_w).step_by(2) {
                            let code = decode_prefix(br, &NORM6_CODES, &NORM6_BITS)? as u8;
                            data[y * mb_w + x] = code & 1;
                            data[y * mb_w + x + 1] = (code >> 1) & 1;
                            data[(y + 1) * mb_w + x] = (code >> 2) & 1;
                            data[(y + 1) * mb_w + x + 1] = (code >> 3) & 1;
                            data[(y + 2) * mb_w + x] = (code >> 4) & 1;
                            data[(y + 2) * mb_w + x + 1] = (code >> 5) & 1;
                        }
                    }
                    if mb_w & 1 != 0 {
                        decode_colskip(br, &mut data, 0, 0, 1, mb_h, mb_w)?;
                    }
                } else {
                    let y0 = mb_h & 1;
                    for y in (y0..mb_h).step_by(2) {
                        for x in (mb_w % 3..mb_w).step_by(3) {
                            let code = decode_prefix(br, &NORM6_CODES, &NORM6_BITS)? as u8;
                            data[y * mb_w + x] = code & 1;
                            data[y * mb_w + x + 1] = (code >> 1) & 1;
                            data[y * mb_w + x + 2] = (code >> 2) & 1;
                            data[(y + 1) * mb_w + x] = (code >> 3) & 1;
                            data[(y + 1) * mb_w + x + 1] = (code >> 4) & 1;
                            data[(y + 1) * mb_w + x + 2] = (code >> 5) & 1;
                        }
                    }
                    let x = mb_w % 3;
                    if x != 0 {
                        decode_colskip(br, &mut data, 0, 0, x, mb_h, mb_w)?;
                    }
                    if mb_h & 1 != 0 && mb_w > x {
                        decode_rowskip(br, &mut data, x, 0, mb_w - x, 1, mb_w)?;
                    }
                }
            }
            BitplaneMode::RowSkip => {
                decode_rowskip(br, &mut data, 0, 0, mb_w, mb_h, mb_w)?;
            }
            BitplaneMode::ColSkip => {
                decode_colskip(br, &mut data, 0, 0, mb_w, mb_h, mb_w)?;
            }
            BitplaneMode::Raw => unreachable!(),
        }

        if matches!(mode, BitplaneMode::Diff2 | BitplaneMode::Diff6) {
            if !data.is_empty() {
                data[0] ^= invert;
                for x in 1..mb_w {
                    data[x] ^= data[x - 1];
                }
                for y in 1..mb_h {
                    let row = y * mb_w;
                    let prev = row - mb_w;
                    data[row] ^= data[prev];
                    for x in 1..mb_w {
                        let left = data[row + x - 1];
                        let above = data[prev + x];
                        if left != above {
                            data[row + x] ^= invert;
                        } else {
                            data[row + x] ^= left;
                        }
                    }
                }
            }
        } else if invert != 0 {
            for v in &mut data {
                *v = if *v == 0 { 1 } else { 0 };
            }
        }

        Some(Self {
            data,
            is_raw: false,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DQuantInfo {
    pub enabled: bool,
    pub profile: u8,
    pub edge: u8,
    pub bi_level: bool,
    pub alt_pquant: u8,
}

#[derive(Debug, Clone)]
pub struct PictureHeader {
    pub frame_type: FrameType,
    pub pqindex: u8,
    pub pquant: u8,
    pub halfqp: bool,
    /// FFmpeg's pquantizer: true = uniform quantizer reconstruction.
    pub pqual_mode: u8,
    pub mvrange: u8,
    pub rptfrm: u8,
    pub pts_ms: u32,
    pub rangeredfrm: bool,
    pub rnd: Option<bool>,
    pub header_bits: usize,
    pub acpred_plane: Option<Vec<u8>>,
    pub acpred_raw: bool,
    pub condover: u8,
    pub overflags_plane: Option<Vec<u8>>,
    pub overflags_raw: bool,
    pub skipmb_plane: Option<Vec<u8>>,
    pub skipmb_raw: bool,
    pub directmb_plane: Option<Vec<u8>>,
    pub directmb_raw: bool,
    pub mvtypemb_plane: Option<Vec<u8>>,
    pub mvtypemb_raw: bool,
    pub bfrac_num: i32,
    pub bfrac_den: i32,
    pub mv_mode: MvMode,
    pub mv_mode2: MvMode,
    pub lumscale: u8,
    pub lumshift: u8,
    pub mvtab: u8,
    pub cbptab: u8,
    pub ttmbf: bool,
    pub ttfrm: u8,
    pub transacfrm: u8,
    pub transacfrm2: u8,
    pub dctab: bool,
    pub dquant: DQuantInfo,
}

const PQUANT_IMPLICIT: [u8; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
    24, 25, 27, 29, 31,
];
const BFRAC: [(i32, i32); 23] = [
    (1, 2),
    (1, 3),
    (2, 3),
    (1, 4),
    (3, 4),
    (1, 5),
    (2, 5),
    (3, 5),
    (4, 5),
    (1, 6),
    (5, 6),
    (1, 7),
    (2, 7),
    (3, 7),
    (4, 7),
    (5, 7),
    (6, 7),
    (1, 8),
    (3, 8),
    (5, 8),
    (7, 8),
    (0, 0),
    (0, 1),
];
const MV_PMODE: [[MvMode; 5]; 2] = [
    [
        MvMode::OneMvHpelBilin,
        MvMode::OneMv,
        MvMode::OneMvHpel,
        MvMode::IntensityComp,
        MvMode::MixedMv,
    ],
    [
        MvMode::OneMv,
        MvMode::MixedMv,
        MvMode::OneMvHpel,
        MvMode::IntensityComp,
        MvMode::OneMvHpelBilin,
    ],
];
const MV_PMODE2: [[MvMode; 4]; 2] = [
    [
        MvMode::OneMvHpelBilin,
        MvMode::OneMv,
        MvMode::OneMvHpel,
        MvMode::MixedMv,
    ],
    [
        MvMode::OneMv,
        MvMode::MixedMv,
        MvMode::OneMvHpel,
        MvMode::OneMvHpelBilin,
    ],
];

impl PictureHeader {
    pub fn parse(
        data: &[u8],
        seq: &SequenceHeader,
        pts_ms: u32,
        mb_w: usize,
        mb_h: usize,
    ) -> Result<Self> {
        if seq.profile == Profile::Advanced {
            if seq.interlace {
                return Err(DecoderError::Unsupported(
                    "VC-1 Advanced interlaced pictures are not yet supported by the native decoder"
                        .into(),
                ));
            }
            return Self::parse_advanced_progressive(data, seq, pts_ms, mb_w, mb_h);
        }

        let mut br = BitReader::new(data);

        if seq.finterpflag {
            let _interpfrm = need_bit(&mut br, "INTERPFRM")?;
        }
        need_bits(&mut br, 2, "FRMCNT")?;
        let rangeredfrm = seq.rangered && need_bit(&mut br, "RANGEREDFRM")?;

        let mut frame_type = if need_bit(&mut br, "PTYPE")? {
            FrameType::P
        } else if seq.max_b_frames > 0 && !need_bit(&mut br, "PTYPE_B")? {
            FrameType::B
        } else {
            FrameType::I
        };

        let mut bfrac_num = 1;
        let mut bfrac_den = 2;
        if frame_type == FrameType::B {
            let mut idx = need_bits(&mut br, 3, "BFRACTION")? as usize;
            if idx == 7 {
                idx = 7 + need_bits(&mut br, 4, "BFRACTION_EXT")? as usize;
            }
            if idx >= BFRAC.len() || idx == 21 {
                return Err(DecoderError::InvalidData("invalid VC-1 BFRACTION".into()));
            }
            let frac = BFRAC[idx];
            if idx == 22 {
                frame_type = FrameType::BI;
                bfrac_num = 0;
                bfrac_den = 1;
            } else {
                bfrac_num = frac.0;
                bfrac_den = frac.1;
            }
        }

        if matches!(frame_type, FrameType::I | FrameType::BI) {
            need_bits(&mut br, 7, "BF")?;
        }

        let pqindex = need_bits(&mut br, 5, "PQINDEX")? as u8;
        if pqindex == 0 {
            return Err(DecoderError::InvalidData("VC-1 PQINDEX is zero".into()));
        }
        let pquant = match seq.quantizer_mode {
            QuantizerMode::Implicit => PQUANT_IMPLICIT[pqindex as usize],
            _ => pqindex,
        };
        let mut halfqp = if pqindex < 9 {
            need_bit(&mut br, "HALFQP")?
        } else {
            false
        };
        let pquantizer = match seq.quantizer_mode {
            QuantizerMode::Implicit => pqindex < 9,
            QuantizerMode::NonUniform => false,
            QuantizerMode::Explicit => need_bit(&mut br, "PQUANTIZER")?,
            QuantizerMode::Uniform => true,
        };

        let mvrange = if seq.extended_mv {
            read_unary(&mut br, false, 3, "MVRANGE")?
        } else {
            0
        };
        if seq.multires && frame_type != FrameType::B {
            let _respic = need_bits(&mut br, 2, "RESPIC")?;
        }
        if seq.res_x8 && matches!(frame_type, FrameType::I | FrameType::BI) {
            let x8_type = need_bit(&mut br, "X8_TYPE")?;
            if x8_type {
                return Err(DecoderError::Unsupported("VC-1 X8 intra coding".into()));
            }
        }

        let mut skipmb_plane = None;
        let mut skipmb_raw = false;
        let mut directmb_plane = None;
        let mut directmb_raw = false;
        let mut mvtypemb_plane = None;
        let mut mvtypemb_raw = false;
        let mut mv_mode = MvMode::OneMv;
        let mut mv_mode2 = MvMode::OneMv;
        let mut lumscale = 32u8;
        let mut lumshift = 0u8;
        let mut mvtab = 0u8;
        let mut cbptab = 0u8;
        let mut ttmbf = true;
        let mut ttfrm = 0u8;
        let mut dq = DQuantInfo::default();

        match frame_type {
            FrameType::P => {
                let lowquant = if pquant > 12 { 0usize } else { 1usize };
                let mode_idx = read_unary(&mut br, true, 4, "MVMODE")? as usize;
                mv_mode = MV_PMODE[lowquant][mode_idx.min(4)];
                if mv_mode == MvMode::IntensityComp {
                    let mode2_idx = read_unary(&mut br, true, 3, "MVMODE2")? as usize;
                    mv_mode2 = MV_PMODE2[lowquant][mode2_idx.min(3)];
                    lumscale = need_bits(&mut br, 6, "LUMSCALE")? as u8;
                    lumshift = need_bits(&mut br, 6, "LUMSHIFT")? as u8;
                } else {
                    mv_mode2 = mv_mode;
                }

                if mv_mode == MvMode::MixedMv
                    || (mv_mode == MvMode::IntensityComp && mv_mode2 == MvMode::MixedMv)
                {
                    let bp = Bitplane::decode(&mut br, mb_w, mb_h).ok_or_else(|| {
                        DecoderError::InvalidData("invalid VC-1 MVTYPE bitplane".into())
                    })?;
                    mvtypemb_raw = bp.is_raw;
                    if !bp.is_raw {
                        mvtypemb_plane = Some(bp.data);
                    }
                }
                let bp = Bitplane::decode(&mut br, mb_w, mb_h).ok_or_else(|| {
                    DecoderError::InvalidData("invalid VC-1 SKIPMB bitplane".into())
                })?;
                skipmb_raw = bp.is_raw;
                if !bp.is_raw {
                    skipmb_plane = Some(bp.data);
                }

                mvtab = need_bits(&mut br, 2, "MVTAB")? as u8;
                cbptab = need_bits(&mut br, 2, "CBPTAB")? as u8;
                if seq.dquant != 0 {
                    dq = parse_dquant(&mut br, seq.dquant, pquant)?;
                    // VC-1 §8.1.1.8 / FFmpeg vop_dquant_decoding(): ALL_MBS with
                    // DQBILEVEL=0 disables HALFQP for the picture.
                    if dq.enabled && dq.profile == 3 && !dq.bi_level {
                        halfqp = false;
                    }
                }
                if seq.vstransform {
                    ttmbf = need_bit(&mut br, "TTMBF")?;
                    ttfrm = if ttmbf {
                        [0u8, 3, 6, 7][need_bits(&mut br, 2, "TTFRM")? as usize]
                    } else {
                        0
                    };
                }
            }
            FrameType::B => {
                mv_mode = if need_bit(&mut br, "MVMODE_B")? {
                    MvMode::OneMv
                } else {
                    MvMode::OneMvHpelBilin
                };
                mv_mode2 = mv_mode;
                let direct = Bitplane::decode(&mut br, mb_w, mb_h).ok_or_else(|| {
                    DecoderError::InvalidData("invalid VC-1 DIRECTMB bitplane".into())
                })?;
                directmb_raw = direct.is_raw;
                if !direct.is_raw {
                    directmb_plane = Some(direct.data);
                }
                let skip = Bitplane::decode(&mut br, mb_w, mb_h).ok_or_else(|| {
                    DecoderError::InvalidData("invalid VC-1 SKIPMB bitplane".into())
                })?;
                skipmb_raw = skip.is_raw;
                if !skip.is_raw {
                    skipmb_plane = Some(skip.data);
                }
                mvtab = need_bits(&mut br, 2, "MVTAB")? as u8;
                cbptab = need_bits(&mut br, 2, "CBPTAB")? as u8;
                if seq.dquant != 0 {
                    dq = parse_dquant(&mut br, seq.dquant, pquant)?;
                    // VC-1 §8.1.1.8 / FFmpeg vop_dquant_decoding(): ALL_MBS with
                    // DQBILEVEL=0 disables HALFQP for the picture.
                    if dq.enabled && dq.profile == 3 && !dq.bi_level {
                        halfqp = false;
                    }
                }
                if seq.vstransform {
                    ttmbf = need_bit(&mut br, "TTMBF")?;
                    ttfrm = if ttmbf {
                        [0u8, 3, 6, 7][need_bits(&mut br, 2, "TTFRM")? as usize]
                    } else {
                        0
                    };
                }
            }
            FrameType::I | FrameType::BI | FrameType::Skipped => {}
        }

        // AC/DC table syntax. decode012 produces 0,1,2.
        let transacfrm = decode012(&mut br, "TRANSACFRM")?;
        let transacfrm2 = if matches!(frame_type, FrameType::I | FrameType::BI) {
            decode012(&mut br, "TRANSACFRM2")?
        } else {
            transacfrm
        };
        let dctab = need_bit(&mut br, "DCTAB")?;

        Ok(Self {
            frame_type,
            pqindex,
            pquant,
            halfqp,
            pqual_mode: pquantizer as u8,
            mvrange,
            rptfrm: 0,
            pts_ms,
            rangeredfrm,
            rnd: None,
            header_bits: br.bits_read(),
            acpred_plane: None,
            acpred_raw: false,
            condover: 0,
            overflags_plane: None,
            overflags_raw: false,
            skipmb_plane,
            skipmb_raw,
            directmb_plane,
            directmb_raw,
            mvtypemb_plane,
            mvtypemb_raw,
            bfrac_num,
            bfrac_den,
            mv_mode,
            mv_mode2,
            lumscale,
            lumshift,
            mvtab,
            cbptab,
            ttmbf,
            ttfrm,
            transacfrm,
            transacfrm2,
            dctab,
            dquant: dq,
        })
    }

    fn parse_advanced_progressive(
        data: &[u8],
        seq: &SequenceHeader,
        pts_ms: u32,
        mb_w: usize,
        mb_h: usize,
    ) -> Result<Self> {
        let mut br = BitReader::new(data);

        let ptype = read_unary(&mut br, false, 4, "PTYPE")?;
        let mut frame_type = match ptype {
            0 => FrameType::P,
            1 => FrameType::B,
            2 => FrameType::I,
            3 => FrameType::BI,
            4 => FrameType::Skipped,
            _ => unreachable!(),
        };

        if seq.tfcntrflag {
            need_bits(&mut br, 8, "TFCNTR")?;
        }
        let rptfrm = if seq.broadcast {
            need_bits(&mut br, 2, "RPTFRM")? as u8
        } else {
            0
        };
        if seq.panscanflag {
            return Err(DecoderError::Unsupported(
                "VC-1 Advanced pan-scan picture syntax is unsupported".into(),
            ));
        }

        if frame_type == FrameType::Skipped {
            return Ok(Self {
                frame_type,
                pqindex: 1,
                pquant: 1,
                halfqp: false,
                pqual_mode: 1,
                mvrange: 0,
                rptfrm,
                pts_ms,
                rangeredfrm: false,
                rnd: None,
                header_bits: br.bits_read(),
                acpred_plane: None,
                acpred_raw: false,
                condover: 0,
                overflags_plane: None,
                overflags_raw: false,
                skipmb_plane: None,
                skipmb_raw: false,
                directmb_plane: None,
                directmb_raw: false,
                mvtypemb_plane: None,
                mvtypemb_raw: false,
                bfrac_num: 1,
                bfrac_den: 2,
                mv_mode: MvMode::OneMv,
                mv_mode2: MvMode::OneMv,
                lumscale: 32,
                lumshift: 0,
                mvtab: 0,
                cbptab: 0,
                ttmbf: true,
                ttfrm: 0,
                transacfrm: 0,
                transacfrm2: 0,
                dctab: false,
                dquant: DQuantInfo::default(),
            });
        }

        let rnd = need_bit(&mut br, "RNDCTRL")?;
        if seq.finterpflag {
            need_bit(&mut br, "INTERPFRM")?;
        }

        let mut bfrac_num = 1;
        let mut bfrac_den = 2;
        if frame_type == FrameType::B {
            let mut idx = need_bits(&mut br, 3, "BFRACTION")? as usize;
            if idx == 7 {
                idx = 7 + need_bits(&mut br, 4, "BFRACTION_EXT")? as usize;
            }
            if idx >= BFRAC.len() || idx == 21 {
                return Err(DecoderError::InvalidData("invalid VC-1 BFRACTION".into()));
            }
            if idx == 22 {
                frame_type = FrameType::BI;
                bfrac_num = 0;
                bfrac_den = 1;
            } else {
                (bfrac_num, bfrac_den) = BFRAC[idx];
            }
        }

        let pqindex = need_bits(&mut br, 5, "PQINDEX")? as u8;
        if pqindex == 0 {
            return Err(DecoderError::InvalidData("VC-1 PQINDEX is zero".into()));
        }
        let pquant = match seq.quantizer_mode {
            QuantizerMode::Implicit => PQUANT_IMPLICIT[pqindex as usize],
            _ => pqindex,
        };
        let mut halfqp = if pqindex < 9 {
            need_bit(&mut br, "HALFQP")?
        } else {
            false
        };
        let pquantizer = match seq.quantizer_mode {
            QuantizerMode::Implicit => pqindex < 9,
            QuantizerMode::NonUniform => false,
            QuantizerMode::Explicit => need_bit(&mut br, "PQUANTIZER")?,
            QuantizerMode::Uniform => true,
        };
        if seq.postprocflag {
            need_bits(&mut br, 2, "POSTPROC")?;
        }

        let mut acpred_plane = None;
        let mut acpred_raw = false;
        let mut condover = 0u8;
        let mut overflags_plane = None;
        let mut overflags_raw = false;
        let mut skipmb_plane = None;
        let mut skipmb_raw = false;
        let mut directmb_plane = None;
        let mut directmb_raw = false;
        let mut mvtypemb_plane = None;
        let mut mvtypemb_raw = false;
        let mut mv_mode = MvMode::OneMv;
        let mut mv_mode2 = MvMode::OneMv;
        let mut lumscale = 32u8;
        let mut lumshift = 0u8;
        let mut mvtab = 0u8;
        let mut cbptab = 0u8;
        let mut mvrange = 0u8;
        let mut ttmbf = true;
        let mut ttfrm = 0u8;
        let mut dq = DQuantInfo::default();

        match frame_type {
            FrameType::I | FrameType::BI => {
                let bp = Bitplane::decode(&mut br, mb_w, mb_h).ok_or_else(|| {
                    DecoderError::InvalidData("invalid VC-1 Advanced ACPRED bitplane".into())
                })?;
                acpred_raw = bp.is_raw;
                if !bp.is_raw {
                    acpred_plane = Some(bp.data);
                }

                if seq.overlap && pquant <= 8 {
                    condover = decode012(&mut br, "CONDOVER")?;
                    if condover == 2 {
                        let over = Bitplane::decode(&mut br, mb_w, mb_h).ok_or_else(|| {
                            DecoderError::InvalidData(
                                "invalid VC-1 Advanced OVERFLAGS bitplane".into(),
                            )
                        })?;
                        overflags_raw = over.is_raw;
                        if !over.is_raw {
                            overflags_plane = Some(over.data);
                        }
                    }
                }
            }
            FrameType::P => {
                if seq.extended_mv {
                    mvrange = read_unary(&mut br, false, 3, "MVRANGE")?;
                }
                let lowquant = if pquant > 12 { 0usize } else { 1usize };
                let mode_idx = read_unary(&mut br, true, 4, "MVMODE")? as usize;
                mv_mode = MV_PMODE[lowquant][mode_idx.min(4)];
                if mv_mode == MvMode::IntensityComp {
                    let mode2_idx = read_unary(&mut br, true, 3, "MVMODE2")? as usize;
                    mv_mode2 = MV_PMODE2[lowquant][mode2_idx.min(3)];
                    lumscale = need_bits(&mut br, 6, "LUMSCALE")? as u8;
                    lumshift = need_bits(&mut br, 6, "LUMSHIFT")? as u8;
                } else {
                    mv_mode2 = mv_mode;
                }

                if mv_mode == MvMode::MixedMv
                    || (mv_mode == MvMode::IntensityComp && mv_mode2 == MvMode::MixedMv)
                {
                    let bp = Bitplane::decode(&mut br, mb_w, mb_h).ok_or_else(|| {
                        DecoderError::InvalidData("invalid VC-1 MVTYPE bitplane".into())
                    })?;
                    mvtypemb_raw = bp.is_raw;
                    if !bp.is_raw {
                        mvtypemb_plane = Some(bp.data);
                    }
                }
                let skip = Bitplane::decode(&mut br, mb_w, mb_h).ok_or_else(|| {
                    DecoderError::InvalidData("invalid VC-1 SKIPMB bitplane".into())
                })?;
                skipmb_raw = skip.is_raw;
                if !skip.is_raw {
                    skipmb_plane = Some(skip.data);
                }
                mvtab = need_bits(&mut br, 2, "MVTAB")? as u8;
                cbptab = need_bits(&mut br, 2, "CBPTAB")? as u8;
                if seq.dquant != 0 {
                    dq = parse_dquant(&mut br, seq.dquant, pquant)?;
                    if dq.enabled && dq.profile == 3 && !dq.bi_level {
                        halfqp = false;
                    }
                }
                if seq.vstransform {
                    ttmbf = need_bit(&mut br, "TTMBF")?;
                    ttfrm = if ttmbf {
                        [0u8, 3, 6, 7][need_bits(&mut br, 2, "TTFRM")? as usize]
                    } else {
                        0
                    };
                }
            }
            FrameType::B => {
                if seq.extended_mv {
                    mvrange = read_unary(&mut br, false, 3, "MVRANGE")?;
                }
                mv_mode = if need_bit(&mut br, "MVMODE_B")? {
                    MvMode::OneMv
                } else {
                    MvMode::OneMvHpelBilin
                };
                mv_mode2 = mv_mode;
                let direct = Bitplane::decode(&mut br, mb_w, mb_h).ok_or_else(|| {
                    DecoderError::InvalidData("invalid VC-1 DIRECTMB bitplane".into())
                })?;
                directmb_raw = direct.is_raw;
                if !direct.is_raw {
                    directmb_plane = Some(direct.data);
                }
                let skip = Bitplane::decode(&mut br, mb_w, mb_h).ok_or_else(|| {
                    DecoderError::InvalidData("invalid VC-1 SKIPMB bitplane".into())
                })?;
                skipmb_raw = skip.is_raw;
                if !skip.is_raw {
                    skipmb_plane = Some(skip.data);
                }
                mvtab = need_bits(&mut br, 2, "MVTAB")? as u8;
                cbptab = need_bits(&mut br, 2, "CBPTAB")? as u8;
                if seq.dquant != 0 {
                    dq = parse_dquant(&mut br, seq.dquant, pquant)?;
                    if dq.enabled && dq.profile == 3 && !dq.bi_level {
                        halfqp = false;
                    }
                }
                if seq.vstransform {
                    ttmbf = need_bit(&mut br, "TTMBF")?;
                    ttfrm = if ttmbf {
                        [0u8, 3, 6, 7][need_bits(&mut br, 2, "TTFRM")? as usize]
                    } else {
                        0
                    };
                }
            }
            FrameType::Skipped => unreachable!(),
        }

        let transacfrm = decode012(&mut br, "TRANSACFRM")?;
        let transacfrm2 = if matches!(frame_type, FrameType::I | FrameType::BI) {
            decode012(&mut br, "TRANSACFRM2")?
        } else {
            transacfrm
        };
        let dctab = need_bit(&mut br, "DCTAB")?;

        // Advanced I/BI pictures carry VOP DQuant after AC/DC table syntax.
        if matches!(frame_type, FrameType::I | FrameType::BI) && seq.dquant != 0 {
            dq = parse_dquant(&mut br, seq.dquant, pquant)?;
            if dq.enabled && dq.profile == 3 && !dq.bi_level {
                halfqp = false;
            }
        }

        Ok(Self {
            frame_type,
            pqindex,
            pquant,
            halfqp,
            pqual_mode: pquantizer as u8,
            mvrange,
            rptfrm,
            pts_ms,
            rangeredfrm: false,
            rnd: Some(rnd),
            header_bits: br.bits_read(),
            acpred_plane,
            acpred_raw,
            condover,
            overflags_plane,
            overflags_raw,
            skipmb_plane,
            skipmb_raw,
            directmb_plane,
            directmb_raw,
            mvtypemb_plane,
            mvtypemb_raw,
            bfrac_num,
            bfrac_den,
            mv_mode,
            mv_mode2,
            lumscale,
            lumshift,
            mvtab,
            cbptab,
            ttmbf,
            ttfrm,
            transacfrm,
            transacfrm2,
            dctab,
            dquant: dq,
        })
    }

    pub fn parse_simple(data: &[u8], seq: &SequenceHeader, pts_ms: u32) -> Result<Self> {
        let mb_w = seq.width.div_ceil(16).max(1) as usize;
        let mb_h = seq.height.div_ceil(16).max(1) as usize;
        Self::parse(data, seq, pts_ms, mb_w, mb_h)
    }
}

fn parse_dquant(br: &mut BitReader<'_>, dquant: u8, pquant: u8) -> Result<DQuantInfo> {
    let mut out = DQuantInfo::default();
    if dquant != 2 {
        out.enabled = need_bit(br, "DQUANTFRM")?;
        if !out.enabled {
            return Ok(out);
        }
        out.profile = need_bits(br, 2, "DQPROFILE")? as u8;
        // FFmpeg enum values: SINGLE_EDGE=0, DOUBLE_EDGES=1, ALL_MBS=3.
        match out.profile {
            0 | 1 => out.edge = need_bits(br, 2, "DQSBEDGE")? as u8,
            3 => {
                out.bi_level = need_bit(br, "DQBILEVEL")?;
                if !out.bi_level {
                    return Ok(out);
                }
            }
            _ => {}
        }
    }
    // DQUANT == 2 still carries PQDIFF/ABSPQ, but FFmpeg leaves
    // dquantfrm clear. Do not turn it into per-macroblock MQUANT syntax.
    let pqdiff = need_bits(br, 3, "PQDIFF")? as u8;
    out.alt_pquant = if pqdiff == 7 {
        need_bits(br, 5, "ABSPQ")? as u8
    } else {
        // Match vop_dquant_decoding(): ALTPQUANT is not clamped here.
        // GET_MQUANT handles out-of-range macroblock values later.
        pquant + pqdiff + 1
    };
    Ok(out)
}

fn decode012(br: &mut BitReader<'_>, what: &str) -> Result<u8> {
    if !need_bit(br, what)? {
        Ok(0)
    } else {
        Ok(if need_bit(br, what)? { 2 } else { 1 })
    }
}

fn read_unary(br: &mut BitReader<'_>, stop: bool, max: u8, what: &str) -> Result<u8> {
    let mut n = 0u8;
    while n < max {
        let bit = need_bit(br, what)?;
        if bit == stop {
            break;
        }
        n += 1;
    }
    Ok(n)
}

fn need_bit(br: &mut BitReader<'_>, what: &str) -> Result<bool> {
    br.read_bit()
        .ok_or_else(|| DecoderError::InvalidData(format!("truncated VC-1 {what}")))
}

fn need_bits(br: &mut BitReader<'_>, n: u8, what: &str) -> Result<u32> {
    br.read_bits(n)
        .ok_or_else(|| DecoderError::InvalidData(format!("truncated VC-1 {what}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imode_prefixes_are_unique() {
        for i in 0..IMODE_CODES.len() {
            for j in i + 1..IMODE_CODES.len() {
                let a_len = IMODE_BITS[i];
                let b_len = IMODE_BITS[j];
                let min = a_len.min(b_len);
                let a = IMODE_CODES[i] >> (a_len - min);
                let b = IMODE_CODES[j] >> (b_len - min);
                assert_ne!(a, b, "IMODE prefix collision: {i} vs {j}");
            }
        }
    }

    #[test]
    fn bfraction_has_extended_entries() {
        assert_eq!(BFRAC[0], (1, 2));
        assert_eq!(BFRAC[20], (7, 8));
        assert_eq!(BFRAC[22], (0, 1));
    }

    #[test]
    fn wvc1_unescape_matches_vc1_emulation_prevention() {
        assert_eq!(
            vc1_unescape_buffer(&[0x12, 0x00, 0x00, 0x03, 0x01, 0x34]),
            vec![0x12, 0x00, 0x00, 0x01, 0x34]
        );
    }

    #[test]
    fn parses_anemoi_advanced_sequence_and_first_i_header() {
        // WVC1 BITMAPINFO private data from anemoi_op.wmv. It contains an
        // Advanced sequence header (0x0f) followed by the entry point (0x0e).
        let extradata = [
            0x27, 0x00, 0x00, 0x01, 0x0f, 0xdb, 0xc0, 0x3b, 0xf2, 0x1b, 0x8a, 0x3b, 0xf8, 0x86,
            0xe8, 0x0c, 0x88, 0x00, 0x00, 0x01, 0x0e, 0x1a, 0x40, 0x40,
        ];
        let seq = SequenceHeader::parse_wvc1(&extradata).unwrap();
        assert_eq!(seq.profile, Profile::Advanced);
        assert_eq!(seq.level, 3);
        assert_eq!((seq.width, seq.height), (1920, 1080));
        assert_eq!((seq.display_width, seq.display_height), (1920, 1080));
        assert_eq!((seq.frame_rate_num, seq.frame_rate_den), (60_000, 1001));
        assert!(!seq.interlace);
        assert!(seq.broadcast);
        assert!(seq.loop_filter);
        assert!(seq.extended_mv);
        assert!(seq.vstransform);
        assert!(!seq.overlap);
        assert_eq!(seq.dquant, 0);

        // Escaped prefix of the first ASF video media object. Its complete
        // Advanced picture header is 87 bits; the remaining bits begin the
        // first I-picture macroblock layer.
        let escaped = [
            0xc0, 0x82, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03,
            0x01, 0x06,
        ];
        let frame = vc1_unescape_buffer(&escaped);
        let pic = PictureHeader::parse(&frame, &seq, 0, 120, 68).unwrap();
        assert_eq!(pic.frame_type, FrameType::I);
        assert_eq!(pic.pqindex, 4);
        assert_eq!(pic.pquant, 4);
        assert_eq!(pic.rnd, Some(false));
        assert_eq!(pic.header_bits, 87);
        assert!(!pic.acpred_raw);
        assert!(pic.acpred_plane.is_some());

        // The following one-byte ASF video object is the next coded picture
        // in anemoi_op.wmv. Advanced Profile encodes a skipped P picture as
        // PTYPE=1111 followed only by the two-bit RPTFRM field here.
        let skipped = PictureHeader::parse(&[0xf2], &seq, 33, 120, 68).unwrap();
        assert_eq!(skipped.frame_type, FrameType::Skipped);
        assert_eq!(skipped.header_bits, 6);
    }
}

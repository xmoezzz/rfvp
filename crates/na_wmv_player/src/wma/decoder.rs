use std::f32;

use crate::asf::AudioStreamInfo;
use crate::error::{DecoderError, Result};

use super::bitstream::GetBitContext;
use super::common::ff_wma_get_frame_len_bits;
use super::mdct::MdctNaive;
use super::tables;
use super::vlc::{ff_vlc_init_from_lengths, ff_vlc_init_sparse, get_vlc2, Vlc, VlcElem};

const BLOCK_MIN_BITS: i32 = 7;
const BLOCK_MAX_BITS: i32 = 11;
const BLOCK_MAX_SIZE: usize = 1 << BLOCK_MAX_BITS;
const BLOCK_NB_SIZES: usize = (BLOCK_MAX_BITS - BLOCK_MIN_BITS + 1) as usize;

const HIGH_BAND_MAX_SIZE: usize = 16;
const NB_LSP_COEFS: usize = 10;

const MAX_CODED_SUPERFRAME_SIZE: usize = 32768;
const MAX_CHANNELS: usize = 2;

const NOISE_TAB_SIZE: usize = 8192;
const LSP_POW_BITS: usize = 7;

const VLCBITS: i32 = 9;
const VLCMAX: i32 = (22 + VLCBITS - 1) / VLCBITS;

const EXPVLCBITS: i32 = 8;
const EXPMAX: i32 = (19 + EXPVLCBITS - 1) / EXPVLCBITS;

const HGAINVLCBITS: i32 = 9;
const HGAINMAX: i32 = (13 + HGAINVLCBITS - 1) / HGAINVLCBITS;

/// A decoded PCM chunk.
#[derive(Debug, Clone)]
pub struct PcmFrameF32 {
    pub pts_ms: u32,
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved samples.
    pub samples: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
enum WmaVersion {
    V1,
    V2,
}

impl WmaVersion {
    fn id(&self) -> i32 {
        match self {
            WmaVersion::V1 => 1,
            WmaVersion::V2 => 2,
        }
    }
}

/// Direct translation of upstream `WMACodecContext` for WMAv1/2.
pub struct WmaDecoder {
    version: WmaVersion,

    channels: usize,
    sample_rate: u32,
    bit_rate: u32,
    block_align: u16,

    // Flags derived from `flags2`.
    use_exp_vlc: bool,
    use_bit_reservoir: bool,
    use_variable_block_len: bool,
    use_noise_coding: bool,

    byte_offset_bits: i32,

    // VLC tables.
    exp_vlc: Vlc,
    hgain_vlc: Vlc,
    coef_vlc: [Vlc; 2],
    run_table: [Vec<u16>; 2],
    level_table: [Vec<f32>; 2],

    // Frame / block config.
    frame_len_bits: i32,
    frame_len: usize,
    nb_block_sizes: usize,

    reset_block_lengths: bool,
    block_len_bits: i32,
    next_block_len_bits: i32,
    prev_block_len_bits: i32,
    block_len: usize,
    block_num: i32,
    block_pos: usize,

    ms_stereo: bool,
    channel_coded: [bool; MAX_CHANNELS],

    // Exponent bands.
    exponent_sizes: [usize; BLOCK_NB_SIZES],
    exponent_bands: [[u16; 25]; BLOCK_NB_SIZES],
    high_band_start: [usize; BLOCK_NB_SIZES],
    coefs_start: usize,
    coefs_end: [usize; BLOCK_NB_SIZES],
    exponent_high_sizes: [usize; BLOCK_NB_SIZES],
    exponent_high_bands: [[u16; HIGH_BAND_MAX_SIZE]; BLOCK_NB_SIZES],

    high_band_coded: [[bool; HIGH_BAND_MAX_SIZE]; MAX_CHANNELS],
    high_band_values: [[i32; HIGH_BAND_MAX_SIZE]; MAX_CHANNELS],

    // Exponents and coefficients.
    exponents_bsize: [usize; MAX_CHANNELS],
    exponents: [Vec<f32>; MAX_CHANNELS],
    max_exponent: [f32; MAX_CHANNELS],
    coefs1: [Vec<f32>; MAX_CHANNELS],
    coefs: [Vec<f32>; MAX_CHANNELS],

    // MDCT.
    mdct: Vec<MdctNaive>,
    windows: Vec<Vec<f32>>, // per block-size: half window of length block_len
    output: Vec<f32>,       // 2*BLOCK_MAX_SIZE
    frame_out: [Vec<f32>; MAX_CHANNELS],

    // Bit reservoir.
    last_superframe: Vec<u8>,
    last_bitoffset: usize,
    last_superframe_len: usize,
    eof_done: bool,

    // Noise.
    noise_table: Vec<f32>,
    noise_index: usize,
    noise_mult: f32,

    // LSP to curve.
    lsp_cos_table: Vec<f32>,
    lsp_pow_e_table: [f32; 256],
    lsp_pow_m_table1: [f32; 1 << LSP_POW_BITS],
    lsp_pow_m_table2: [f32; 1 << LSP_POW_BITS],

    exponents_initialized: [bool; MAX_CHANNELS],
}

fn ilog2_u32(x: u32) -> i32 {
    31 - (x.leading_zeros() as i32)
}

fn ff_exp10f(x: f32) -> f32 {
    // ff_exp10f(x) = exp2f(M_LOG2_10 * x)
    (std::f32::consts::LOG2_10 * x).exp2()
}

fn sine_window_init(n: usize) -> Vec<f32> {
    // Translated from ff_sine_window_init.
    let mut w = vec![0f32; n];
    let den = 2.0f32 * n as f32;
    for i in 0..n {
        w[i] = ((i as f32 + 0.5) * (std::f32::consts::PI / den)).sin();
    }
    w
}


fn vector_fmul_reverse(dst: &mut [f32], src0: &[f32], win: &[f32]) {
    let len = dst.len();
    for i in 0..len {
        dst[i] = src0[i] * win[len - 1 - i];
    }
}

fn butterflies_float(v1: &mut [f32], v2: &mut [f32]) {
    for i in 0..v1.len() {
        let t = v1[i] - v2[i];
        v1[i] += v2[i];
        v2[i] = t;
    }
}


fn pow_m1_4_tables(
    x: f32,
    lsp_pow_e_table: &[f32; 256],
    lsp_pow_m_table1: &[f32; 1 << LSP_POW_BITS],
    lsp_pow_m_table2: &[f32; 1 << LSP_POW_BITS],
) -> f32 {
    // Direct translation of `pow_m1_4` from upstream wmadec.c, but parameterized to avoid borrowing `self`.
    let u = x.to_bits();
    let e = (u >> 23) as usize;
    let m = ((u >> (23 - LSP_POW_BITS)) & ((1 << LSP_POW_BITS) - 1) as u32) as usize;
    let t_bits = ((u << LSP_POW_BITS) & ((1 << 23) - 1)) | (127 << 23);
    let t = f32::from_bits(t_bits);
    let a = lsp_pow_m_table1[m];
    let b = lsp_pow_m_table2[m];
    lsp_pow_e_table[e] * (a + b * t)
}

fn wma_lsp_to_curve_tables(
    out: &mut [f32],
    n: usize,
    lsp: &[f32; NB_LSP_COEFS],
    lsp_cos_table: &[f32],
    lsp_pow_e_table: &[f32; 256],
    lsp_pow_m_table1: &[f32; 1 << LSP_POW_BITS],
    lsp_pow_m_table2: &[f32; 1 << LSP_POW_BITS],
) -> f32 {
    // Direct translation of `wma_lsp_to_curve` from upstream wmadec.c, parameterized to avoid borrowing `self`.
    let mut val_max = 0.0f32;
    for i in 0..n {
        let mut p = 0.5f32;
        let mut q = 0.5f32;
        let w = lsp_cos_table[i];
        let mut j = 1usize;
        while j < NB_LSP_COEFS {
            q *= w - lsp[j - 1];
            p *= w - lsp[j];
            j += 2;
        }
        p *= p * (2.0f32 - w);
        q *= q * (2.0f32 + w);
        let mut v = p + q;
        v = pow_m1_4_tables(v, lsp_pow_e_table, lsp_pow_m_table1, lsp_pow_m_table2);
        if v > val_max {
            val_max = v;
        }
        out[i] = v;
    }
    val_max
}




fn wma_window_apply(
    out: &mut [f32],
    output: &[f32],
    windows: &[Vec<f32>],
    frame_len_bits: i32,
    block_len_bits: i32,
    prev_block_len_bits: i32,
    next_block_len_bits: i32,
    block_len: usize,
) {
    // Direct translation of upstream `wma_window`, but parameterized to avoid borrowing `self`.
    let mut in_buf: &[f32] = output;

    // Left part.
    if block_len_bits <= prev_block_len_bits {
        let bsize = (frame_len_bits - block_len_bits) as usize;
        let win = &windows[bsize];
        for i in 0..block_len {
            out[i] = in_buf[i] * win[i] + out[i];
        }
    } else {
        let prev_len = 1usize << prev_block_len_bits;
        let n = (block_len - prev_len) / 2;
        let bsize = (frame_len_bits - prev_block_len_bits) as usize;
        let win = &windows[bsize];
        for i in 0..prev_len {
            let idx = n + i;
            out[idx] = in_buf[idx] * win[i] + out[idx];
        }
        out[n + prev_len..n + prev_len + n].copy_from_slice(&in_buf[n + prev_len..n + prev_len + n]);
    }

    // Right part.
    let out2 = &mut out[block_len..];
    in_buf = &in_buf[block_len..];

    if block_len_bits <= next_block_len_bits {
        let bsize = (frame_len_bits - block_len_bits) as usize;
        vector_fmul_reverse(&mut out2[..block_len], &in_buf[..block_len], &windows[bsize]);
    } else {
        let next_len = 1usize << next_block_len_bits;
        let n = (block_len - next_len) / 2;
        let bsize = (frame_len_bits - next_block_len_bits) as usize;
        out2[n + next_len..n + next_len + n].copy_from_slice(&in_buf[n + next_len..n + next_len + n]);
        vector_fmul_reverse(&mut out2[n..n + next_len], &in_buf[n..n + next_len], &windows[bsize]);
    }
}



impl WmaDecoder {
    pub fn new(info: &AudioStreamInfo) -> Result<Self> {
        let version = match info.format_tag {
            0x0160 => WmaVersion::V1,
            0x0161 => WmaVersion::V2,
            _ => return Err(DecoderError::Unsupported(format!("unsupported WMA format tag: 0x{:04x}", info.format_tag))),
        };

        if info.block_align == 0 {
            return Err(DecoderError::InvalidData("block_align is not set".into()));
        }

        let channels = info.channels as usize;
        if channels == 0 || channels > MAX_CHANNELS {
            return Err(DecoderError::Unsupported("only mono/stereo supported".into()));
        }

        // Extract flags2 like upstream.
        let mut flags2: u16 = 0;
        let extradata = &info.extra_data;
        match version {
            WmaVersion::V1 => {
                if extradata.len() >= 4 {
                    flags2 = u16::from_le_bytes([extradata[2], extradata[3]]);
                }
            }
            WmaVersion::V2 => {
                if extradata.len() >= 6 {
                    flags2 = u16::from_le_bytes([extradata[4], extradata[5]]);
                }
            }
        }

        let mut use_variable_block_len = (flags2 & 0x0004) != 0;
        let use_exp_vlc = (flags2 & 0x0001) != 0;
        let use_bit_reservoir = (flags2 & 0x0002) != 0;

        // upstream quirk (issue1503).
        if let WmaVersion::V2 = version {
            if extradata.len() >= 8 {
                let v = u16::from_le_bytes([extradata[4], extradata[5]]);
                if v == 0x000d && use_variable_block_len {
                    use_variable_block_len = false;
                }
            }
        }

        // Pre-init fixed fields.
        let mut dec = Self {
            version,
            channels,
            sample_rate: info.sample_rate,
            bit_rate: info.bit_rate,
            block_align: info.block_align,

            use_exp_vlc,
            use_bit_reservoir,
            use_variable_block_len,
            use_noise_coding: true,

            byte_offset_bits: 0,

            exp_vlc: Vlc::default(),
            hgain_vlc: Vlc::default(),
            coef_vlc: [Vlc::default(), Vlc::default()],
            run_table: [Vec::new(), Vec::new()],
            level_table: [Vec::new(), Vec::new()],

            frame_len_bits: 0,
            frame_len: 0,
            nb_block_sizes: 0,

            reset_block_lengths: true,
            block_len_bits: 0,
            next_block_len_bits: 0,
            prev_block_len_bits: 0,
            block_len: 0,
            block_num: 0,
            block_pos: 0,

            ms_stereo: false,
            channel_coded: [false; MAX_CHANNELS],

            exponent_sizes: [0usize; BLOCK_NB_SIZES],
            exponent_bands: [[0u16; 25]; BLOCK_NB_SIZES],
            high_band_start: [0usize; BLOCK_NB_SIZES],
            coefs_start: 0,
            coefs_end: [0usize; BLOCK_NB_SIZES],
            exponent_high_sizes: [0usize; BLOCK_NB_SIZES],
            exponent_high_bands: [[0u16; HIGH_BAND_MAX_SIZE]; BLOCK_NB_SIZES],

            high_band_coded: [[false; HIGH_BAND_MAX_SIZE]; MAX_CHANNELS],
            high_band_values: [[0i32; HIGH_BAND_MAX_SIZE]; MAX_CHANNELS],

            exponents_bsize: [0usize; MAX_CHANNELS],
            exponents: [vec![0f32; BLOCK_MAX_SIZE], vec![0f32; BLOCK_MAX_SIZE]],
            max_exponent: [1.0f32; MAX_CHANNELS],
            coefs1: [vec![0f32; BLOCK_MAX_SIZE], vec![0f32; BLOCK_MAX_SIZE]],
            coefs: [vec![0f32; BLOCK_MAX_SIZE], vec![0f32; BLOCK_MAX_SIZE]],

            mdct: Vec::new(),
            windows: Vec::new(),
            output: vec![0f32; BLOCK_MAX_SIZE * 2],
            frame_out: [vec![0f32; BLOCK_MAX_SIZE * 2], vec![0f32; BLOCK_MAX_SIZE * 2]],

            last_superframe: vec![0u8; MAX_CODED_SUPERFRAME_SIZE + 64],
            last_bitoffset: 0,
            last_superframe_len: 0,
            eof_done: false,

            noise_table: vec![0f32; NOISE_TAB_SIZE],
            noise_index: 0,
            noise_mult: 0.0,

            lsp_cos_table: vec![0f32; BLOCK_MAX_SIZE],
            lsp_pow_e_table: [0f32; 256],
            lsp_pow_m_table1: [0f32; 1 << LSP_POW_BITS],
            lsp_pow_m_table2: [0f32; 1 << LSP_POW_BITS],

            exponents_initialized: [false; MAX_CHANNELS],
        };

        // Full init = ff_wma_init + wma_decode_init bits.
        dec.ff_wma_init(flags2 as i32)?;
        dec.wma_decode_init(flags2 as i32)?;

        Ok(dec)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels as u16
    }

    pub fn frame_len(&self) -> usize {
        self.frame_len
    }

    /// Decode one ASF packet payload (usually `block_align` bytes).
    pub fn decode_packet(&mut self, pkt: &[u8], pts_ms: u32) -> Result<Option<PcmFrameF32>> {
        if pkt.is_empty() {
            if self.eof_done {
                return Ok(None);
            }
            // Flush delayed samples.
            self.eof_done = true;
            let mut out = Vec::with_capacity(self.frame_len * self.channels);
            for i in 0..self.frame_len {
                for ch in 0..self.channels {
                    out.push(self.frame_out[ch][i]);
                }
            }
            self.last_superframe_len = 0;
            return Ok(Some(PcmFrameF32 {
                pts_ms,
                sample_rate: self.sample_rate,
                channels: self.channels as u16,
                samples: out,
            }));
        }

        if pkt.len() < self.block_align as usize {
            return Err(DecoderError::InvalidData(format!(
                "Input packet size too small ({} < {})",
                pkt.len(),
                self.block_align
            )));
        }

        let buf = &pkt[..self.block_align as usize];

        let mut gb = GetBitContext::new(buf);

        let mut nb_frames: i32;

        if self.use_bit_reservoir {
            // super frame header
            gb.skip_bits(4)?; // super frame index
            let mut nf = gb.get_bits(4)? as i32;
            nf -= if self.last_superframe_len <= 0 { 1 } else { 0 };
            nb_frames = nf;
            if nb_frames <= 0 {
                let is_error = nb_frames < 0 || gb.bits_left() <= 8;
                if is_error {
                    return Err(DecoderError::InvalidData(format!(
                        "nb_frames is {nb_frames} bits left {}",
                        gb.bits_left()
                    )));
                }

                if self.last_superframe_len + buf.len() - 1 > MAX_CODED_SUPERFRAME_SIZE {
                    return Err(DecoderError::InvalidData("bit reservoir overflow".into()));
                }

                let mut q = self.last_superframe_len;
                let mut len = buf.len() - 1;
                while len > 0 {
                    let b = gb.get_bits(8)? as u8;
                    self.last_superframe[q] = b;
                    q += 1;
                    len -= 1;
                }

                self.last_superframe_len += 8 * buf.len() - 8;
                return Ok(None);
            }
        } else {
            nb_frames = 1;
        }

        // Planar output like upstream, then interleave.
        let mut samples: [Vec<f32>; MAX_CHANNELS] = [Vec::new(), Vec::new()];
        for ch in 0..self.channels {
            samples[ch].resize(nb_frames as usize * self.frame_len, 0f32);
        }
        let mut samples_offset: usize = 0;

        if self.use_bit_reservoir {
            let bit_offset = gb.get_bits((self.byte_offset_bits + 3) as usize)? as usize;
            if bit_offset as isize > gb.bits_left() {
                return Err(DecoderError::InvalidData("Invalid last frame bit offset".into()));
            }

            if self.last_superframe_len > 0 {
                // Add `bit_offset` bits to last frame.
                let add_bytes = (bit_offset + 7) >> 3;
                if self.last_superframe_len + add_bytes > MAX_CODED_SUPERFRAME_SIZE {
                    return Err(DecoderError::InvalidData("bit reservoir overflow".into()));
                }

                let mut q = self.last_superframe_len;
                let mut len = bit_offset;
                while len > 7 {
                    self.last_superframe[q] = gb.get_bits(8)? as u8;
                    q += 1;
                    len -= 8;
                }
                if len > 0 {
                    self.last_superframe[q] = (gb.get_bits(len)? as u8) << (8 - len);
                }

                // Decode the previous frame.
                let total_bits = self.last_superframe_len * 8 + bit_offset;
                let need_bytes = (total_bits + 7) / 8;
                // Avoid borrowing `self` across the decode call.
                let sf_bytes: Vec<u8> = self.last_superframe[..need_bytes].to_vec();
                let mut gb2 = GetBitContext::new(&sf_bytes);
                if self.last_bitoffset > 0 {
                    gb2.skip_bits(self.last_bitoffset)?;
                }
                self.reset_block_lengths = true;
                self.wma_decode_frame(&mut gb2, &mut samples, samples_offset)?;
                samples_offset += self.frame_len;
                nb_frames -= 1;
            }

            // Read each frame starting from bit_offset.
            let pos = bit_offset + 4 + 4 + (self.byte_offset_bits as usize) + 3;
            if pos >= MAX_CODED_SUPERFRAME_SIZE * 8 || pos > buf.len() * 8 {
                return Err(DecoderError::InvalidData("invalid superframe pos".into()));
            }

            let start_byte = pos >> 3;
            let mut gb3 = GetBitContext::new(&buf[start_byte..]);
            let rem = pos & 7;
            if rem > 0 {
                gb3.skip_bits(rem)?;
            }

            self.reset_block_lengths = true;
            for _ in 0..nb_frames {
                self.wma_decode_frame(&mut gb3, &mut samples, samples_offset)?;
                samples_offset += self.frame_len;
            }

            // Copy end of frame into last frame buffer.
            let consumed_bits = gb3.bits_read();
            let mut pos2 = consumed_bits + ((bit_offset + 4 + 4 + (self.byte_offset_bits as usize) + 3) & !7);
            self.last_bitoffset = pos2 & 7;
            pos2 >>= 3;
            let len = buf.len().saturating_sub(pos2);
            if len > MAX_CODED_SUPERFRAME_SIZE {
                return Err(DecoderError::InvalidData("invalid reservoir len".into()));
            }
            self.last_superframe_len = len;
            self.last_superframe[..len].copy_from_slice(&buf[pos2..pos2 + len]);
        } else {
            self.reset_block_lengths = true;
            self.wma_decode_frame(&mut gb, &mut samples, samples_offset)?;
            samples_offset += self.frame_len;
        }

        // Interleave.
        let total_samples = samples_offset * self.channels;
        let mut out = Vec::with_capacity(total_samples);
        for i in 0..samples_offset {
            for ch in 0..self.channels {
                out.push(samples[ch][i]);
            }
        }

        Ok(Some(PcmFrameF32 {
            pts_ms,
            sample_rate: self.sample_rate,
            channels: self.channels as u16,
            samples: out,
        }))
    }

    fn wma_decode_init(&mut self, flags2: i32) -> Result<()> {
        // Initialize MDCT contexts (naive) like wma_decode_init.
        let scale = 1.0f64 / 32768.0f64;
        self.mdct.clear();
        for i in 0..self.nb_block_sizes {
            let len = 1usize << (self.frame_len_bits - i as i32);
            self.mdct.push(MdctNaive::new(len, scale));
        }

        // Noise/hgain VLC.
        if self.use_noise_coding {
            let flat: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    tables::FF_WMA_HGAIN_HUFFTAB.as_ptr() as *const u8,
                    tables::FF_WMA_HGAIN_HUFFTAB.len() * 2,
                )
            };
            let lens: &[i8] = unsafe {
                std::slice::from_raw_parts(flat.as_ptr().add(1) as *const i8, flat.len() - 1)
            };
            ff_vlc_init_from_lengths(
                &mut self.hgain_vlc,
                HGAINVLCBITS,
                tables::FF_WMA_HGAIN_HUFFTAB.len(),
                lens,
                2,
                Some(flat),
                2,
                1,
                -18,
                0,
            )?;
        }

        // Exponent VLC.
        if self.use_exp_vlc {
            let bits = &tables::FF_AAC_SCALEFACTOR_BITS;
            let codes_u32 = &tables::FF_AAC_SCALEFACTOR_CODE;
            let codes_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(codes_u32.as_ptr() as *const u8, codes_u32.len() * 4)
            };

            ff_vlc_init_sparse(
                &mut self.exp_vlc,
                EXPVLCBITS,
                bits.len(),
                bits,
                1,
                1,
                codes_bytes,
                4,
                4,
                None,
                0,
                0,
                0,
            )?;
        } else {
            self.wma_lsp_to_curve_init(self.frame_len);
        }

        // Flags and defaults.
        let _ = flags2;
        Ok(())
    }

    fn ff_wma_init(&mut self, flags2: i32) -> Result<()> {
        // Validate stream params.
        if self.sample_rate > 50000 || self.channels > 2 || self.bit_rate == 0 {
            return Err(DecoderError::InvalidData("invalid audio params".into()));
        }

        let version_id = self.version.id();

        // Compute MDCT block size.
        self.frame_len_bits = ff_wma_get_frame_len_bits(self.sample_rate as i32, version_id, 0);
        self.next_block_len_bits = self.frame_len_bits;
        self.prev_block_len_bits = self.frame_len_bits;
        self.block_len_bits = self.frame_len_bits;

        self.frame_len = 1usize << self.frame_len_bits;
        if self.use_variable_block_len {
            let mut nb = ((flags2 >> 3) & 3) + 1;
            if (self.bit_rate / self.channels as u32) >= 32000 {
                nb += 2;
            }
            let nb_max = self.frame_len_bits - BLOCK_MIN_BITS;
            if nb > nb_max {
                nb = nb_max;
            }
            self.nb_block_sizes = (nb + 1) as usize;
        } else {
            self.nb_block_sizes = 1;
        }

        // Rate dependent params.
        self.use_noise_coding = true;
        let mut high_freq = self.sample_rate as f32 * 0.5f32;

        // Version 2 normalized rates.
        let mut sample_rate1 = self.sample_rate as i32;
        if version_id == 2 {
            if sample_rate1 >= 44100 {
                sample_rate1 = 44100;
            } else if sample_rate1 >= 22050 {
                sample_rate1 = 22050;
            } else if sample_rate1 >= 16000 {
                sample_rate1 = 16000;
            } else if sample_rate1 >= 11025 {
                sample_rate1 = 11025;
            } else if sample_rate1 >= 8000 {
                sample_rate1 = 8000;
            }
        }

        let bps = (self.bit_rate as f32) / ((self.channels as f32) * (self.sample_rate as f32));
        let mut bps1 = bps;
        if self.channels == 2 {
            bps1 = bps * 1.6f32;
        }

        let x = (bps * (self.frame_len as f32) / 8.0 + 0.5) as u32;
        self.byte_offset_bits = ilog2_u32(x.max(1)) + 2;

        // Compute high frequency and noise coding.
        if sample_rate1 == 44100 {
            if bps1 >= 0.61 {
                self.use_noise_coding = false;
            } else {
                high_freq *= 0.4;
            }
        } else if sample_rate1 == 22050 {
            if bps1 >= 1.16 {
                self.use_noise_coding = false;
            } else if bps1 >= 0.72 {
                high_freq *= 0.7;
            } else {
                high_freq *= 0.6;
            }
        } else if sample_rate1 == 16000 {
            if bps > 0.5 {
                high_freq *= 0.5;
            } else {
                high_freq *= 0.3;
            }
        } else if sample_rate1 == 11025 {
            high_freq *= 0.7;
        } else if sample_rate1 == 8000 {
            if bps <= 0.625 {
                high_freq *= 0.5;
            } else if bps > 0.75 {
                self.use_noise_coding = false;
            } else {
                high_freq *= 0.65;
            }
        } else {
            if bps >= 0.8 {
                high_freq *= 0.75;
            } else if bps >= 0.6 {
                high_freq *= 0.6;
            } else {
                high_freq *= 0.5;
            }
        }

        // Compute scale factor band sizes.
        self.coefs_start = if version_id == 1 { 3 } else { 0 };

        for k in 0..self.nb_block_sizes {
            let block_len = self.frame_len >> k;

            if version_id == 1 {
                let mut lpos = 0usize;
                let mut i = 0usize;
                for idx in 0..25 {
                    let a = tables::FF_WMA_CRITICAL_FREQS[idx] as usize;
                    let b = self.sample_rate as usize;
                    let mut pos = ((block_len * 2 * a) + (b >> 1)) / b;
                    if pos > block_len {
                        pos = block_len;
                    }
                    self.exponent_bands[0][idx] = (pos - lpos) as u16;
                    if pos >= block_len {
                        i = idx + 1;
                        break;
                    }
                    lpos = pos;
                    i = idx + 1;
                }
                self.exponent_sizes[0] = i;
            } else {
                // Hardcoded tables.
                let a = self.frame_len_bits - BLOCK_MIN_BITS - (k as i32);
                let mut table_row: Option<&[u8; 25]> = None;
                if a < 3 {
                    if self.sample_rate >= 44100 {
                        table_row = Some(&tables::EXPONENT_BAND_44100[a as usize]);
                    } else if self.sample_rate >= 32000 {
                        table_row = Some(&tables::EXPONENT_BAND_32000[a as usize]);
                    } else if self.sample_rate >= 22050 {
                        table_row = Some(&tables::EXPONENT_BAND_22050[a as usize]);
                    }
                }

                if let Some(row) = table_row {
                    let n = row[0] as usize;
                    for i in 0..n {
                        self.exponent_bands[k][i] = row[1 + i] as u16;
                    }
                    self.exponent_sizes[k] = n;
                } else {
                    let mut j = 0usize;
                    let mut lpos = 0usize;
                    for idx in 0..25 {
                        let a = tables::FF_WMA_CRITICAL_FREQS[idx] as usize;
                        let b = self.sample_rate as usize;
                        let mut pos = ((block_len * 2 * a) + (b << 1)) / (4 * b);
                        pos <<= 2;
                        if pos > block_len {
                            pos = block_len;
                        }
                        if pos > lpos {
                            self.exponent_bands[k][j] = (pos - lpos) as u16;
                            j += 1;
                        }
                        if pos >= block_len {
                            break;
                        }
                        lpos = pos;
                    }
                    self.exponent_sizes[k] = j;
                }
            }

            self.coefs_end[k] = (self.frame_len - ((self.frame_len * 9) / 100)) >> k;
            self.high_band_start[k] = (((block_len as f32) * 2.0 * high_freq) / (self.sample_rate as f32) + 0.5) as usize;

            let n = self.exponent_sizes[k];
            let mut j = 0usize;
            let mut pos = 0usize;
            for i in 0..n {
                let start0 = pos;
                pos += self.exponent_bands[k][i] as usize;
                let end0 = pos;
                let mut start = start0;
                let mut end = end0;
                if start < self.high_band_start[k] {
                    start = self.high_band_start[k];
                }
                if end > self.coefs_end[k] {
                    end = self.coefs_end[k];
                }
                if end > start {
                    self.exponent_high_bands[k][j] = (end - start) as u16;
                    j += 1;
                }
            }
            self.exponent_high_sizes[k] = j;
        }

        // Init MDCT windows.
        self.windows.clear();
        for i in 0..self.nb_block_sizes {
            let half = 1usize << (self.frame_len_bits - i as i32);
            self.windows.push(sine_window_init(half));
        }

        self.reset_block_lengths = true;

        // Noise table.
        if self.use_noise_coding {
            self.noise_mult = if self.use_exp_vlc { 0.02 } else { 0.04 };
            let mut seed: u32 = 1;
            let norm = (1.0 / ((1u64 << 31) as f32)) * 3.0f32.sqrt() * self.noise_mult;
            for i in 0..NOISE_TAB_SIZE {
                seed = seed.wrapping_mul(314159).wrapping_add(1);
                self.noise_table[i] = (seed as i32 as f32) * norm;
            }
        }

        // Choose coef VLC tables.
        let mut coef_vlc_table = 2;
        if self.sample_rate >= 32000 {
            if bps1 < 0.72 {
                coef_vlc_table = 0;
            } else if bps1 < 1.16 {
                coef_vlc_table = 1;
            }
        }
        let t0 = &tables::COEF_VLCS[coef_vlc_table * 2];
        let t1 = &tables::COEF_VLCS[coef_vlc_table * 2 + 1];

        self.init_coef_vlc(0, t0)?;
        self.init_coef_vlc(1, t1)?;

        Ok(())
    }

    fn init_coef_vlc(&mut self, idx: usize, tbl: &tables::CoefVlcTable) -> Result<()> {
        // vlc_init(vlc, VLCBITS, n, table_bits, 1, 1, table_codes, 4, 4, 0)
        let bits = tbl.huffbits;
        let codes_u32 = tbl.huffcodes;
        let codes_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(codes_u32.as_ptr() as *const u8, codes_u32.len() * 4)
        };
        ff_vlc_init_sparse(
            &mut self.coef_vlc[idx],
            VLCBITS,
            tbl.n,
            bits,
            1,
            1,
            codes_bytes,
            4,
            4,
            None,
            0,
            0,
            0,
        )?;

        // Build run/level tables like init_coef_vlc.
        let n = tbl.n;
        let levels_table = tbl.levels;

        let mut run_table = vec![0u16; n];
        let mut flevel_table = vec![0f32; n];
        let mut int_table = vec![0u16; n];

        let mut i = 2usize;
        let mut level = 1usize;
        let mut k = 0usize;
        while i < n {
            int_table[k] = i as u16;
            let l = levels_table[k] as usize;
            k += 1;
            for j in 0..l {
                run_table[i] = j as u16;
                flevel_table[i] = level as f32;
                i += 1;
            }
            level += 1;
        }

        self.run_table[idx] = run_table;
        self.level_table[idx] = flevel_table;

        Ok(())
    }

    fn ff_wma_total_gain_to_bits(total_gain: i32) -> i32 {
        if total_gain < 15 {
            13
        } else if total_gain < 32 {
            12
        } else if total_gain < 40 {
            11
        } else if total_gain < 45 {
            10
        } else {
            9
        }
    }

    fn ff_wma_get_large_val(gb: &mut GetBitContext<'_>) -> Result<u32> {
        let mut n_bits: usize = 8;
        if gb.get_bits1()? != 0 {
            n_bits += 8;
            if gb.get_bits1()? != 0 {
                n_bits += 8;
                if gb.get_bits1()? != 0 {
                    n_bits += 7;
                }
            }
        }
        gb.get_bits_long(n_bits)
    }

    #[allow(clippy::too_many_arguments)]
    fn ff_wma_run_level_decode(
        gb: &mut GetBitContext<'_>,
        vlc: &[VlcElem],
        level_table: &[f32],
        run_table: &[u16],
        version: i32,
        ptr: &mut [f32],
        mut offset: i32,
        num_coefs: i32,
        block_len: usize,
        frame_len_bits: i32,
        coef_nb_bits: i32,
    ) -> Result<()> {
        let coef_mask = (block_len as i32) - 1;
        while offset < num_coefs {
            let code = get_vlc2(gb, vlc, VLCBITS, VLCMAX)?;
            if code > 1 {
                offset += run_table[code as usize] as i32;
                let sign = gb.get_bits1()? as i32 - 1;
                let lvl_bits = level_table[code as usize].to_bits();
                let signed_bits = lvl_bits ^ ((sign as u32) & 0x8000_0000);
                ptr[(offset & coef_mask) as usize] = f32::from_bits(signed_bits);
            } else if code == 1 {
                break;
            } else {
                let level: i32;
                if version == 0 {
                    level = gb.get_bits(coef_nb_bits as usize)? as i32;
                    offset += gb.get_bits(frame_len_bits as usize)? as i32;
                } else {
                    level = Self::ff_wma_get_large_val(gb)? as i32;
                    if gb.get_bits1()? != 0 {
                        if gb.get_bits1()? != 0 {
                            if gb.get_bits1()? != 0 {
                                return Err(DecoderError::InvalidData("broken escape sequence".into()));
                            } else {
                                offset += gb.get_bits(frame_len_bits as usize)? as i32 + 4;
                            }
                        } else {
                            offset += gb.get_bits(2)? as i32 + 1;
                        }
                    }
                }
                let sign = gb.get_bits1()? as i32 - 1;
                let v = (level ^ sign) - sign;
                ptr[(offset & coef_mask) as usize] = v as f32;
            }
            offset += 1;
        }

        if offset > num_coefs {
            return Err(DecoderError::InvalidData("overflow in spectral RLE".into()));
        }

        Ok(())
    }

    fn wma_lsp_to_curve_init(&mut self, frame_len: usize) {
        let wdel = std::f32::consts::PI / (frame_len as f32);
        for i in 0..frame_len {
            self.lsp_cos_table[i] = 2.0f32 * (wdel * (i as f32)).cos();
        }

        for i in 0..256 {
            let e = (i as i32) - 126;
            self.lsp_pow_e_table[i] = (e as f32 * -0.25).exp2();
        }

        let mut b = 1.0f32;
        for i in (0..(1 << LSP_POW_BITS)).rev() {
            let m = (1 << LSP_POW_BITS) + i;
            let mut a = (m as f32) * (0.5f32 / (1 << LSP_POW_BITS) as f32);
            a = 1.0f32 / a.sqrt().sqrt();
            self.lsp_pow_m_table1[i] = 2.0f32 * a - b;
            self.lsp_pow_m_table2[i] = b - a;
            b = a;
        }
    }

    fn decode_exp_lsp(&mut self, gb: &mut GetBitContext<'_>, ch: usize) -> Result<()> {
        // upstream wmadec.c: decode_exp_lsp()
        let mut lsp: [f32; NB_LSP_COEFS] = [0.0; NB_LSP_COEFS];
        for i in 0..NB_LSP_COEFS {
            let val = if i == 0 || i >= 8 {
                gb.get_bits(3)? as usize
            } else {
                gb.get_bits(4)? as usize
            };
            lsp[i] = tables::FF_WMA_LSP_CODEBOOK[i][val];
        }

        let cos = &self.lsp_cos_table;
        let e = &self.lsp_pow_e_table;
        let m1 = &self.lsp_pow_m_table1;
        let m2 = &self.lsp_pow_m_table2;
        let out = &mut self.exponents[ch];
        let vmax = wma_lsp_to_curve_tables(out, self.block_len, &lsp, cos, e, m1, m2);
        self.max_exponent[ch] = vmax;
        Ok(())
    }

    fn decode_exp_vlc(&mut self, gb: &mut GetBitContext<'_>, ch: usize) -> Result<()> {
        let mut last_exp: i32;
        let mut max_scale: f32 = 0.0;
        let ptab = &tables::POW_TAB[60..];

        let bsize = (self.frame_len_bits - self.block_len_bits) as usize;
        let bands = &self.exponent_bands[bsize];

        let mut q = 0usize;
        let q_end = self.block_len;

        if self.version.id() == 1 {
            last_exp = gb.get_bits(5)? as i32 + 10;
            let v = ptab[last_exp as usize];
            max_scale = v;
            let n = bands[0] as usize;
            for _ in 0..n {
                self.exponents[ch][q] = v;
                q += 1;
            }
        } else {
            last_exp = 36;
        }

        let mut ptr_idx = 0usize;
        if self.version.id() == 1 {
            ptr_idx = 1;
        }

        while q < q_end {
            let code = get_vlc2(gb, &self.exp_vlc.table, EXPVLCBITS, EXPMAX)?;
            last_exp += code - 60;
            if (last_exp as i32 + 60) as usize >= tables::POW_TAB.len() {
                return Err(DecoderError::InvalidData(format!("Exponent out of range: {last_exp}")));
            }
            let v = ptab[last_exp as usize];
            if v > max_scale {
                max_scale = v;
            }
            let n = bands[ptr_idx] as usize;
            ptr_idx += 1;
            for _ in 0..n {
                self.exponents[ch][q] = v;
                q += 1;
            }
        }

        self.max_exponent[ch] = max_scale;
        Ok(())
    }


    fn wma_decode_block(&mut self, gb: &mut GetBitContext<'_>) -> Result<bool> {
        // Returns Ok(true) if last block of frame.
        // Translated from wma_decode_block.

        // Compute current block length.
        if self.use_variable_block_len {
            let n = ilog2_u32((self.nb_block_sizes - 1) as u32) + 1;
            if self.reset_block_lengths {
                self.reset_block_lengths = false;
                let v = gb.get_bits(n as usize)? as usize;
                if v >= self.nb_block_sizes {
                    return Err(DecoderError::InvalidData("prev_block_len_bits out of range".into()));
                }
                self.prev_block_len_bits = self.frame_len_bits - v as i32;
                let v = gb.get_bits(n as usize)? as usize;
                if v >= self.nb_block_sizes {
                    return Err(DecoderError::InvalidData("block_len_bits out of range".into()));
                }
                self.block_len_bits = self.frame_len_bits - v as i32;
            } else {
                self.prev_block_len_bits = self.block_len_bits;
                self.block_len_bits = self.next_block_len_bits;
            }
            let v = gb.get_bits(n as usize)? as usize;
            if v >= self.nb_block_sizes {
                return Err(DecoderError::InvalidData("next_block_len_bits out of range".into()));
            }
            self.next_block_len_bits = self.frame_len_bits - v as i32;
        } else {
            self.next_block_len_bits = self.frame_len_bits;
            self.prev_block_len_bits = self.frame_len_bits;
            self.block_len_bits = self.frame_len_bits;
        }

        let bsize = (self.frame_len_bits - self.block_len_bits) as usize;
        if (self.frame_len_bits - self.block_len_bits) as usize >= self.nb_block_sizes {
            return Err(DecoderError::InvalidData("block_len_bits not initialized".into()));
        }

        self.block_len = 1usize << self.block_len_bits;
        if self.block_pos + self.block_len > self.frame_len {
            return Err(DecoderError::InvalidData("frame_len overflow".into()));
        }

        if self.channels == 2 {
            self.ms_stereo = gb.get_bits1()? != 0;
        }

        let mut v_any = false;
        for ch in 0..self.channels {
            let a = gb.get_bits1()? != 0;
            self.channel_coded[ch] = a;
            v_any |= a;
        }

        if !v_any {
            return self.wma_decode_block_next(gb, bsize);
        }

        // Total gain.
        let mut total_gain: i32 = 1;
        loop {
            if gb.bits_left() < 7 {
                return Err(DecoderError::InvalidData("total_gain overread".into()));
            }
            let a = gb.get_bits(7)? as i32;
            total_gain += a;
            if a != 127 {
                break;
            }
        }

        let coef_nb_bits = Self::ff_wma_total_gain_to_bits(total_gain);

        // Number of coefficients.
        let ncoefs = (self.coefs_end[bsize] as i32) - (self.coefs_start as i32);
        let mut nb_coefs = [0i32; MAX_CHANNELS];
        for ch in 0..self.channels {
            nb_coefs[ch] = ncoefs;
        }

        // Noise coding.
        if self.use_noise_coding {
            for ch in 0..self.channels {
                if self.channel_coded[ch] {
                    let n1 = self.exponent_high_sizes[bsize];
                    for i in 0..n1 {
                        let a = gb.get_bits1()? != 0;
                        self.high_band_coded[ch][i] = a;
                        if a {
                            nb_coefs[ch] -= self.exponent_high_bands[bsize][i] as i32;
                        }
                    }
                }
            }
            for ch in 0..self.channels {
                if self.channel_coded[ch] {
                    let n1 = self.exponent_high_sizes[bsize];
                    let mut val: i32 = 0x8000_0000u32 as i32;
                    for i in 0..n1 {
                        if self.high_band_coded[ch][i] {
                            if val == (0x8000_0000u32 as i32) {
                                val = gb.get_bits(7)? as i32 - 19;
                            } else {
                                val += get_vlc2(gb, &self.hgain_vlc.table, HGAINVLCBITS, HGAINMAX)?;
                            }
                            self.high_band_values[ch][i] = val;
                        }
                    }
                }
            }
        }

        // Exponents can be reused in short blocks.
        let reuse = (self.block_len_bits == self.frame_len_bits) || (gb.get_bits1()? != 0);
        if reuse {
            for ch in 0..self.channels {
                if self.channel_coded[ch] {
                    if self.use_exp_vlc {
                        self.decode_exp_vlc(gb, ch)?;
                    } else {
                        self.decode_exp_lsp(gb, ch)?;
                    }
                    self.exponents_bsize[ch] = bsize;
                    self.exponents_initialized[ch] = true;
                }
            }
        }

        for ch in 0..self.channels {
            if self.channel_coded[ch] && !self.exponents_initialized[ch] {
                return Err(DecoderError::InvalidData("exponents not initialized".into()));
            }
        }

        // Parse spectral coefficients.
        for ch in 0..self.channels {
            if self.channel_coded[ch] {
                let tindex = (ch == 1 && self.ms_stereo) as usize;
                for v in &mut self.coefs1[ch][..self.block_len] {
                    *v = 0.0;
                }
                // Decode into coefs1 (upstream WMACoef).
                Self::ff_wma_run_level_decode(
                    gb,
                    &self.coef_vlc[tindex].table,
                    &self.level_table[tindex],
                    &self.run_table[tindex],
                    0,
                    &mut self.coefs1[ch],
                    0,
                    nb_coefs[ch],
                    self.block_len,
                    self.frame_len_bits,
                    coef_nb_bits,
                )?;
            }
            if self.version.id() == 1 && self.channels >= 2 {
                gb.align_to_byte();
            }
        }

        // Normalize.
        let n4 = self.block_len / 2;
        let mut mdct_norm = 1.0f32 / (n4 as f32);
        if self.version.id() == 1 {
            mdct_norm *= (n4 as f32).sqrt();
        }

        // Compute MDCT coefficients.
        for ch in 0..self.channels {
            if !self.channel_coded[ch] {
                continue;
            }

            let esize = self.exponents_bsize[ch];
            let mult = ff_exp10f(total_gain as f32 * 0.05f32) / self.max_exponent[ch] * mdct_norm;

            let mut coefs_pos = 0usize;

            if self.use_noise_coding {
                // very low freqs: noise
                for i in 0..self.coefs_start {
                    let exp_idx = ((i << bsize) >> esize) as usize;
                    let noise = self.noise_table[self.noise_index];
                    self.noise_index = (self.noise_index + 1) & (NOISE_TAB_SIZE - 1);
                    self.coefs[ch][coefs_pos] = noise * self.exponents[ch][exp_idx] * mult;
                    coefs_pos += 1;
                }

                let n1 = self.exponent_high_sizes[bsize];

                // compute power of high bands
                let mut exp_power = [0f32; HIGH_BAND_MAX_SIZE];
                let mut exponents_ptr = (self.high_band_start[bsize] << bsize) >> esize;
                let mut last_high_band: usize = 0;
                for j in 0..n1 {
                    let n = self.exponent_high_bands[bsize][j] as usize;
                    if self.high_band_coded[ch][j] {
                        let mut e2: f32 = 0.0;
                        for i in 0..n {
                            let v = self.exponents[ch][exponents_ptr + ((i << bsize) >> esize)];
                            e2 += v * v;
                        }
                        exp_power[j] = e2 / (n as f32);
                        last_high_band = j;
                    }
                    exponents_ptr += (n << bsize) >> esize;
                }

                // main freqs and high freqs
                let mut exponents_ptr = (self.coefs_start << bsize) >> esize;
                let mut coef1_idx = 0usize;

                for j in (-1i32)..(n1 as i32) {
                    let n = if j < 0 {
                        self.high_band_start[bsize].saturating_sub(self.coefs_start)
                    } else {
                        self.exponent_high_bands[bsize][j as usize] as usize
                    };

                    if j >= 0 && self.high_band_coded[ch][j as usize] {
                        let mut mult1 = (exp_power[j as usize] / exp_power[last_high_band]).sqrt();
                        mult1 *= ff_exp10f(self.high_band_values[ch][j as usize] as f32 * 0.05f32);
                        mult1 /= self.max_exponent[ch] * self.noise_mult;
                        mult1 *= mdct_norm;

                        for i in 0..n {
                            let noise = self.noise_table[self.noise_index];
                            self.noise_index = (self.noise_index + 1) & (NOISE_TAB_SIZE - 1);
                            let exp = self.exponents[ch][exponents_ptr + ((i << bsize) >> esize)];
                            self.coefs[ch][coefs_pos] = noise * exp * mult1;
                            coefs_pos += 1;
                        }
                        exponents_ptr += (n << bsize) >> esize;
                    } else {
                        for i in 0..n {
                            let noise = self.noise_table[self.noise_index];
                            self.noise_index = (self.noise_index + 1) & (NOISE_TAB_SIZE - 1);
                            let exp = self.exponents[ch][exponents_ptr + ((i << bsize) >> esize)];
                            let coef1 = self.coefs1[ch][coef1_idx];
                            coef1_idx += 1;
                            self.coefs[ch][coefs_pos] = (coef1 + noise) * exp * mult;
                            coefs_pos += 1;
                        }
                        exponents_ptr += (n << bsize) >> esize;
                    }
                }

                // very high freqs: noise
                let n = self.block_len - self.coefs_end[bsize];
                let exp_last = self.exponents[ch][((exponents_ptr as i32 - (1 << bsize)) >> esize) as usize];
                let mult1 = mult * exp_last;
                for _ in 0..n {
                    let noise = self.noise_table[self.noise_index];
                    self.noise_index = (self.noise_index + 1) & (NOISE_TAB_SIZE - 1);
                    self.coefs[ch][coefs_pos] = noise * mult1;
                    coefs_pos += 1;
                }
            } else {
                for _ in 0..self.coefs_start {
                    self.coefs[ch][coefs_pos] = 0.0;
                    coefs_pos += 1;
                }

                let n = nb_coefs[ch] as usize;
                for i in 0..n {
                    let exp = self.exponents[ch][((i << bsize) >> esize)];
                    let coef1 = self.coefs1[ch][i];
                    self.coefs[ch][coefs_pos] = coef1 * exp * mult;
                    coefs_pos += 1;
                }
                let tail = self.block_len - self.coefs_end[bsize];
                for _ in 0..tail {
                    self.coefs[ch][coefs_pos] = 0.0;
                    coefs_pos += 1;
                }
            }
        }

        if self.ms_stereo && self.channel_coded[1] {
            if !self.channel_coded[0] {
                for v in &mut self.coefs[0][..self.block_len] {
                    *v = 0.0;
                }
                self.channel_coded[0] = true;
            }
            let (c0, c1) = self.coefs.split_at_mut(1);
            let v0 = &mut c0[0][..self.block_len];
            let v1 = &mut c1[0][..self.block_len];
            butterflies_float(v0, v1);
        }

        self.wma_decode_block_next(gb, bsize)
    }

    fn wma_decode_block_next(&mut self, _gb: &mut GetBitContext<'_>, bsize: usize) -> Result<bool> {
        // MDCT + window add.
        for ch in 0..self.channels {
            let n4 = self.block_len / 2;
            if self.channel_coded[ch] {
                self.mdct[bsize].imdct_full(&mut self.output[..self.block_len * 2], &self.coefs[ch][..self.block_len]);
            } else if !(self.ms_stereo && ch == 1) {
                for v in &mut self.output[..self.block_len * 2] {
                    *v = 0.0;
                }
            }

            let index = (self.frame_len / 2) + self.block_pos - n4;
            // frame_out has length 2*BLOCK_MAX_SIZE.
            let frame_len_bits = self.frame_len_bits;
            let block_len_bits = self.block_len_bits;
            let prev_block_len_bits = self.prev_block_len_bits;
            let next_block_len_bits = self.next_block_len_bits;
            let block_len = self.block_len;
            let windows = &self.windows;
            let output = &self.output;
            let out_slice = &mut self.frame_out[ch][index..index + block_len * 2];
            wma_window_apply(out_slice, output, windows, frame_len_bits, block_len_bits, prev_block_len_bits, next_block_len_bits, block_len);
        }

        self.block_num += 1;
        self.block_pos += self.block_len;
        Ok(self.block_pos >= self.frame_len)
    }

    fn wma_decode_frame(&mut self, gb: &mut GetBitContext<'_>, samples: &mut [Vec<f32>; MAX_CHANNELS], samples_offset: usize) -> Result<()> {
        self.block_num = 0;
        self.block_pos = 0;
        loop {
            let last = self.wma_decode_block(gb)?;
            if last {
                break;
            }
        }

        for ch in 0..self.channels {
            samples[ch][samples_offset..samples_offset + self.frame_len]
                .copy_from_slice(&self.frame_out[ch][..self.frame_len]);
            // Shift for overlap.
            let tail = self.frame_out[ch][self.frame_len..self.frame_len * 2].to_vec();
            self.frame_out[ch][..self.frame_len].copy_from_slice(&tail);
        }

        Ok(())
    }
}
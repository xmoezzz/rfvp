// SPDX-License-Identifier: LGPL-2.1-or-later
//
// Native WMA Professional decoder.  The bitstream/state-machine organization
// follows FFmpeg libavcodec/wmaprodec.c; the implementation is Rust and uses
// this crate's own bitreader, VLC and MDCT primitives.

use crate::asf::AudioStreamInfo;
use crate::error::{DecoderError, Result};
use crate::wma::PcmFrameF32;
use crate::wma::bitstream::GetBitContext;
use crate::wma::common::ff_wma_get_frame_len_bits;
use crate::wma::mdct::MdctNaive;
use crate::wma::pro_tables as tables;
use crate::wma::vlc::{Vlc, ff_vlc_init_from_lengths, get_vlc2};

const MAX_CHANNELS: usize = 8;
const MAX_SUBFRAMES: usize = 32;
const MAX_BANDS: usize = 29;
const BLOCK_MIN_BITS: usize = 6;
const BLOCK_MAX_BITS: usize = 13;
const BLOCK_SIZES: usize = BLOCK_MAX_BITS - BLOCK_MIN_BITS + 1;
const MAX_BLOCK_SIZE: usize = 1 << BLOCK_MAX_BITS;
const MAX_FRAME_BITS: usize = 32768 * 8;
const VLCBITS: i32 = 9;
const SCALEVLCBITS: i32 = 8;

#[derive(Clone)]
struct ChannelCtx {
    prev_block_len: usize,
    transmit_coefs: bool,
    subframe_len: [usize; MAX_SUBFRAMES],
    subframe_offset: [usize; MAX_SUBFRAMES],
    num_subframes: usize,
    cur_subframe: usize,
    decoded_samples: usize,
    grouped: bool,
    quant_step: i32,
    reuse_sf: bool,
    scale_factor_step: i32,
    max_scale_factor: i32,
    scale_factors: [i32; MAX_BANDS],
    saved_scale_factors: [[i32; MAX_BANDS]; 2],
    scale_factor_idx: usize,
    table_idx: usize,
    num_vec_coeffs: usize,
    out: Vec<f32>,
}

impl ChannelCtx {
    fn new(samples_per_frame: usize) -> Self {
        Self {
            prev_block_len: samples_per_frame,
            transmit_coefs: false,
            subframe_len: [0; MAX_SUBFRAMES],
            subframe_offset: [0; MAX_SUBFRAMES],
            num_subframes: 0,
            cur_subframe: 0,
            decoded_samples: 0,
            grouped: false,
            quant_step: 0,
            reuse_sf: false,
            scale_factor_step: 1,
            max_scale_factor: 0,
            scale_factors: [0; MAX_BANDS],
            saved_scale_factors: [[0; MAX_BANDS]; 2],
            scale_factor_idx: 0,
            table_idx: 0,
            num_vec_coeffs: 0,
            out: vec![0.0; MAX_BLOCK_SIZE + MAX_BLOCK_SIZE / 2],
        }
    }
}

#[derive(Clone, Default)]
struct ChannelGroup {
    channels: Vec<usize>,
    transform: bool,
    transform_band: [bool; MAX_BANDS],
    matrix: Vec<f32>,
}

struct ProVlcs {
    sf: Vlc,
    sf_rl: Vlc,
    coef: [Vlc; 2],
    vec4: Vlc,
    vec2: Vlc,
    vec1: Vlc,
}

/// Native Windows Media Audio 9 Professional (WAVE tag 0x0162) decoder.
pub struct WmaProDecoder {
    channels: usize,
    sample_rate: u32,
    block_align: usize,
    bits_per_sample: usize,
    decode_flags: u32,
    channel_mask: u32,

    log2_frame_size: usize,
    len_prefix: bool,
    dynamic_range_compression: bool,
    samples_per_frame: usize,
    max_num_subframes: usize,
    max_subframe_len_bit: bool,
    subframe_len_bits: usize,
    min_samples_per_subframe: usize,
    num_possible_block_sizes: usize,
    lfe_channel: Option<usize>,

    num_sfb: [usize; BLOCK_SIZES],
    sfb_offsets: [[usize; MAX_BANDS + 1]; BLOCK_SIZES],
    sf_offsets: [[[usize; MAX_BANDS]; BLOCK_SIZES]; BLOCK_SIZES],
    subwoofer_cutoffs: [usize; BLOCK_SIZES],

    mdct: Vec<MdctNaive>,
    windows: Vec<Vec<f32>>,
    vlcs: ProVlcs,
    channel: Vec<ChannelCtx>,

    packet_sequence_number: Option<u8>,
    reservoir: Vec<u8>,
    reservoir_bits: usize,
    pending_packet_bytes: Vec<u8>,
    pending_packet_pts_ms: Option<u32>,
    packet_loss: bool,
    skip_frame: bool,
    eof_done: bool,
    frame_num: u64,
}

impl WmaProDecoder {
    pub fn new(info: &AudioStreamInfo) -> Result<Self> {
        if info.format_tag != 0x0162 {
            return Err(DecoderError::Unsupported(format!(
                "WMA Pro decoder requires format tag 0x0162, got 0x{:04x}",
                info.format_tag
            )));
        }
        if info.block_align == 0 {
            return Err(DecoderError::InvalidData(
                "WMA Pro block_align is zero".into(),
            ));
        }
        if info.extra_data.len() < 18 {
            return Err(DecoderError::InvalidData(format!(
                "WMA Pro extradata too short: {} < 18",
                info.extra_data.len()
            )));
        }

        let bits_per_sample = u16::from_le_bytes([info.extra_data[0], info.extra_data[1]]) as usize;
        if !(1..=32).contains(&bits_per_sample) {
            return Err(DecoderError::Unsupported(format!(
                "WMA Pro bits per sample {bits_per_sample}"
            )));
        }
        let channel_mask = u32::from_le_bytes([
            info.extra_data[2],
            info.extra_data[3],
            info.extra_data[4],
            info.extra_data[5],
        ]);
        let decode_flags = u16::from_le_bytes([info.extra_data[14], info.extra_data[15]]) as u32;
        let channels = if channel_mask != 0 {
            channel_mask.count_ones() as usize
        } else {
            info.channels as usize
        };
        if channels == 0 || channels > MAX_CHANNELS || channels > info.channels as usize {
            return Err(DecoderError::Unsupported(format!(
                "WMA Pro channel layout: stream={}, mask=0x{channel_mask:08x}",
                info.channels
            )));
        }

        let block_align = info.block_align as usize;
        let log2_frame_size = floor_log2(block_align) + 4;
        if log2_frame_size > 25 {
            return Err(DecoderError::Unsupported(
                "WMA Pro large block alignment".into(),
            ));
        }

        let frame_bits = ff_wma_get_frame_len_bits(info.sample_rate as i32, 3, decode_flags);
        if frame_bits < BLOCK_MIN_BITS as i32 || frame_bits > BLOCK_MAX_BITS as i32 {
            return Err(DecoderError::Unsupported(format!(
                "WMA Pro {}-bit frame size",
                frame_bits
            )));
        }
        let samples_per_frame = 1usize << frame_bits;

        let log2_max_num_subframes = ((decode_flags & 0x38) >> 3) as usize;
        let max_num_subframes = 1usize << log2_max_num_subframes;
        if max_num_subframes > MAX_SUBFRAMES {
            return Err(DecoderError::InvalidData(
                "WMA Pro too many subframes".into(),
            ));
        }
        let max_subframe_len_bit = matches!(max_num_subframes, 4 | 16);
        let subframe_len_bits = floor_log2(log2_max_num_subframes.max(1)) + 1;
        let min_samples_per_subframe = samples_per_frame / max_num_subframes;
        if min_samples_per_subframe < (1 << BLOCK_MIN_BITS) {
            return Err(DecoderError::InvalidData(
                "WMA Pro subframe too small".into(),
            ));
        }
        let num_possible_block_sizes = log2_max_num_subframes + 1;
        if num_possible_block_sizes > BLOCK_SIZES {
            return Err(DecoderError::InvalidData(
                "WMA Pro block-size table overflow".into(),
            ));
        }

        let mut num_sfb = [0usize; BLOCK_SIZES];
        let mut sfb_offsets = [[0usize; MAX_BANDS + 1]; BLOCK_SIZES];
        for i in 0..num_possible_block_sizes {
            let subframe_len = samples_per_frame >> i;
            let mut band = 1usize;
            for &freq in &tables::CRITICAL_FREQ {
                if band >= MAX_BANDS || sfb_offsets[i][band - 1] >= subframe_len {
                    break;
                }
                let mut offset = (subframe_len * 2 * freq as usize) / info.sample_rate as usize + 2;
                offset &= !3usize;
                if offset > sfb_offsets[i][band - 1] {
                    sfb_offsets[i][band] = offset;
                    band += 1;
                }
                if offset >= subframe_len {
                    break;
                }
            }
            sfb_offsets[i][band - 1] = subframe_len;
            num_sfb[i] = band - 1;
            if num_sfb[i] == 0 {
                return Err(DecoderError::InvalidData(
                    "WMA Pro scale-factor bands invalid".into(),
                ));
            }
        }

        let mut sf_offsets = [[[0usize; MAX_BANDS]; BLOCK_SIZES]; BLOCK_SIZES];
        for i in 0..num_possible_block_sizes {
            for b in 0..num_sfb[i] {
                let offset = ((sfb_offsets[i][b] + sfb_offsets[i][b + 1] - 1) << i) >> 1;
                for x in 0..num_possible_block_sizes {
                    let mut v = 0usize;
                    while v + 1 < MAX_BANDS && (sfb_offsets[x][v + 1] << x) < offset {
                        v += 1;
                    }
                    sf_offsets[i][x][b] = v;
                }
            }
        }

        let mut mdct = Vec::with_capacity(BLOCK_SIZES);
        let mut windows = Vec::with_capacity(BLOCK_SIZES);
        for bits in BLOCK_MIN_BITS..=BLOCK_MAX_BITS {
            let len = 1usize << bits;
            let scale =
                1.0f64 / ((1u64 << (bits - 1)) as f64) / ((1u64 << (bits_per_sample - 1)) as f64);
            mdct.push(MdctNaive::new(len, scale));
            windows.push(sine_window(len));
        }

        let mut subwoofer_cutoffs = [0usize; BLOCK_SIZES];
        for i in 0..num_possible_block_sizes {
            let block_size = samples_per_frame >> i;
            let numer = 440usize * block_size + 3usize * ((info.sample_rate as usize) >> 1) - 1;
            subwoofer_cutoffs[i] = (numer / info.sample_rate as usize).clamp(4, block_size);
        }

        let vlcs = build_vlcs()?;
        let lfe_channel = if (channel_mask & 8) != 0 {
            let mut pos = 0usize;
            for bit in [1u32, 2, 4, 8] {
                if channel_mask & bit != 0 {
                    if bit == 8 {
                        break;
                    }
                    pos += 1;
                }
            }
            Some(pos)
        } else {
            None
        };

        Ok(Self {
            channels,
            sample_rate: info.sample_rate,
            block_align,
            bits_per_sample,
            decode_flags,
            channel_mask,
            log2_frame_size,
            len_prefix: (decode_flags & 0x40) != 0,
            dynamic_range_compression: (decode_flags & 0x80) != 0,
            samples_per_frame,
            max_num_subframes,
            max_subframe_len_bit,
            subframe_len_bits,
            min_samples_per_subframe,
            num_possible_block_sizes,
            lfe_channel,
            num_sfb,
            sfb_offsets,
            sf_offsets,
            subwoofer_cutoffs,
            mdct,
            windows,
            vlcs,
            channel: (0..channels)
                .map(|_| ChannelCtx::new(samples_per_frame))
                .collect(),
            packet_sequence_number: None,
            reservoir: Vec::new(),
            reservoir_bits: 0,
            pending_packet_bytes: Vec::new(),
            pending_packet_pts_ms: None,
            packet_loss: true,
            skip_frame: true,
            eof_done: false,
            frame_num: 0,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn channels(&self) -> u16 {
        self.channels as u16
    }
    pub fn frame_len(&self) -> usize {
        self.samples_per_frame
    }

    /// Decode one ASF media object.  All WMA Pro frames contained in the object
    /// are concatenated into one interleaved PCM chunk; a frame crossing an ASF
    /// object boundary is retained in the bit reservoir and completed by the
    /// next call.
    pub fn decode_packet(&mut self, pkt: &[u8], pts_ms: u32) -> Result<Option<PcmFrameF32>> {
        if pkt.is_empty() {
            return self.flush(pts_ms);
        }

        // ASF media-object boundaries are normally WMA packet boundaries, but
        // do not require that from the demuxer. Keep an incomplete
        // block_align-sized codec packet until the following media object.
        if self.pending_packet_bytes.is_empty() {
            self.pending_packet_pts_ms = Some(pts_ms);
        }
        self.pending_packet_bytes.extend_from_slice(pkt);

        let output_pts_ms = self.pending_packet_pts_ms.unwrap_or(pts_ms);
        let mut planar_frames: Vec<Vec<Vec<f32>>> = Vec::new();

        while self.pending_packet_bytes.len() >= self.block_align {
            let block: Vec<u8> = self
                .pending_packet_bytes
                .drain(..self.block_align)
                .collect();
            self.decode_block(&block, &mut planar_frames)?;

            if self.pending_packet_bytes.is_empty() {
                self.pending_packet_pts_ms = None;
            }
        }

        self.pack_output(planar_frames, output_pts_ms)
    }

    /// Decode one `block_align`-sized WMA Professional codec packet.
    ///
    /// This mirrors the packet state machine in FFmpeg's `decode_packet()`:
    /// the packet header describes how many bits at the start complete the
    /// frame saved from the preceding packet. With length-prefixed frames,
    /// complete frames that follow are decoded directly. Without length
    /// prefixes, the remainder is deliberately held until the next packet
    /// supplies the exact boundary of the final cross-packet frame.
    fn decode_block(&mut self, buf: &[u8], out: &mut Vec<Vec<Vec<f32>>>) -> Result<()> {
        debug_assert_eq!(buf.len(), self.block_align);

        let mut gb = GetBitContext::new(buf);
        let seq = gb.get_bits(4)? as u8;
        gb.skip_bits(2)?;
        let declared_prev_bits = gb.get_bits(self.log2_frame_size)? as usize;
        let remaining_after_header = gb.bits_left().max(0) as usize;
        let prev_bits = declared_prev_bits.min(remaining_after_header);
        let continuation_complete = declared_prev_bits <= remaining_after_header;

        if let Some(old) = self.packet_sequence_number
            && !self.packet_loss
            && ((old + 1) & 0x0f) != seq
        {
            self.packet_loss = true;
        }
        self.packet_sequence_number = Some(seq);

        if prev_bits != 0 {
            if self.reservoir_bits != 0 && !self.packet_loss {
                append_bits_from_reader(
                    &mut self.reservoir,
                    &mut self.reservoir_bits,
                    &mut gb,
                    prev_bits,
                )?;

                // When the advertised continuation fits in this packet, the
                // saved buffer now contains only complete frames. In the
                // non-length-prefixed mode there may be several such frames,
                // distinguished by their trailer bits.
                if continuation_complete {
                    let saved = std::mem::take(&mut self.reservoir);
                    let saved_bits = self.reservoir_bits;
                    self.reservoir_bits = 0;
                    self.decode_complete_bit_buffer(&saved, saved_bits, out)?;
                }
            } else {
                gb.skip_bits(prev_bits)?;
            }
        } else if self.reservoir_bits != 0 {
            // The new packet says that none of its leading bits belongs to the
            // previously saved frame, therefore that saved tail was damaged or
            // incomplete and must not be decoded.
            self.reservoir.clear();
            self.reservoir_bits = 0;
        }

        if self.packet_loss {
            // Same recovery point as FFmpeg: after the packet boundary has
            // re-established synchronization, discard stale reservoir state
            // and allow decoding of fresh frames in this packet.
            self.reservoir.clear();
            self.reservoir_bits = 0;
            self.packet_loss = false;
        }

        // If the continuation itself spans the entire packet, every bit has
        // already been appended to the reservoir. A later packet will finish
        // it.
        if !continuation_complete {
            return Ok(());
        }

        if self.len_prefix {
            loop {
                let bits_left = gb.bits_left().max(0) as usize;
                if bits_left <= self.log2_frame_size {
                    break;
                }

                let frame_size = gb.show_bits(self.log2_frame_size)? as usize;
                if frame_size == 0 || frame_size > bits_left {
                    break;
                }

                // FFmpeg copies exactly `frame_size` bits into its frame
                // reservoir before calling decode_frame(). Do the same here:
                // the copied slice still starts with the length prefix, so the
                // prefix is consumed exactly once by decode_frame().
                let frame_bits = take_bits(&mut gb, frame_size)?;
                let mut frame_gb = GetBitContext::new_bits(&frame_bits, frame_size)?;
                let (more, frame) = self.decode_frame(&mut frame_gb, frame_size)?;
                if let Some(frame) = frame {
                    out.push(frame);
                }
                if frame_gb.bits_read() != frame_size {
                    return Err(DecoderError::InvalidData(format!(
                        "WMA Pro frame did not consume declared length: {} != {}",
                        frame_gb.bits_read(),
                        frame_size
                    )));
                }

                // A zero trailer means the remainder begins the frame that is
                // completed by the next packet.
                if !more {
                    break;
                }
            }
        }

        let rest = gb.bits_left().max(0) as usize;
        if rest != 0 {
            self.reservoir = take_bits(&mut gb, rest)?;
            self.reservoir_bits = rest;
            if self.reservoir_bits > MAX_FRAME_BITS {
                return Err(DecoderError::InvalidData(
                    "WMA Pro frame reservoir overflow".into(),
                ));
            }
        } else {
            self.reservoir.clear();
            self.reservoir_bits = 0;
        }

        Ok(())
    }

    fn flush(&mut self, pts_ms: u32) -> Result<Option<PcmFrameF32>> {
        if self.eof_done {
            return Ok(None);
        }
        self.eof_done = true;
        self.reservoir.clear();
        self.reservoir_bits = 0;
        self.pending_packet_bytes.clear();
        self.pending_packet_pts_ms = None;

        // FFmpeg emits the saved second half of the last IMDCT block at EOF.
        let mut planar = vec![vec![0.0f32; self.samples_per_frame]; self.channels];
        for (ch, dst) in planar.iter_mut().enumerate() {
            let n = self.samples_per_frame / 2;
            dst[..n].copy_from_slice(&self.channel[ch].out[..n]);
        }
        self.pack_output(vec![planar], pts_ms)
    }

    fn pack_output(&self, frames: Vec<Vec<Vec<f32>>>, pts_ms: u32) -> Result<Option<PcmFrameF32>> {
        if frames.is_empty() {
            return Ok(None);
        }
        let total_per_channel: usize = frames.iter().map(|f| f[0].len()).sum();
        let mut samples = Vec::with_capacity(total_per_channel * self.channels);
        for frame in frames {
            let len = frame[0].len();
            for i in 0..len {
                for ch in 0..self.channels {
                    samples.push(frame[ch][i]);
                }
            }
        }
        Ok(Some(PcmFrameF32 {
            pts_ms,
            sample_rate: self.sample_rate,
            channels: self.channels as u16,
            samples,
        }))
    }

    fn decode_complete_bit_buffer(
        &mut self,
        data: &[u8],
        bit_len: usize,
        out: &mut Vec<Vec<Vec<f32>>>,
    ) -> Result<()> {
        if bit_len == 0 {
            return Ok(());
        }
        let mut gb = GetBitContext::new_bits(data, bit_len)?;
        loop {
            if gb.bits_left() <= 1 {
                break;
            }
            let before = gb.bits_read();
            let (more, frame) = self.decode_frame(&mut gb, bit_len)?;
            if let Some(frame) = frame {
                out.push(frame);
            }
            if !more || gb.bits_read() <= before {
                break;
            }
        }
        Ok(())
    }

    /// Returns (more_frames, optional planar frame).
    fn decode_frame(
        &mut self,
        gb: &mut GetBitContext<'_>,
        frame_bit_limit: usize,
    ) -> Result<(bool, Option<Vec<Vec<f32>>>)> {
        let frame_offset = gb.bits_read();
        let declared_len = if self.len_prefix {
            gb.get_bits(self.log2_frame_size)? as usize
        } else {
            0
        };

        self.decode_tilehdr(gb)?;

        // Optional post-processing transform parameters; FFmpeg parses and
        // discards them because they do not affect reconstruction here.
        if self.channels > 1 && gb.get_bits1()? != 0 && gb.get_bits1()? != 0 {
            gb.skip_bits(self.channels * self.channels * 4)?;
        }
        if self.dynamic_range_compression {
            let _drc_gain = gb.get_bits(8)?;
        }

        let mut trim_start = 0usize;
        let mut trim_end = 0usize;
        if gb.get_bits1()? != 0 {
            let trim_bits = floor_log2(self.samples_per_frame * 2);
            if gb.get_bits1()? != 0 {
                trim_start = gb.get_bits(trim_bits)? as usize;
            }
            if gb.get_bits1()? != 0 {
                trim_end = gb.get_bits(trim_bits)? as usize;
            }
        }

        for ch in &mut self.channel {
            ch.decoded_samples = 0;
            ch.cur_subframe = 0;
            ch.reuse_sf = false;
        }

        let mut parsed_all = false;
        while !parsed_all {
            parsed_all = self.decode_subframe(gb, frame_bit_limit)?;
        }

        let mut planar = vec![vec![0.0f32; self.samples_per_frame]; self.channels];
        for ch in 0..self.channels {
            planar[ch].copy_from_slice(&self.channel[ch].out[..self.samples_per_frame]);
            let half = self.samples_per_frame / 2;
            self.channel[ch]
                .out
                .copy_within(self.samples_per_frame..self.samples_per_frame + half, 0);
        }

        let mut emit = true;
        if self.skip_frame {
            self.skip_frame = false;
            emit = false;
        }

        if emit && (trim_start != 0 || trim_end != 0) {
            let start = trim_start.min(self.samples_per_frame);
            let end = self.samples_per_frame.saturating_sub(trim_end).max(start);
            for ch in 0..self.channels {
                planar[ch] = planar[ch][start..end].to_vec();
            }
        }

        if self.len_prefix {
            let consumed = gb.bits_read().saturating_sub(frame_offset);
            // FFmpeg requires the prefix to describe exactly the decoded
            // frame plus the one skipped trailer/padding bit and the
            // `more_frames` bit read below.
            if declared_len != consumed + 2 {
                return Err(DecoderError::InvalidData(format!(
                    "WMA Pro frame length mismatch: declared={declared_len}, consumed={consumed}"
                )));
            }
            gb.skip_bits(1)?;
        } else {
            while gb.bits_read() < frame_bit_limit && gb.show_bits(1)? == 0 {
                gb.skip_bits(1)?;
            }
        }

        let more = if gb.bits_read() < frame_bit_limit {
            gb.get_bits1()? != 0
        } else {
            false
        };
        self.frame_num += 1;
        Ok((more, if emit { Some(planar) } else { None }))
    }

    fn decode_subframe_length(&self, gb: &mut GetBitContext<'_>, offset: usize) -> Result<usize> {
        if offset == self.samples_per_frame - self.min_samples_per_subframe {
            return Ok(self.min_samples_per_subframe);
        }
        let mut shift = 0usize;
        if self.max_subframe_len_bit {
            if gb.get_bits1()? != 0 {
                shift = 1 + gb.get_bits(self.subframe_len_bits.saturating_sub(1))? as usize;
            }
        } else {
            shift = gb.get_bits(self.subframe_len_bits)? as usize;
        }
        let len = self.samples_per_frame >> shift;
        if len < self.min_samples_per_subframe || len > self.samples_per_frame {
            return Err(DecoderError::InvalidData(
                "broken WMA Pro subframe length".into(),
            ));
        }
        Ok(len)
    }

    fn decode_tilehdr(&mut self, gb: &mut GetBitContext<'_>) -> Result<()> {
        let mut num_samples = [0usize; MAX_CHANNELS];
        let mut contains = [false; MAX_CHANNELS];
        let mut channels_for_cur = self.channels;
        let fixed_layout = self.max_num_subframes == 1 || gb.get_bits1()? != 0;
        let mut min_channel_len = 0usize;
        for ch in &mut self.channel {
            ch.num_subframes = 0;
        }

        while min_channel_len < self.samples_per_frame {
            for c in 0..self.channels {
                contains[c] = if num_samples[c] == min_channel_len {
                    fixed_layout
                        || channels_for_cur == 1
                        || min_channel_len == self.samples_per_frame - self.min_samples_per_subframe
                        || gb.get_bits1()? != 0
                } else {
                    false
                };
            }
            let sub_len = self.decode_subframe_length(gb, min_channel_len)?;
            min_channel_len += sub_len;
            for c in 0..self.channels {
                if contains[c] {
                    let n = self.channel[c].num_subframes;
                    if n >= MAX_SUBFRAMES {
                        return Err(DecoderError::InvalidData(
                            "WMA Pro num_subframes > 31".into(),
                        ));
                    }
                    self.channel[c].subframe_len[n] = sub_len;
                    num_samples[c] += sub_len;
                    self.channel[c].num_subframes += 1;
                    if num_samples[c] > self.samples_per_frame {
                        return Err(DecoderError::InvalidData(
                            "WMA Pro channel tiling overflow".into(),
                        ));
                    }
                } else if num_samples[c] <= min_channel_len {
                    if num_samples[c] < min_channel_len {
                        channels_for_cur = 0;
                        min_channel_len = num_samples[c];
                    }
                    channels_for_cur += 1;
                }
            }
        }

        for c in 0..self.channels {
            let mut off = 0usize;
            for i in 0..self.channel[c].num_subframes {
                self.channel[c].subframe_offset[i] = off;
                off += self.channel[c].subframe_len[i];
            }
            if off != self.samples_per_frame {
                return Err(DecoderError::InvalidData(
                    "WMA Pro incomplete channel tiling".into(),
                ));
            }
        }
        Ok(())
    }

    /// Decode one subframe and return true once the complete frame is covered.
    fn decode_subframe(
        &mut self,
        gb: &mut GetBitContext<'_>,
        frame_bit_limit: usize,
    ) -> Result<bool> {
        let mut offset = self.samples_per_frame;
        let mut subframe_len = self.samples_per_frame;
        let mut total_samples = self.samples_per_frame * self.channels;

        for c in 0..self.channels {
            self.channel[c].grouped = false;
            if offset > self.channel[c].decoded_samples {
                offset = self.channel[c].decoded_samples;
                let sf = self.channel[c].cur_subframe;
                if sf >= self.channel[c].num_subframes {
                    return Err(DecoderError::InvalidData(
                        "WMA Pro broken subframe index".into(),
                    ));
                }
                subframe_len = self.channel[c].subframe_len[sf];
            }
        }

        let mut active = Vec::with_capacity(self.channels);
        for c in 0..self.channels {
            let cur = self.channel[c].cur_subframe;
            total_samples = total_samples.saturating_sub(self.channel[c].decoded_samples);
            if offset == self.channel[c].decoded_samples
                && cur < self.channel[c].num_subframes
                && subframe_len == self.channel[c].subframe_len[cur]
            {
                total_samples = total_samples.saturating_sub(subframe_len);
                self.channel[c].decoded_samples += subframe_len;
                active.push(c);
            }
        }
        let parsed_all = total_samples == 0;
        if active.is_empty() {
            return Err(DecoderError::InvalidData(
                "WMA Pro subframe has no channels".into(),
            ));
        }

        let table_idx = floor_log2(self.samples_per_frame / subframe_len);
        if table_idx >= self.num_possible_block_sizes {
            return Err(DecoderError::InvalidData(
                "WMA Pro invalid block-size index".into(),
            ));
        }
        let num_bands = self.num_sfb[table_idx];
        let sfb = self.sfb_offsets[table_idx];
        let subwoofer_cutoff = self.subwoofer_cutoffs[table_idx];
        let coeff_offset = offset + self.samples_per_frame / 2;
        if coeff_offset + subframe_len > MAX_BLOCK_SIZE + MAX_BLOCK_SIZE / 2 {
            return Err(DecoderError::InvalidData(
                "WMA Pro coefficient buffer overflow".into(),
            ));
        }
        let esc_len = floor_log2(subframe_len - 1) + 1;

        // Extended subframe header.
        if gb.get_bits1()? != 0 {
            let mut fill = gb.get_bits(2)? as usize;
            if fill == 0 {
                let len = gb.get_bits(4)? as usize;
                fill = if len == 0 {
                    1
                } else {
                    gb.get_bits(len)? as usize + 1
                };
            }
            if gb.bits_read() + fill > frame_bit_limit {
                return Err(DecoderError::InvalidData(
                    "WMA Pro fill bits overflow".into(),
                ));
            }
            gb.skip_bits(fill)?;
        }
        if gb.get_bits1()? != 0 {
            return Err(DecoderError::Unsupported(
                "WMA Pro reserved subframe bit".into(),
            ));
        }

        let groups = self.decode_channel_transform(gb, &active, num_bands)?;

        let mut transmit_any = false;
        for &c in &active {
            let transmit = gb.get_bits1()? != 0;
            self.channel[c].transmit_coefs = transmit;
            transmit_any |= transmit;
        }

        let mut transmit_num_vec_coeffs = false;
        if transmit_any {
            transmit_num_vec_coeffs = gb.get_bits1()? != 0;
            if transmit_num_vec_coeffs {
                let bits = floor_log2(subframe_len.div_ceil(4)) + 1;
                for &c in &active {
                    let n = (gb.get_bits(bits)? as usize) << 2;
                    if n > subframe_len {
                        return Err(DecoderError::InvalidData(
                            "WMA Pro num_vec_coeffs too large".into(),
                        ));
                    }
                    self.channel[c].num_vec_coeffs = n;
                }
            } else {
                for &c in &active {
                    self.channel[c].num_vec_coeffs = subframe_len;
                }
            }

            let mut step = gb.get_sbits(6)?;
            let mut quant_step = ((90 * self.bits_per_sample) >> 4) as i32 + step;
            if step == -32 || step == 31 {
                let sign = if step == 31 { 0 } else { -1 };
                let mut quant = 0i32;
                while gb.bits_read() + 5 < frame_bit_limit {
                    step = gb.get_bits(5)? as i32;
                    if step != 31 {
                        break;
                    }
                    quant += 31;
                }
                quant_step += ((quant + step) ^ sign) - sign;
            }

            if active.len() == 1 {
                self.channel[active[0]].quant_step = quant_step;
            } else {
                let modifier_len = gb.get_bits(3)? as usize;
                for &c in &active {
                    self.channel[c].quant_step = quant_step;
                    if gb.get_bits1()? != 0 {
                        self.channel[c].quant_step += if modifier_len != 0 {
                            gb.get_bits(modifier_len)? as i32 + 1
                        } else {
                            1
                        };
                    }
                }
            }
            self.decode_scale_factors(gb, &active, table_idx, num_bands)?;
        }

        for &c in &active {
            let range = coeff_offset..coeff_offset + subframe_len;
            self.channel[c].out[range.clone()].fill(0.0);
            if self.channel[c].transmit_coefs && gb.bits_read() < frame_bit_limit {
                self.decode_coeffs(
                    gb,
                    c,
                    coeff_offset,
                    subframe_len,
                    esc_len,
                    transmit_num_vec_coeffs,
                )?;
            }
        }

        if transmit_any {
            self.inverse_channel_transform(&groups, coeff_offset, subframe_len, &sfb, num_bands)?;
            for &c in &active {
                let scale_factors = self.channel[c].scale_factors;
                let max_sf = self.channel[c].max_scale_factor;
                let sf_step = self.channel[c].scale_factor_step;
                let qstep = self.channel[c].quant_step;
                let mut tmp = vec![0.0f32; subframe_len];
                for b in 0..num_bands {
                    let start = sfb[b].min(subframe_len);
                    let end = sfb[b + 1].min(subframe_len);
                    let exp = qstep - (max_sf - scale_factors[b]) * sf_step;
                    let quant = 10.0f32.powf(exp as f32 / 20.0);
                    for i in start..end {
                        tmp[i] = self.channel[c].out[coeff_offset + i] * quant;
                    }
                }
                if Some(c) == self.lfe_channel {
                    tmp[subwoofer_cutoff.min(subframe_len)..].fill(0.0);
                }
                let mdct_idx = floor_log2(subframe_len) - BLOCK_MIN_BITS;
                // FFmpeg initializes AV_TX_FLOAT_MDCT with inverse=1 and writes
                // the N-sample inverse transform directly at `coeffs`.  This is
                // the half-IMDCT / DCT-IV-shaped output used by WMA Pro's
                // overlap stage, not the 2N "full IMDCT" helper.
                let mut imdct = vec![0.0f32; subframe_len];
                self.mdct[mdct_idx].imdct_half(&mut imdct, &tmp);
                self.channel[c].out[coeff_offset..coeff_offset + subframe_len]
                    .copy_from_slice(&imdct);
            }
        }

        self.window_overlap(&active, coeff_offset, subframe_len)?;

        for &c in &active {
            if self.channel[c].cur_subframe >= self.channel[c].num_subframes {
                return Err(DecoderError::InvalidData("WMA Pro broken subframe".into()));
            }
            self.channel[c].cur_subframe += 1;
        }
        Ok(parsed_all)
    }

    fn decode_channel_transform(
        &mut self,
        gb: &mut GetBitContext<'_>,
        active: &[usize],
        num_bands: usize,
    ) -> Result<Vec<ChannelGroup>> {
        for ch in &mut self.channel {
            ch.grouped = false;
        }
        if self.channels <= 1 {
            return Ok(Vec::new());
        }
        if gb.get_bits1()? != 0 {
            return Err(DecoderError::Unsupported(
                "WMA Pro channel-transform extension".into(),
            ));
        }

        let mut groups = Vec::new();
        let mut remaining = active.len();
        while remaining != 0 && groups.len() < active.len() {
            let mut g = ChannelGroup::default();
            if remaining > 2 {
                for &c in active {
                    if !self.channel[c].grouped && gb.get_bits1()? != 0 {
                        self.channel[c].grouped = true;
                        g.channels.push(c);
                    }
                }
            } else {
                for &c in active {
                    if !self.channel[c].grouped {
                        self.channel[c].grouped = true;
                        g.channels.push(c);
                    }
                }
            }
            if g.channels.is_empty() {
                return Err(DecoderError::InvalidData(
                    "WMA Pro empty channel group".into(),
                ));
            }

            let n = g.channels.len();
            g.matrix = vec![0.0; n * n];
            if n == 2 {
                if gb.get_bits1()? != 0 {
                    if gb.get_bits1()? != 0 {
                        return Err(DecoderError::Unsupported(
                            "WMA Pro unknown stereo transform".into(),
                        ));
                    }
                } else {
                    g.transform = true;
                    let v = if self.channels == 2 { 1.0 } else { 0.70703125 };
                    g.matrix.copy_from_slice(&[v, -v, v, v]);
                }
            } else if n > 2 && gb.get_bits1()? != 0 {
                g.transform = true;
                if gb.get_bits1()? != 0 {
                    self.decode_custom_decorrelation_matrix(gb, &mut g)?;
                } else if n <= 6 {
                    let off = tables::DEFAULT_DECORRELATION_OFFSETS[n];
                    g.matrix
                        .copy_from_slice(&tables::DEFAULT_DECORRELATION_MATRICES[off..off + n * n]);
                } else {
                    return Err(DecoderError::Unsupported(
                        "WMA Pro default coupling > 6 channels".into(),
                    ));
                }
            }

            if g.transform {
                if gb.get_bits1()? == 0 {
                    for b in 0..num_bands {
                        g.transform_band[b] = gb.get_bits1()? != 0;
                    }
                } else {
                    g.transform_band[..num_bands].fill(true);
                }
            }
            remaining = remaining.saturating_sub(n);
            groups.push(g);
        }
        if remaining != 0 {
            return Err(DecoderError::InvalidData(
                "WMA Pro incomplete channel grouping".into(),
            ));
        }
        Ok(groups)
    }

    fn decode_custom_decorrelation_matrix(
        &self,
        gb: &mut GetBitContext<'_>,
        g: &mut ChannelGroup,
    ) -> Result<()> {
        let n = g.channels.len();
        let rotations = n * (n - 1) / 2;
        let mut rotation_offset = Vec::with_capacity(rotations);
        for _ in 0..rotations {
            rotation_offset.push(gb.get_bits(6)? as usize);
        }
        for i in 0..n {
            g.matrix[i * n + i] = if gb.get_bits1()? != 0 { 1.0 } else { -1.0 };
        }
        let mut off = 0usize;
        for i in 1..n {
            for x in 0..i {
                let r = rotation_offset[off + x];
                let angle = r as f32 * std::f32::consts::PI / 64.0;
                let sinv = angle.sin();
                let cosv = angle.cos();
                for y in 0..=i {
                    let v1 = g.matrix[x * n + y];
                    let v2 = g.matrix[i * n + y];
                    g.matrix[x * n + y] = v1 * sinv - v2 * cosv;
                    g.matrix[i * n + y] = v1 * cosv + v2 * sinv;
                }
            }
            off += i;
        }
        Ok(())
    }

    fn decode_scale_factors(
        &mut self,
        gb: &mut GetBitContext<'_>,
        active: &[usize],
        table_idx: usize,
        num_bands: usize,
    ) -> Result<()> {
        for &c in active {
            let saved_idx = self.channel[c].scale_factor_idx;
            let dst_idx = 1 - saved_idx;

            // FFmpeg keeps two persistent banks containing the last
            // *transmitted* scale factors, while `scale_factors` points at a
            // temporary/resampled bank for the current block.  In particular,
            // a block that reuses factors at a different size must use the
            // resampled values even when no new factors are transmitted.
            if self.channel[c].reuse_sf {
                let old_table = self.channel[c].table_idx;
                for b in 0..num_bands {
                    let src_b = self.sf_offsets[table_idx][old_table][b];
                    self.channel[c].scale_factors[b] =
                        self.channel[c].saved_scale_factors[saved_idx][src_b];
                }
            }

            let transmit = self.channel[c].cur_subframe == 0 || gb.get_bits1()? != 0;
            if transmit {
                if !self.channel[c].reuse_sf {
                    let step = gb.get_bits(2)? as i32 + 1;
                    self.channel[c].scale_factor_step = step;
                    let mut val = 45 / step;
                    for b in 0..num_bands {
                        val += get_vlc2(gb, &self.vlcs.sf.table, SCALEVLCBITS, 3)?;
                        self.channel[c].scale_factors[b] = val;
                    }
                } else {
                    let mut b = 0usize;
                    while b < num_bands {
                        let idx = get_vlc2(gb, &self.vlcs.sf_rl.table, VLCBITS, 3)?;
                        if idx < 0 {
                            return Err(DecoderError::InvalidData(
                                "WMA Pro scale-factor VLC".into(),
                            ));
                        }
                        if idx == 1 {
                            break;
                        }

                        let (skip, val, sign) = if idx == 0 {
                            let code = gb.get_bits(14)? as i32;
                            (((code & 0x3f) >> 1) as usize, code >> 6, (code & 1) - 1)
                        } else {
                            let i = idx as usize;
                            if i >= tables::SCALE_RL_RUN.len() {
                                return Err(DecoderError::InvalidData(
                                    "WMA Pro scale-factor VLC index".into(),
                                ));
                            }
                            (
                                tables::SCALE_RL_RUN[i] as usize,
                                tables::SCALE_RL_LEVEL[i] as i32,
                                gb.get_bits1()? as i32 - 1,
                            )
                        };

                        b += skip;
                        if b >= num_bands {
                            return Err(DecoderError::InvalidData(
                                "WMA Pro scale-factor run overflow".into(),
                            ));
                        }
                        self.channel[c].scale_factors[b] += (val ^ sign) - sign;
                        b += 1;
                    }
                }

                let current = self.channel[c].scale_factors;
                self.channel[c].saved_scale_factors[dst_idx][..num_bands]
                    .copy_from_slice(&current[..num_bands]);
                self.channel[c].scale_factor_idx = dst_idx;
                self.channel[c].table_idx = table_idx;
                self.channel[c].reuse_sf = true;
            }

            self.channel[c].max_scale_factor = self.channel[c].scale_factors[..num_bands]
                .iter()
                .copied()
                .max()
                .unwrap_or(0);
        }
        Ok(())
    }

    fn decode_coeffs(
        &mut self,
        gb: &mut GetBitContext<'_>,
        c: usize,
        coeff_offset: usize,
        subframe_len: usize,
        esc_len: usize,
        transmit_num_vec_coeffs: bool,
    ) -> Result<()> {
        let table_idx = gb.get_bits1()? as usize;
        let coef_vlc = &self.vlcs.coef[table_idx].table;
        let mut cur = 0usize;
        let mut zeros = 0usize;
        let mut rl_mode = false;
        let num_vec = self.channel[c].num_vec_coeffs;

        while (transmit_num_vec_coeffs || !rl_mode) && cur + 3 < num_vec {
            let idx = get_vlc2(gb, &self.vlcs.vec4.table, VLCBITS, 2)?;
            let mut vals = [0u32; 4];
            if idx < 0 {
                for base in [0usize, 2] {
                    let idx2 = get_vlc2(gb, &self.vlcs.vec2.table, VLCBITS, 2)?;
                    if idx2 < 0 {
                        for j in 0..2 {
                            let mut v = get_vlc2(gb, &self.vlcs.vec1.table, VLCBITS, 2)?;
                            if v < 0 {
                                return Err(DecoderError::InvalidData("WMA Pro vec1 VLC".into()));
                            }
                            if v as usize == tables::VEC1_TABLE.len() - 1 {
                                v += wma_get_large_val(gb)? as i32;
                            }
                            vals[base + j] = v as u32;
                        }
                    } else {
                        let packed = idx2 as u32;
                        vals[base] = packed >> 4;
                        vals[base + 1] = packed & 0x0f;
                    }
                }
            } else {
                let packed = idx as u32;
                vals = [
                    packed >> 12,
                    (packed >> 8) & 15,
                    (packed >> 4) & 15,
                    packed & 15,
                ];
            }

            for &v in &vals {
                if cur >= subframe_len {
                    break;
                }
                let out = coeff_offset + cur;
                if v != 0 {
                    let sign = gb.get_bits1()? as i32 - 1;
                    let bits = (v as f32).to_bits() ^ ((sign as u32) & 0x8000_0000);
                    self.channel[c].out[out] = f32::from_bits(bits);
                    zeros = 0;
                } else {
                    self.channel[c].out[out] = 0.0;
                    zeros += 1;
                    rl_mode |= zeros > (subframe_len >> 8);
                }
                cur += 1;
            }
        }

        if cur < subframe_len {
            while cur < subframe_len {
                let code = get_vlc2(gb, coef_vlc, VLCBITS, 3)?;
                if code == 1 {
                    break;
                }
                let (run, level) = if code > 1 {
                    let i = code as usize;
                    if table_idx == 0 {
                        if i >= tables::COEF0_RUN.len() {
                            return Err(DecoderError::InvalidData(
                                "WMA Pro coefficient VLC index".into(),
                            ));
                        }
                        (tables::COEF0_RUN[i], tables::COEF0_LEVEL[i])
                    } else {
                        if i >= tables::COEF1_RUN.len() {
                            return Err(DecoderError::InvalidData(
                                "WMA Pro coefficient VLC index".into(),
                            ));
                        }
                        (tables::COEF1_RUN[i], tables::COEF1_LEVEL[i])
                    }
                } else {
                    let level = wma_get_large_val(gb)? as f32;
                    let mut run = 0usize;
                    if gb.get_bits1()? != 0 {
                        if gb.get_bits1()? != 0 {
                            if gb.get_bits1()? != 0 {
                                return Err(DecoderError::InvalidData(
                                    "WMA Pro broken coefficient escape".into(),
                                ));
                            }
                            run = gb.get_bits(esc_len)? as usize + 4;
                        } else {
                            run = gb.get_bits(2)? as usize + 1;
                        }
                    }
                    cur = cur.saturating_add(run);
                    if cur >= subframe_len {
                        break;
                    }
                    let sign = gb.get_bits1()? as i32 - 1;
                    let bits = level.to_bits() ^ ((sign as u32) & 0x8000_0000);
                    self.channel[c].out[coeff_offset + cur] = f32::from_bits(bits);
                    cur += 1;
                    continue;
                };
                cur = cur.saturating_add(run as usize);
                if cur >= subframe_len {
                    break;
                }
                let sign = gb.get_bits1()? as i32 - 1;
                let bits = level.to_bits() ^ ((sign as u32) & 0x8000_0000);
                self.channel[c].out[coeff_offset + cur] = f32::from_bits(bits);
                cur += 1;
            }
        }
        if cur > subframe_len {
            return Err(DecoderError::InvalidData(
                "WMA Pro spectral RLE overflow".into(),
            ));
        }
        Ok(())
    }

    fn inverse_channel_transform(
        &mut self,
        groups: &[ChannelGroup],
        coeff_offset: usize,
        subframe_len: usize,
        sfb: &[usize; MAX_BANDS + 1],
        num_bands: usize,
    ) -> Result<()> {
        for g in groups {
            if !g.transform {
                continue;
            }
            let n = g.channels.len();
            for b in 0..num_bands {
                let start = sfb[b].min(subframe_len);
                let end = sfb[b + 1].min(subframe_len);
                if g.transform_band[b] {
                    for y in start..end {
                        let mut data = [0.0f32; MAX_CHANNELS];
                        let mut mapped = [0.0f32; MAX_CHANNELS];
                        for (i, &c) in g.channels.iter().enumerate() {
                            data[i] = self.channel[c].out[coeff_offset + y];
                        }
                        for row in 0..n {
                            let mut sum = 0.0f32;
                            for col in 0..n {
                                sum += data[col] * g.matrix[row * n + col];
                            }
                            mapped[row] = sum;
                        }
                        for (i, &c) in g.channels.iter().enumerate() {
                            self.channel[c].out[coeff_offset + y] = mapped[i];
                        }
                    }
                } else if self.channels == 2 {
                    let scale = 181.0f32 / 128.0;
                    for &c in &g.channels {
                        for y in start..end {
                            self.channel[c].out[coeff_offset + y] *= scale;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn window_overlap(
        &mut self,
        active: &[usize],
        coeff_offset: usize,
        subframe_len: usize,
    ) -> Result<()> {
        for &c in active {
            let mut winlen = self.channel[c].prev_block_len;
            let mut start = coeff_offset.saturating_sub(winlen / 2);
            if subframe_len < winlen {
                start += (winlen - subframe_len) / 2;
                winlen = subframe_len;
            }
            if !winlen.is_power_of_two() || winlen < (1 << BLOCK_MIN_BITS) {
                return Err(DecoderError::InvalidData(
                    "WMA Pro invalid overlap window".into(),
                ));
            }
            let widx = floor_log2(winlen) - BLOCK_MIN_BITS;
            let half = winlen / 2;
            let win = &self.windows[widx];
            if start + winlen > self.channel[c].out.len() || win.len() < winlen {
                return Err(DecoderError::InvalidData(
                    "WMA Pro overlap buffer overflow".into(),
                ));
            }
            vector_fmul_window_in_place(&mut self.channel[c].out[start..start + winlen], win, half);
            self.channel[c].prev_block_len = subframe_len;
        }
        Ok(())
    }
}

fn floor_log2(v: usize) -> usize {
    debug_assert!(v > 0);
    usize::BITS as usize - 1 - v.leading_zeros() as usize
}

fn sine_window(full_len: usize) -> Vec<f32> {
    // ff_sine_window_init(): sin((i + 0.5) * PI / (2 * n)).
    (0..full_len)
        .map(|i| ((i as f64 + 0.5) * std::f64::consts::PI / (2.0 * full_len as f64)).sin() as f32)
        .collect()
}

fn vector_fmul_window_in_place(buf: &mut [f32], win: &[f32], len: usize) {
    if len == 0 || buf.len() < len * 2 || win.len() < len * 2 {
        return;
    }
    let old = buf.to_vec();
    for k in 0..len {
        let s0 = old[k];
        let s1 = old[len * 2 - 1 - k];
        let wi = win[k];
        let wj = win[len * 2 - 1 - k];
        buf[k] = s0 * wj - s1 * wi;
        buf[len * 2 - 1 - k] = s0 * wi + s1 * wj;
    }
}

fn build_vlcs() -> Result<ProVlcs> {
    Ok(ProVlcs {
        sf: build_pairs_vlc(&tables::SCALE_TABLE, SCALEVLCBITS, -60)?,
        sf_rl: build_pairs_vlc(&tables::SCALE_RL_TABLE, VLCBITS, 0)?,
        coef: [
            build_lens_syms_vlc(&tables::COEF0_LENS, &tables::COEF0_SYMS, VLCBITS, 0)?,
            build_pairs_vlc(&tables::COEF1_TABLE, VLCBITS, 0)?,
        ],
        vec4: build_lens_syms_vlc(&tables::VEC4_LENS, &tables::VEC4_SYMS, VLCBITS, -1)?,
        vec2: build_pairs_vlc(&tables::VEC2_TABLE, VLCBITS, -1)?,
        vec1: build_pairs_vlc(&tables::VEC1_TABLE, VLCBITS, 0)?,
    })
}

fn build_pairs_vlc<const N: usize>(pairs: &[(u16, u8); N], bits: i32, offset: i32) -> Result<Vlc> {
    let lens: Vec<i8> = pairs.iter().map(|p| p.1 as i8).collect();
    let syms: Vec<u16> = pairs.iter().map(|p| p.0).collect();
    build_vlc(&lens, &syms, bits, offset)
}

fn build_lens_syms_vlc<const N: usize>(
    lens: &[u8; N],
    syms: &[u16; N],
    bits: i32,
    offset: i32,
) -> Result<Vlc> {
    let lens_i8: Vec<i8> = lens.iter().map(|&v| v as i8).collect();
    build_vlc(&lens_i8, syms, bits, offset)
}

fn build_vlc(lens: &[i8], syms: &[u16], bits: i32, offset: i32) -> Result<Vlc> {
    let mut sym_bytes = Vec::with_capacity(syms.len() * 2);
    for &s in syms {
        sym_bytes.extend_from_slice(&s.to_ne_bytes());
    }
    let mut vlc = Vlc::default();
    ff_vlc_init_from_lengths(
        &mut vlc,
        bits,
        lens.len(),
        lens,
        1,
        Some(&sym_bytes),
        2,
        2,
        offset,
        0,
    )?;
    Ok(vlc)
}

fn wma_get_large_val(gb: &mut GetBitContext<'_>) -> Result<u32> {
    let mut n = 8usize;
    if gb.get_bits1()? != 0 {
        n += 8;
        if gb.get_bits1()? != 0 {
            n += 8;
            if gb.get_bits1()? != 0 {
                n += 7;
            }
        }
    }
    gb.get_bits_long(n)
}

fn take_bits(gb: &mut GetBitContext<'_>, len: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(len.div_ceil(8));
    let mut acc = 0u8;
    let mut nacc = 0usize;
    for _ in 0..len {
        acc = (acc << 1) | gb.get_bits1()? as u8;
        nacc += 1;
        if nacc == 8 {
            out.push(acc);
            acc = 0;
            nacc = 0;
        }
    }
    if nacc != 0 {
        out.push(acc << (8 - nacc));
    }
    Ok(out)
}

fn append_bits_from_reader(
    dst: &mut Vec<u8>,
    dst_bits: &mut usize,
    gb: &mut GetBitContext<'_>,
    len: usize,
) -> Result<()> {
    for _ in 0..len {
        push_bit(dst, dst_bits, gb.get_bits1()? as u8);
    }
    Ok(())
}

fn push_bit(dst: &mut Vec<u8>, bits: &mut usize, bit: u8) {
    let byte = *bits / 8;
    let shift = 7 - (*bits % 8);
    if byte == dst.len() {
        dst.push(0);
    }
    if bit != 0 {
        dst[byte] |= 1 << shift;
    }
    *bits += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coefficient_run_level_layout_matches_ffmpeg_sizes() {
        assert_eq!((tables::COEF0_RUN[2], tables::COEF0_LEVEL[2]), (0, 1.0));
        assert_eq!(
            (tables::COEF0_RUN[151], tables::COEF0_LEVEL[151]),
            (149, 1.0)
        );
        assert_eq!(
            (tables::COEF0_RUN[271], tables::COEF0_LEVEL[271]),
            (0, 28.0)
        );
        assert_eq!((tables::COEF1_RUN[2], tables::COEF1_LEVEL[2]), (0, 1.0));
        assert_eq!(
            (tables::COEF1_RUN[101], tables::COEF1_LEVEL[101]),
            (99, 1.0)
        );
        assert_eq!(
            (tables::COEF1_RUN[243], tables::COEF1_LEVEL[243]),
            (0, 52.0)
        );
    }

    #[test]
    fn pro_vlc_tables_build() {
        let v = build_vlcs().expect("FFmpeg WMA Pro VLC tables must be canonical");
        assert!(!v.sf.table.is_empty());
        assert!(!v.coef[0].table.is_empty());
        assert!(!v.vec4.table.is_empty());
    }
}

//! Public library API.

#[cfg(target_os = "uefi")]
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
#[cfg(target_os = "uefi")]
use std::hash::BuildHasherDefault;
use std::io::{Read, Seek, SeekFrom};

use crate::asf::{AsfFile, AsfPayload, VideoStreamInfo};
use crate::decoder::{MacroblockDecoder, YuvFrame};
use crate::error::{DecoderError, Result};
use crate::vc1::{PictureHeader, SequenceHeader, vc1_unescape_buffer};
#[cfg(feature = "audio")]
use crate::wma::{PcmFrameF32, WmaDecoder, WmaProDecoder};
use crate::wmv2::{Wmv2FrameHeader, Wmv2FrameType, Wmv2Params};

/// A decoded video frame with timing metadata.
#[derive(Clone)]
pub struct DecodedFrame {
    pub pts_ms: u32,
    pub is_key_frame: bool,
    pub frame: YuvFrame,
}

/// A decoded audio frame with timing metadata.
#[cfg(feature = "audio")]
#[derive(Clone)]
pub struct DecodedAudioFrame {
    pub pts_ms: u32,
    pub frame: PcmFrameF32,
}

/// WMV2 (Windows Media Video 8) decoder.
///
/// The picture header parsing and macroblock decode paths are aligned with upstream.
pub struct Wmv2Decoder {
    params: Wmv2Params,
    mb_dec: MacroblockDecoder,
    cur: YuvFrame,
    locked_hdr_off: Option<usize>,
}

impl Wmv2Decoder {
    /// Create a decoder for a fixed resolution.
    ///
    /// `extradata` is the 4-byte WMV2 ext header typically carried in ASF stream properties.
    pub fn new(width: u32, height: u32, extradata: &[u8]) -> Self {
        let params = Wmv2Params::new(width, height);
        let mut mb_dec = MacroblockDecoder::new(width, height);
        mb_dec.wmv2_set_extradata(extradata);
        let cur = YuvFrame::new(width, height);
        Self {
            params,
            mb_dec,
            cur,
            locked_hdr_off: None,
        }
    }

    pub fn width(&self) -> u32 {
        self.params.width
    }

    pub fn height(&self) -> u32 {
        self.params.height
    }

    /// Borrow the internal YUV420p frame buffer.
    ///
    /// The returned reference stays valid until the next successful decode.
    pub fn current_frame(&self) -> &YuvFrame {
        &self.cur
    }

    /// Decode one assembled WMV2 frame payload.
    ///
    /// Returns `Ok(None)` if no plausible picture header can be found.
    pub fn decode_frame(
        &mut self,
        payload: &[u8],
        is_key_frame: bool,
    ) -> Result<Option<&YuvFrame>> {
        if payload.is_empty() {
            return Ok(None);
        }

        let mut best_score: i64 = -1;
        let mut best_off: usize = 0;
        let mut best_hdr: Option<Wmv2FrameHeader> = None;

        // Try the previously locked offset first, then fall back to a small scan.
        let mut offs: Vec<usize> = Vec::with_capacity(18);
        if let Some(o) = self.locked_hdr_off {
            offs.push(o);
        }
        for o in 0..=16 {
            if Some(o) != self.locked_hdr_off {
                offs.push(o);
            }
        }

        for off in offs {
            if off > payload.len() {
                continue;
            }
            let cands = Wmv2FrameHeader::parse_candidates(
                &payload[off..],
                self.mb_dec.width_mb,
                self.mb_dec.height_mb,
            );
            if cands.is_empty() {
                continue;
            }
            for h in cands {
                // ASF keyframe marking should correspond to WMV2 I pictures.
                if is_key_frame && h.frame_type != Wmv2FrameType::I {
                    continue;
                }

                // upstream-aligned scoring strategy.
                let mut sc: i64 = if h.frame_skipped {
                    1
                } else if is_key_frame {
                    2
                } else {
                    self.mb_dec.probe_wmv2_payload(&payload[off..], &h) as i64
                };

                if Some(off) == self.locked_hdr_off {
                    sc += 64;
                }

                if sc > best_score {
                    best_score = sc;
                    best_off = off;
                    best_hdr = Some(h);
                }
            }
        }

        let Some(hdr) = best_hdr else {
            return Ok(None);
        };

        if self.locked_hdr_off.is_none() {
            self.locked_hdr_off = Some(best_off);
        }

        let frame_data = &payload[best_off..];
        self.mb_dec
            .decode_wmv2_frame(frame_data, &hdr, &self.params, &mut self.cur)?;
        Ok(Some(&self.cur))
    }

    /// Decode and return an owned frame buffer (clone).
    pub fn decode_frame_owned(
        &mut self,
        payload: &[u8],
        is_key_frame: bool,
    ) -> Result<Option<YuvFrame>> {
        let Some(f) = self.decode_frame(payload, is_key_frame)? else {
            return Ok(None);
        };
        Ok(Some(f.clone()))
    }
}

/// Native WMV3 (VC-1 Simple/Main profile) decoder.
pub struct Wmv3Decoder {
    seq: SequenceHeader,
    mb_dec: MacroblockDecoder,
    cur: YuvFrame,
}

impl Wmv3Decoder {
    pub fn new(width: u32, height: u32, extradata: &[u8]) -> Result<Self> {
        let mut seq = SequenceHeader::parse(extradata)?;
        seq.width = width;
        seq.height = height;
        seq.display_width = width;
        seq.display_height = height;

        // VC-1 Simple/Main reconstructs a complete macroblock surface, not only
        // the display rectangle.  FFmpeg likewise sets h_edge_pos/v_edge_pos to
        // mb_width*16 / mb_height*16.  Those non-display pixels are real coded
        // reference samples: later P/B motion compensation may legally point
        // into the final partial macroblock row/column.  Decoding directly into
        // a width*height surface discards them (e.g. rows 1080..1087 for 1080p)
        // and turns later MC into visible-edge replication.
        let coded_width = width
            .checked_add(15)
            .ok_or_else(|| DecoderError::InvalidData("WMV3 width overflow".into()))?
            / 16
            * 16;
        let coded_height = height
            .checked_add(15)
            .ok_or_else(|| DecoderError::InvalidData("WMV3 height overflow".into()))?
            / 16
            * 16;

        Ok(Self {
            seq,
            mb_dec: MacroblockDecoder::new(coded_width, coded_height),
            cur: YuvFrame::new(coded_width, coded_height),
        })
    }

    pub fn width(&self) -> u32 {
        self.seq.width
    }
    pub fn height(&self) -> u32 {
        self.seq.height
    }

    fn visible_frame(&self) -> YuvFrame {
        let width = self.seq.width as usize;
        let height = self.seq.height as usize;
        let src_width = self.cur.width as usize;
        let src_height = self.cur.height as usize;
        debug_assert!(width <= src_width && height <= src_height);

        let mut out = YuvFrame::new(self.seq.width, self.seq.height);
        for y in 0..height {
            let src = y * src_width;
            let dst = y * width;
            out.y[dst..dst + width].copy_from_slice(&self.cur.y[src..src + width]);
        }

        let cw = width / 2;
        let ch = height / 2;
        let src_cw = src_width / 2;
        for y in 0..ch {
            let src = y * src_cw;
            let dst = y * cw;
            out.cb[dst..dst + cw].copy_from_slice(&self.cur.cb[src..src + cw]);
            out.cr[dst..dst + cw].copy_from_slice(&self.cur.cr[src..src + cw]);
        }
        out
    }

    pub fn decode_frame_owned(
        &mut self,
        payload: &[u8],
        is_key_frame: bool,
        pts_ms: u32,
    ) -> Result<Option<YuvFrame>> {
        if payload.is_empty() {
            return Ok(None);
        }
        let mb_w = self.seq.width.div_ceil(16) as usize;
        let mb_h = self.seq.height.div_ceil(16) as usize;
        let hdr = PictureHeader::parse(payload, &self.seq, pts_ms, mb_w, mb_h)?;
        if is_key_frame
            && !matches!(
                hdr.frame_type,
                crate::vc1::FrameType::I | crate::vc1::FrameType::BI
            )
        {
            log::debug!(
                "ASF key-frame flag disagrees with WMV3 PTYPE: {:?}",
                hdr.frame_type
            );
        }
        self.mb_dec
            .decode_frame(payload, &hdr, &self.seq, &mut self.cur)?;
        // Keep the macroblock-aligned surface internally for future references;
        // only crop when handing a frame to callers/rendering.
        Ok(Some(self.visible_frame()))
    }
}

/// Native VC-1 Advanced Profile decoder for ASF/WVC1 streams.
///
/// The compressed picture reconstruction is shared with the WMV3 decoder,
/// while WVC1 uses Advanced Profile sequence/entry-point syntax and VC-1
/// emulation-prevention escaping at the transport boundary.
pub struct Wvc1Decoder {
    seq: SequenceHeader,
    mb_dec: MacroblockDecoder,
    cur: YuvFrame,
}

impl Wvc1Decoder {
    pub fn new(asf_width: u32, asf_height: u32, extradata: &[u8]) -> Result<Self> {
        let mut seq = SequenceHeader::parse_wvc1(extradata)?;
        if seq.interlace {
            return Err(DecoderError::Unsupported(
                "VC-1 Advanced interlaced pictures are not implemented by the native decoder"
                    .into(),
            ));
        }
        if seq.range_mapy.is_some() || seq.range_mapuv.is_some() {
            return Err(DecoderError::Unsupported(
                "VC-1 Advanced range mapping is not implemented by the native decoder".into(),
            ));
        }

        // ASF BITMAPINFOHEADER dimensions describe the displayed video while
        // the Advanced sequence/entry-point headers describe the coded
        // reference surface. They normally match; refuse contradictory
        // metadata rather than silently decoding with the wrong MB geometry.
        if asf_width != 0 && asf_width > seq.width {
            return Err(DecoderError::InvalidData(format!(
                "WVC1 ASF width {asf_width} exceeds coded width {}",
                seq.width
            )));
        }
        if asf_height != 0 && asf_height > seq.height {
            return Err(DecoderError::InvalidData(format!(
                "WVC1 ASF height {asf_height} exceeds coded height {}",
                seq.height
            )));
        }
        if asf_width != 0 {
            seq.display_width = asf_width;
        }
        if asf_height != 0 {
            seq.display_height = asf_height;
        }

        let coded_width = seq
            .width
            .checked_add(15)
            .ok_or_else(|| DecoderError::InvalidData("WVC1 width overflow".into()))?
            / 16
            * 16;
        let coded_height = seq
            .height
            .checked_add(15)
            .ok_or_else(|| DecoderError::InvalidData("WVC1 height overflow".into()))?
            / 16
            * 16;

        Ok(Self {
            seq,
            mb_dec: MacroblockDecoder::new(coded_width, coded_height),
            cur: YuvFrame::new(coded_width, coded_height),
        })
    }

    pub fn width(&self) -> u32 {
        self.seq.display_width.min(self.seq.width)
    }

    pub fn height(&self) -> u32 {
        self.seq.display_height.min(self.seq.height)
    }

    fn visible_frame(&self) -> YuvFrame {
        let width = self.seq.display_width.min(self.seq.width) as usize;
        let height = self.seq.display_height.min(self.seq.height) as usize;
        let src_width = self.cur.width as usize;
        let mut out = YuvFrame::new(width as u32, height as u32);

        for y in 0..height {
            let src = y * src_width;
            let dst = y * width;
            out.y[dst..dst + width].copy_from_slice(&self.cur.y[src..src + width]);
        }

        let cw = width / 2;
        let ch = height / 2;
        let src_cw = src_width / 2;
        for y in 0..ch {
            let src = y * src_cw;
            let dst = y * cw;
            out.cb[dst..dst + cw].copy_from_slice(&self.cur.cb[src..src + cw]);
            out.cr[dst..dst + cw].copy_from_slice(&self.cur.cr[src..src + cw]);
        }
        out
    }

    fn marker_at(data: &[u8], from: usize) -> Option<usize> {
        if data.len() < 4 || from > data.len().saturating_sub(4) {
            return None;
        }
        (from..=data.len() - 4).find(|&i| data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1)
    }

    /// Convert one ASF WVC1 media object into the raw Advanced picture
    /// bitstream consumed by `PictureHeader`/`MacroblockDecoder`.
    ///
    /// FFmpeg accepts both bare escaped WVC1 pictures and start-code-delimited
    /// VC-1 access units. Handle both forms and apply in-band entry points.
    fn prepare_picture_payload(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        let Some(first) = Self::marker_at(payload, 0) else {
            return Ok(vc1_unescape_buffer(payload));
        };

        // Bytes before the first recognized marker are not picture syntax.
        // WVC1 ASF normally starts at the marker when markers are present.
        let mut pos = first;
        let mut frame: Option<Vec<u8>> = None;
        while let Some(start) = Self::marker_at(payload, pos) {
            let marker = payload[start + 3];
            let next = Self::marker_at(payload, start + 4).unwrap_or(payload.len());
            let body = &payload[start + 4..next];
            match marker {
                0x0d => {
                    frame = Some(vc1_unescape_buffer(body));
                }
                0x0e => {
                    let entry = vc1_unescape_buffer(body);
                    let old_width = self.seq.width;
                    let old_height = self.seq.height;
                    self.seq.apply_wvc1_entry_point(&entry)?;
                    if self.seq.width != old_width || self.seq.height != old_height {
                        return Err(DecoderError::Unsupported(format!(
                            "in-band WVC1 coded-size change {}x{} -> {}x{} requires decoder reinitialization",
                            old_width, old_height, self.seq.width, self.seq.height
                        )));
                    }
                    if self.seq.interlace {
                        return Err(DecoderError::Unsupported(
                            "VC-1 Advanced interlaced pictures are not implemented by the native decoder"
                                .into(),
                        ));
                    }
                    if self.seq.range_mapy.is_some() || self.seq.range_mapuv.is_some() {
                        return Err(DecoderError::Unsupported(
                            "VC-1 Advanced range mapping is not implemented by the native decoder"
                                .into(),
                        ));
                    }
                }
                0x0b => {
                    return Err(DecoderError::Unsupported(
                        "VC-1 Advanced slice start codes are not implemented by the native decoder"
                            .into(),
                    ));
                }
                0x0c => {
                    return Err(DecoderError::Unsupported(
                        "VC-1 Advanced field start codes require interlaced decoding".into(),
                    ));
                }
                0x0f => {
                    // A new sequence header changes coded dimensions/profile
                    // state and requires decoder-buffer reinitialization. ASF
                    // WVC1 carries the stable sequence header in extradata;
                    // reject an in-band format switch instead of reusing stale
                    // reference surfaces.
                    return Err(DecoderError::Unsupported(
                        "in-band VC-1 Advanced sequence changes are not supported".into(),
                    ));
                }
                0x0a | 0x00..=0x09 => {}
                _ => {}
            }
            if next >= payload.len() {
                break;
            }
            pos = next;
        }

        frame.ok_or_else(|| {
            DecoderError::InvalidData("WVC1 access unit has no VC-1 frame start code".into())
        })
    }

    pub fn decode_frame_owned(
        &mut self,
        payload: &[u8],
        is_key_frame: bool,
        pts_ms: u32,
    ) -> Result<Option<YuvFrame>> {
        if payload.is_empty() {
            return Ok(None);
        }
        let frame_payload = self.prepare_picture_payload(payload)?;
        if frame_payload.is_empty() {
            return Ok(None);
        }
        let mb_w = self.seq.width.div_ceil(16) as usize;
        let mb_h = self.seq.height.div_ceil(16) as usize;
        let hdr = PictureHeader::parse(&frame_payload, &self.seq, pts_ms, mb_w, mb_h)?;
        if is_key_frame
            && !matches!(
                hdr.frame_type,
                crate::vc1::FrameType::I | crate::vc1::FrameType::BI
            )
        {
            log::debug!(
                "ASF key-frame flag disagrees with WVC1 PTYPE: {:?}",
                hdr.frame_type
            );
        }
        self.mb_dec
            .decode_frame(&frame_payload, &hdr, &self.seq, &mut self.cur)?;
        Ok(Some(self.visible_frame()))
    }
}

enum VideoCodecDecoder {
    Wmv12(Wmv2Decoder),
    Wmv3(Wmv3Decoder),
    Wvc1(Wvc1Decoder),
}

impl VideoCodecDecoder {
    fn decode_frame_owned(
        &mut self,
        payload: &[u8],
        is_key: bool,
        pts_ms: u32,
    ) -> Result<Option<YuvFrame>> {
        match self {
            Self::Wmv12(d) => d.decode_frame_owned(payload, is_key),
            Self::Wmv3(d) => d.decode_frame_owned(payload, is_key, pts_ms),
            Self::Wvc1(d) => d.decode_frame_owned(payload, is_key, pts_ms),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ASF media-object reassembly (frame reassembly)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FrameKey {
    stream_number: u8,
    object_id: u32,
}

#[derive(Debug, Clone)]
struct FrameAssembly {
    total: usize,
    pts_ms: u32,
    is_key: bool,
    data: Vec<u8>,
    ranges: Vec<(usize, usize)>,
}

impl FrameAssembly {
    fn new(total: usize, pts_ms: u32, is_key: bool) -> Self {
        Self {
            total,
            pts_ms,
            is_key,
            data: vec![0u8; total],
            ranges: Vec::new(),
        }
    }

    fn insert(&mut self, offset: usize, frag: &[u8]) {
        if self.total == 0 || offset >= self.total || frag.is_empty() {
            return;
        }
        let end = (offset + frag.len()).min(self.total);
        let n = end - offset;
        self.data[offset..end].copy_from_slice(&frag[..n]);
        self.add_range(offset, end);
    }

    fn add_range(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        self.ranges.push((start, end));
        self.ranges.sort_by_key(|r| r.0);

        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(self.ranges.len());
        for (s, e) in self.ranges.drain(..) {
            if let Some(last) = merged.last_mut()
                && s <= last.1
            {
                last.1 = last.1.max(e);
                continue;
            }
            merged.push((s, e));
        }
        self.ranges = merged;
    }

    fn covered_len(&self) -> usize {
        self.ranges.iter().map(|(s, e)| e - s).sum()
    }

    fn is_complete(&self) -> bool {
        self.total > 0
            && self.covered_len() >= self.total
            && self.ranges.len() == 1
            && self.ranges[0] == (0, self.total)
    }
}

#[cfg(target_os = "uefi")]
type InFlightMap = HashMap<FrameKey, FrameAssembly, BuildHasherDefault<DefaultHasher>>;
#[cfg(not(target_os = "uefi"))]
type InFlightMap = HashMap<FrameKey, FrameAssembly>;

#[derive(Default)]
struct FrameAssembler {
    in_flight: InFlightMap,
}

impl FrameAssembler {
    fn push(&mut self, payload: AsfPayload) -> Option<(u32, bool, Vec<u8>)> {
        if payload.data.is_empty() {
            return None;
        }

        let key = FrameKey {
            stream_number: payload.stream_number,
            object_id: payload.object_id,
        };

        // Fast path: complete media object in one payload (or size unknown).
        if payload.obj_offset == 0 {
            let osz = payload.obj_size as usize;
            if osz == 0 || osz == payload.data.len() {
                return Some((payload.pts_ms, payload.is_key_frame, payload.data));
            }
        }

        // If the total object size is unknown, we cannot reliably reassemble.
        if payload.obj_size == 0 {
            return Some((payload.pts_ms, payload.is_key_frame, payload.data));
        }

        let total = payload.obj_size as usize;
        let entry = self
            .in_flight
            .entry(key)
            .or_insert_with(|| FrameAssembly::new(total, payload.pts_ms, payload.is_key_frame));

        // Update meta (first PTS wins; keyframe if any fragment says so).
        entry.is_key |= payload.is_key_frame;
        entry.insert(payload.obj_offset as usize, &payload.data);

        if entry.is_complete() {
            let assembly = self.in_flight.remove(&key).unwrap();
            return Some((assembly.pts_ms, assembly.is_key, assembly.data));
        }
        None
    }
}

/// ASF + WMV2 decoding pipeline.
///
/// This type owns the `Read+Seek` source, parses ASF headers, reassembles media objects
/// and decodes WMV2 frames into `YuvFrame`.
pub struct AsfWmv2Decoder<R: Read + Seek> {
    reader: R,
    asf: AsfFile,
    video_info: VideoStreamInfo,
    assembler: FrameAssembler,
    /// Completed ASF media objects already returned by `AsfFile::read_packet`
    /// but not yet consumed by `next_frame`. A single ASF packet may contain
    /// multiple compressed payloads/media objects; dropping the tail after
    /// returning the first decoded picture corrupts the VC-1 reference chain.
    pending_payloads: VecDeque<AsfPayload>,
    decoder: VideoCodecDecoder,
}

/// ASF + WMA decoding pipeline (WMA v1/v2 and WMA Professional).
///
/// This type owns the `Read+Seek` source, parses ASF headers, reassembles media objects
/// and decodes WMA packets into PCM.
#[cfg(feature = "audio")]
enum AudioCodecDecoder {
    Wma12(Box<WmaDecoder>),
    WmaPro(Box<WmaProDecoder>),
}

#[cfg(feature = "audio")]
impl AudioCodecDecoder {
    fn sample_rate(&self) -> u32 {
        match self {
            Self::Wma12(d) => d.sample_rate(),
            Self::WmaPro(d) => d.sample_rate(),
        }
    }

    fn channels(&self) -> u16 {
        match self {
            Self::Wma12(d) => d.channels(),
            Self::WmaPro(d) => d.channels(),
        }
    }

    fn decode_packet(&mut self, packet: &[u8], pts_ms: u32) -> Result<Option<PcmFrameF32>> {
        match self {
            Self::Wma12(d) => d.decode_packet(packet, pts_ms),
            Self::WmaPro(d) => d.decode_packet(packet, pts_ms),
        }
    }
}

#[cfg(feature = "audio")]
pub struct AsfWmaDecoder<R: Read + Seek> {
    reader: R,
    asf: AsfFile,
    audio_stream_number: u8,
    audio_format_tag: u16,
    audio_block_align: u16,
    decoder: AudioCodecDecoder,
    assembler: FrameAssembler,
    /// Completed ASF media objects already returned by `AsfFile::read_packet`
    /// but not yet consumed by `next_frame`. As with WMV video, one ASF packet
    /// can contain multiple WMA media objects. Returning one decoded PCM frame
    /// must not discard the remaining objects because WMA superframes / bit
    /// reservoir state depends on consuming the stream strictly in order.
    pending_payloads: VecDeque<AsfPayload>,
    last_pts_ms: u32,
    flushed_eof: bool,
}

#[cfg(feature = "audio")]
impl<R: Read + Seek> AsfWmaDecoder<R> {
    /// Open an ASF/WMV stream and initialize the WMA decoder.
    ///
    /// The decoder selects the first audio stream with format tag 0x0160 (WMAv1),
    /// 0x0161 (WMAv2), or 0x0162 (WMA Professional).
    pub fn open(mut reader: R) -> Result<Self> {
        let asf = AsfFile::open(&mut reader)?;
        let mut chosen = None;
        for a in asf.audio_streams.iter() {
            if matches!(a.format_tag, 0x0160..=0x0162) {
                chosen = Some(a.clone());
                break;
            }
        }
        let Some(audio_info) = chosen else {
            return Err(DecoderError::Unsupported(
                "No supported WMA (0x0160/0x0161/0x0162) audio stream found".into(),
            ));
        };

        reader.seek(SeekFrom::Start(asf.data_offset))?;
        let decoder = match audio_info.format_tag {
            0x0162 => AudioCodecDecoder::WmaPro(Box::new(WmaProDecoder::new(&audio_info)?)),
            _ => AudioCodecDecoder::Wma12(Box::new(WmaDecoder::new(&audio_info)?)),
        };

        Ok(Self {
            reader,
            asf,
            audio_stream_number: audio_info.stream_number,
            audio_format_tag: audio_info.format_tag,
            audio_block_align: audio_info.block_align,
            decoder,
            assembler: FrameAssembler::default(),
            pending_payloads: VecDeque::new(),
            last_pts_ms: 0,
            flushed_eof: false,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.decoder.sample_rate()
    }

    pub fn channels(&self) -> u16 {
        self.decoder.channels()
    }

    pub fn duration_ms(&self) -> Option<u64> {
        self.asf.play_duration_ms
    }

    /// Decode the next audio frame.
    ///
    /// Returns `Ok(None)` on end-of-stream.
    pub fn next_frame(&mut self) -> Result<Option<DecodedAudioFrame>> {
        loop {
            // `AsfFile::read_packet` can complete multiple WMA media objects in
            // one ASF packet. `next_frame` returns only one PCM frame, so keep
            // the packet tail across calls exactly like `AsfWmv2Decoder`.
            // Dropping those objects desynchronizes stateful WMA superframes /
            // bit reservoir and eventually produces frame-length/bitstream
            // overflow errors even though the ASF stream itself is valid.
            let payload = if let Some(payload) = self.pending_payloads.pop_front() {
                payload
            } else {
                let payloads = match self.asf.read_packet(&mut self.reader) {
                    Ok(p) => p,
                    Err(DecoderError::EndOfStream) => {
                        if self.flushed_eof {
                            return Ok(None);
                        }
                        self.flushed_eof = true;
                        if let Some(frame) = self.decoder.decode_packet(&[], self.last_pts_ms)? {
                            return Ok(Some(DecodedAudioFrame {
                                pts_ms: frame.pts_ms,
                                frame,
                            }));
                        }
                        return Ok(None);
                    }
                    Err(e) => return Err(e),
                };
                self.pending_payloads.extend(
                    payloads
                        .into_iter()
                        .filter(|payload| payload.stream_number == self.audio_stream_number),
                );
                let Some(payload) = self.pending_payloads.pop_front() else {
                    continue;
                };
                payload
            };

            let Some((pts_ms, _is_key, data)) = self.assembler.push(payload) else {
                continue;
            };
            self.last_pts_ms = pts_ms;
            let audio_format_tag = self.audio_format_tag;
            let audio_block_align = self.audio_block_align;
            let media_object_len = data.len();
            let frame = self.decoder.decode_packet(&data, pts_ms).map_err(|err| {
                let context = format!(
                    "WMA tag=0x{audio_format_tag:04x} block_align={audio_block_align} media_object_len={media_object_len} pts_ms={pts_ms}",
                );
                match err {
                    DecoderError::InvalidData(message) => {
                        DecoderError::InvalidData(format!("{context}: {message}"))
                    }
                    DecoderError::Unsupported(message) => {
                        DecoderError::Unsupported(format!("{context}: {message}"))
                    }
                    other => other,
                }
            })?;
            if let Some(frame) = frame {
                return Ok(Some(DecodedAudioFrame { pts_ms, frame }));
            }
        }
    }
}

impl<R: Read + Seek> AsfWmv2Decoder<R> {
    /// Open an ASF/WMV stream and initialize the WMV2 decoder.
    ///
    /// The decoder selects the first video stream whose FourCC is WMV1, WMV2, WMV3, or WVC1.
    pub fn open(mut reader: R) -> Result<Self> {
        let asf = AsfFile::open(&mut reader)?;
        let mut video_info: Option<VideoStreamInfo> = None;
        for v in asf.video_streams.iter() {
            let four_cc = std::str::from_utf8(&v.codec_four_cc)
                .unwrap_or("")
                .to_uppercase();
            if matches!(four_cc.as_str(), "WMV3" | "WMV2" | "WMV1" | "WVC1") {
                video_info = Some(v.clone());
                break;
            }
        }
        let Some(video_info) = video_info else {
            return Err(DecoderError::Unsupported(
                "No supported WMV1/WMV2/WMV3/WVC1 video stream found".into(),
            ));
        };

        reader.seek(SeekFrom::Start(asf.data_offset))?;

        let four_cc = std::str::from_utf8(&video_info.codec_four_cc)
            .unwrap_or("")
            .to_uppercase();
        let decoder = if four_cc == "WMV3" {
            VideoCodecDecoder::Wmv3(Wmv3Decoder::new(
                video_info.width,
                video_info.height,
                &video_info.extra_data,
            )?)
        } else if four_cc == "WVC1" {
            VideoCodecDecoder::Wvc1(Wvc1Decoder::new(
                video_info.width,
                video_info.height,
                &video_info.extra_data,
            )?)
        } else {
            VideoCodecDecoder::Wmv12(Wmv2Decoder::new(
                video_info.width,
                video_info.height,
                &video_info.extra_data,
            ))
        };

        Ok(Self {
            reader,
            asf,
            video_info,
            assembler: FrameAssembler::default(),
            pending_payloads: VecDeque::new(),
            decoder,
        })
    }

    /// Return the selected video stream info.
    pub fn video_stream_info(&self) -> &VideoStreamInfo {
        &self.video_info
    }

    pub fn duration_ms(&self) -> Option<u64> {
        self.asf.play_duration_ms
    }

    /// Decode the next video frame.
    ///
    /// Returns `Ok(None)` on end-of-stream.
    pub fn next_frame(&mut self) -> Result<Option<DecodedFrame>> {
        loop {
            // `AsfFile::read_packet` returns every completed media object from
            // one ASF packet. In particular ASF compressed payloads may carry
            // many tiny WMV pictures in the same packet. `next_frame` returns
            // only one picture to its caller, so preserve the remaining media
            // objects here and consume them before reading another ASF packet.
            //
            // The old implementation iterated the local `payloads` vector and
            // returned as soon as the first picture decoded. Rust then dropped
            // the rest of that vector. Losing even a skip/P picture changes the
            // VC-1 reference state and makes all following predicted pictures
            // decode against the wrong anchor.
            let payload = if let Some(payload) = self.pending_payloads.pop_front() {
                payload
            } else {
                let payloads = match self.asf.read_packet(&mut self.reader) {
                    Ok(p) => p,
                    Err(DecoderError::EndOfStream) => return Ok(None),
                    Err(e) => return Err(e),
                };
                self.pending_payloads.extend(
                    payloads
                        .into_iter()
                        .filter(|payload| payload.stream_number == self.video_info.stream_number),
                );
                let Some(payload) = self.pending_payloads.pop_front() else {
                    continue;
                };
                payload
            };

            let Some((pts_ms, is_key, data)) = self.assembler.push(payload) else {
                continue;
            };

            if let Some(frame) = self.decoder.decode_frame_owned(&data, is_key, pts_ms)? {
                return Ok(Some(DecodedFrame {
                    pts_ms,
                    is_key_frame: is_key,
                    frame,
                }));
            }
        }
    }
}

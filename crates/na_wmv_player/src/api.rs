//! Public library API.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use crate::asf::{AsfFile, AsfPayload, VideoStreamInfo};
use crate::decoder::{MacroblockDecoder, YuvFrame};
use crate::error::{DecoderError, Result};
use crate::wma::{PcmFrameF32, WmaDecoder};
use crate::wmv2::{Wmv2FrameHeader, Wmv2FrameType, Wmv2Params};

/// A decoded video frame with timing metadata.
#[derive(Clone)]
pub struct DecodedFrame {
    pub pts_ms: u32,
    pub is_key_frame: bool,
    pub frame: YuvFrame,
}

/// A decoded audio frame with timing metadata.
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
    pub fn decode_frame(&mut self, payload: &[u8], is_key_frame: bool) -> Result<Option<&YuvFrame>> {
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
            let cands = Wmv2FrameHeader::parse_candidates(&payload[off..], self.mb_dec.width_mb, self.mb_dec.height_mb);
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
        self.mb_dec.decode_wmv2_frame(frame_data, &hdr, &self.params, &mut self.cur)?;
        Ok(Some(&self.cur))
    }

    /// Decode and return an owned frame buffer (clone).
    pub fn decode_frame_owned(&mut self, payload: &[u8], is_key_frame: bool) -> Result<Option<YuvFrame>> {
        let Some(f) = self.decode_frame(payload, is_key_frame)? else {
            return Ok(None);
        };
        Ok(Some(f.clone()))
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
            if let Some(last) = merged.last_mut() {
                if s <= last.1 {
                    last.1 = last.1.max(e);
                    continue;
                }
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

#[derive(Default)]
struct FrameAssembler {
    in_flight: HashMap<FrameKey, FrameAssembly>,
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
    decoder: Wmv2Decoder,
}

/// ASF + WMA (v1/v2) decoding pipeline.
///
/// This type owns the `Read+Seek` source, parses ASF headers, reassembles media objects
/// and decodes WMA packets into PCM.
pub struct AsfWmaDecoder<R: Read + Seek> {
    reader: R,
    asf: AsfFile,
    audio_stream_number: u8,
    decoder: WmaDecoder,
    assembler: FrameAssembler,
    last_pts_ms: u32,
    flushed_eof: bool,
}

impl<R: Read + Seek> AsfWmaDecoder<R> {
    /// Open an ASF/WMV stream and initialize the WMA decoder.
    ///
    /// The decoder selects the first audio stream with format tag 0x0160 (WMAv1)
    /// or 0x0161 (WMAv2).
    pub fn open(mut reader: R) -> Result<Self> {
        let asf = AsfFile::open(&mut reader)?;
        let mut chosen = None;
        for a in asf.audio_streams.iter() {
            if matches!(a.format_tag, 0x0160 | 0x0161) {
                chosen = Some(a.clone());
                break;
            }
        }
        let Some(audio_info) = chosen else {
            return Err(DecoderError::Unsupported(
                "No supported WMA (0x0160/0x0161) audio stream found".into(),
            ));
        };

        reader.seek(SeekFrom::Start(asf.data_offset))?;
        let decoder = WmaDecoder::new(&audio_info)?;

        Ok(Self {
            reader,
            asf,
            audio_stream_number: audio_info.stream_number,
            decoder,
            assembler: FrameAssembler::default(),
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

    /// Decode the next audio frame.
    ///
    /// Returns `Ok(None)` on end-of-stream.
    pub fn next_frame(&mut self) -> Result<Option<DecodedAudioFrame>> {
        loop {
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

            for payload in payloads {
                if payload.stream_number != self.audio_stream_number {
                    continue;
                }
                let Some((pts_ms, _is_key, data)) = self.assembler.push(payload) else {
                    continue;
                };
                self.last_pts_ms = pts_ms;
                if let Some(frame) = self.decoder.decode_packet(&data, pts_ms)? {
                    return Ok(Some(DecodedAudioFrame { pts_ms, frame }));
                }
            }
        }
    }
}

impl<R: Read + Seek> AsfWmv2Decoder<R> {
    /// Open an ASF/WMV stream and initialize the WMV2 decoder.
    ///
    /// The decoder selects the first video stream whose FourCC is WMV2 or WMV1.
    pub fn open(mut reader: R) -> Result<Self> {
        let asf = AsfFile::open(&mut reader)?;
        let mut video_info: Option<VideoStreamInfo> = None;
        for v in asf.video_streams.iter() {
            let four_cc = std::str::from_utf8(&v.codec_four_cc)
                .unwrap_or("")
                .to_uppercase();
            if matches!(four_cc.as_str(), "WMV2" | "WMV1") {
                video_info = Some(v.clone());
                break;
            }
        }
        let Some(video_info) = video_info else {
            return Err(DecoderError::Unsupported("No WMV2/WMV1 video stream found".into()));
        };

        reader.seek(SeekFrom::Start(asf.data_offset))?;

        let decoder = Wmv2Decoder::new(video_info.width, video_info.height, &video_info.extra_data);

        Ok(Self {
            reader,
            asf,
            video_info,
            assembler: FrameAssembler::default(),
            decoder,
        })
    }

    /// Return the selected video stream info.
    pub fn video_stream_info(&self) -> &VideoStreamInfo {
        &self.video_info
    }

    /// Decode the next video frame.
    ///
    /// Returns `Ok(None)` on end-of-stream.
    pub fn next_frame(&mut self) -> Result<Option<DecodedFrame>> {
        loop {
            let payloads = match self.asf.read_packet(&mut self.reader) {
                Ok(p) => p,
                Err(DecoderError::EndOfStream) => return Ok(None),
                Err(e) => return Err(e),
            };

            for payload in payloads {
                if payload.stream_number != self.video_info.stream_number {
                    continue;
                }
                let Some((pts_ms, is_key, data)) = self.assembler.push(payload) else {
                    continue;
                };

                if let Some(frame) = self.decoder.decode_frame_owned(&data, is_key)? {
                    return Ok(Some(DecodedFrame {
                        pts_ms,
                        is_key_frame: is_key,
                        frame,
                    }));
                }
            }
        }
    }
}

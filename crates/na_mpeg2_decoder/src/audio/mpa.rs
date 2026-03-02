use crate::error::{AvError, Result};

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_MP1, CODEC_TYPE_MP2, CODEC_TYPE_MP3};
use symphonia::core::errors::Error as SymphError;
use symphonia::core::formats::Packet;

#[derive(Clone)]
pub struct MpaAudioChunk {
    pub pts_ms: i64,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[derive(Default)]
pub struct MpaAudioDecoder {
    buf: Vec<u8>,

    dec: Option<Box<dyn Decoder>>,
    sample_buf: Option<SampleBuffer<f32>>,

    // Best-effort PTS tracking.
    next_pts_ms: Option<i64>,

    // Symphonia packet track id (arbitrary but must be consistent).
    track_id: u32,
}

impl MpaAudioDecoder {
    pub fn new() -> Self {
        Self { buf: Vec::new(), dec: None, sample_buf: None, next_pts_ms: None, track_id: 0 }
    }

    pub fn push_with<F>(&mut self, data: &[u8], pts_ms: Option<i64>, mut on_chunk: F) -> Result<()>
    where
        F: FnMut(MpaAudioChunk),
    {
        if let Some(pts) = pts_ms {
            self.next_pts_ms = Some(pts);
        }

        self.buf.extend_from_slice(data);

        let mut pos = 0usize;
        while pos + 4 <= self.buf.len() {
            let Some(h) = MpaHeader::parse(&self.buf[pos..]) else {
                pos += 1;
                continue;
            };

            if pos + h.frame_len > self.buf.len() {
                break;
            }

            // Avoid borrowing self.buf while calling into self (decoder state).
            let pkt_owned = self.buf[pos..pos + h.frame_len].to_vec();
            pos += h.frame_len;

            let pts0 = self.next_pts_ms.unwrap_or(0);
            self.decode_one_packet(&pkt_owned, pts0, h.codec_type, &mut on_chunk)?;
        }

        if pos > 0 {
            self.buf.drain(0..pos);
        }

        Ok(())
    }

    fn decode_one_packet<F>(&mut self, pkt_bytes: &[u8], pts_ms: i64, codec_type: symphonia::core::codecs::CodecType, on_chunk: &mut F) -> Result<()>
    where
        F: FnMut(MpaAudioChunk),
    {
        if self.dec.is_none() {
            let mut cp = CodecParameters::new();
            cp.for_codec(codec_type);

            let dec = symphonia::default::get_codecs()
                .make(&cp, &DecoderOptions::default())
                .map_err(AvError::from)?;
            self.dec = Some(dec);
        }

        let pkt = Packet::new_from_boxed_slice(self.track_id, 0, 0, pkt_bytes.to_vec().into_boxed_slice());

        let dec = self.dec.as_mut().expect("decoder must be initialized");
        match dec.decode(&pkt) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let duration = decoded.capacity();
                let duration_u64 = duration as u64;

                let sb = match self.sample_buf.as_mut() {
                    None => {
                        self.sample_buf = Some(SampleBuffer::<f32>::new(duration_u64, spec));
                        self.sample_buf.as_mut().unwrap()
                    }
                    Some(sb) => {
                        if sb.capacity() < duration {
                            *sb = SampleBuffer::<f32>::new(duration_u64, spec);
                        }
                        sb
                    }
                };

                sb.copy_interleaved_ref(decoded.clone());

                let channels = spec.channels.count() as u16;
                let samples = sb.samples().to_vec();

                let sample_rate = spec.rate;
                on_chunk(MpaAudioChunk { pts_ms, sample_rate, channels, samples });

                // Advance PTS based on decoded frames.
                let frames = decoded.frames() as i64;
                if frames > 0 && sample_rate > 0 {
                    let dur_ms = (frames * 1000) / (sample_rate as i64);
                    self.next_pts_ms = Some(pts_ms + dur_ms);
                }
            }
            Err(SymphError::DecodeError(_)) => {
                // Best-effort: ignore bad frames.
            }
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
struct MpaHeader {
    frame_len: usize,
    codec_type: symphonia::core::codecs::CodecType,
}

impl MpaHeader {
    fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 4 {
            return None;
        }
        let b0 = buf[0];
        let b1 = buf[1];
        let b2 = buf[2];

        // Sync.
        if b0 != 0xFF || (b1 & 0xE0) != 0xE0 {
            return None;
        }

        let version_id = (b1 >> 3) & 0x03;
        let layer_id = (b1 >> 1) & 0x03;
        if version_id == 0x01 || layer_id == 0x00 {
            return None;
        }

        let bitrate_idx = (b2 >> 4) & 0x0F;
        let sr_idx = (b2 >> 2) & 0x03;
        if bitrate_idx == 0 || bitrate_idx == 0x0F || sr_idx == 0x03 {
            return None;
        }

        let padding: u32 = ((b2 >> 1) & 0x01) as u32;

        let (sr, is_v1) = match version_id {
            0x03 => (SAMPLE_RATES_V1[sr_idx as usize], true),
            0x02 => (SAMPLE_RATES_V2[sr_idx as usize], false),
            0x00 => (SAMPLE_RATES_V25[sr_idx as usize], false),
            _ => return None,
        };

        let (codec_type, bitrate_kbps, frame_len) = match layer_id {
            0x03 => {
                // Layer I
                let br = if is_v1 {
                    BITRATES_V1_L1[bitrate_idx as usize]
                } else {
                    BITRATES_V2_L1[bitrate_idx as usize]
                };
                let fl = (((12u64 * (br as u64) * 1000u64) / (sr as u64)) + (padding as u64)) * 4u64;
                (CODEC_TYPE_MP1, br, fl as usize)
            }
            0x02 => {
                // Layer II
                let br = if is_v1 {
                    BITRATES_V1_L2[bitrate_idx as usize]
                } else {
                    BITRATES_V2_L2L3[bitrate_idx as usize]
                };
                let fl = ((144u64 * (br as u64) * 1000u64) / (sr as u64)) + (padding as u64);
                (CODEC_TYPE_MP2, br, fl as usize)
            }
            0x01 => {
                // Layer III
                let br = if is_v1 {
                    BITRATES_V1_L3[bitrate_idx as usize]
                } else {
                    BITRATES_V2_L2L3[bitrate_idx as usize]
                };
                let coeff: u64 = if is_v1 { 144 } else { 72 };
                let fl = ((coeff * (br as u64) * 1000u64) / (sr as u64)) + (padding as u64);
                (CODEC_TYPE_MP3, br, fl as usize)
            }
            _ => return None,
        };

        if bitrate_kbps == 0 || frame_len < 4 {
            return None;
        }

        Some(Self { frame_len, codec_type })
    }
}

const SAMPLE_RATES_V1: [u32; 3] = [44100, 48000, 32000];
const SAMPLE_RATES_V2: [u32; 3] = [22050, 24000, 16000];
const SAMPLE_RATES_V25: [u32; 3] = [11025, 12000, 8000];

const BITRATES_V1_L1: [u32; 16] = [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0];
const BITRATES_V1_L2: [u32; 16] = [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0];
const BITRATES_V1_L3: [u32; 16] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];

const BITRATES_V2_L1: [u32; 16] = [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0];
const BITRATES_V2_L2L3: [u32; 16] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0];

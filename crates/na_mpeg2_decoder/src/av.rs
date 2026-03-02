use std::collections::VecDeque;
use std::sync::Arc;

use crate::audio::{MpaAudioChunk, MpaAudioDecoder};
use crate::convert::frame_to_rgba_bt601_limited;
use crate::demux::{Demuxer, Packet, StreamType};
use crate::error::Result;
use crate::video::{Decoder as VideoDecoder, Frame};

#[derive(Clone)]
pub struct MpegRgbaFrame {
    pub pts_ms: i64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone)]
pub struct MpegAudioF32 {
    pub pts_ms: i64,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[derive(Clone)]
pub enum MpegAvEvent {
    Video(MpegRgbaFrame),
    Audio(MpegAudioF32),
}

#[derive(Default)]
pub struct MpegAvPipeline {
    demux: Demuxer,
    vdec: VideoDecoder,
    adec: MpaAudioDecoder,

    pkts: Vec<Packet>,
    pub stash: VecDeque<MpegAvEvent>,
}

impl MpegAvPipeline {
    pub fn new() -> Self {
        Self { demux: Demuxer::new_auto(), vdec: VideoDecoder::new(), adec: MpaAudioDecoder::new(), pkts: Vec::new(), stash: VecDeque::new() }
    }

    #[inline]
    pub fn demuxer_mut(&mut self) -> &mut Demuxer {
        &mut self.demux
    }

    #[inline]
    pub fn video_decoder_mut(&mut self) -> &mut VideoDecoder {
        &mut self.vdec
    }

    #[inline]
    pub fn audio_decoder_mut(&mut self) -> &mut MpaAudioDecoder {
        &mut self.adec
    }

    pub fn push_with<F>(&mut self, data: &[u8], pts_90k: Option<i64>, mut on_event: F) -> Result<()>
    where
        F: FnMut(MpegAvEvent),
    {
        self.pkts.clear();
        self.demux.push_into(data, pts_90k, &mut self.pkts);

        // Move packets out to avoid borrowing self.pkts while calling &mut self handlers.
        let mut local_pkts: Vec<Packet> = Vec::new();
        std::mem::swap(&mut self.pkts, &mut local_pkts);

        for pkt in local_pkts.drain(..) {
            match pkt.stream_type {
                StreamType::MpegVideo => self.handle_video_pkt(&pkt, &mut on_event)?,
                StreamType::MpegAudio => self.handle_audio_pkt(&pkt, &mut on_event)?,
                StreamType::Unknown => {}
            }
        }

        std::mem::swap(&mut self.pkts, &mut local_pkts);
        self.pkts.clear();

        Ok(())
    }

    pub fn push(&mut self, data: &[u8], pts_90k: Option<i64>) -> Result<()> {
        let mut tmp: Vec<MpegAvEvent> = Vec::new();
        self.push_with(data, pts_90k, |ev| tmp.push(ev))?;
        for ev in tmp {
            self.stash.push_back(ev);
        }
        Ok(())
    }

    pub fn flush_with<F>(&mut self, mut on_event: F) -> Result<()>
    where
        F: FnMut(MpegAvEvent),
    {
        // Video: flush delayed frames.
        for f in self.vdec.flush_shared()? {
            self.emit_video_frame(f, &mut on_event)?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        let mut tmp: Vec<MpegAvEvent> = Vec::new();
        self.flush_with(|ev| tmp.push(ev))?;
        for ev in tmp {
            self.stash.push_back(ev);
        }
        Ok(())
    }

    fn handle_video_pkt<F>(&mut self, pkt: &Packet, on_event: &mut F) -> Result<()>
    where
        F: FnMut(MpegAvEvent),
    {
        let decoded: Vec<Arc<Frame>> = self.vdec.decode_shared(&pkt.data, pkt.pts_90k)?;
        for f in decoded {
            self.emit_video_frame(f, on_event)?;
        }
        Ok(())
    }

    fn emit_video_frame<F>(&mut self, f: Arc<Frame>, on_event: &mut F) -> Result<()>
    where
        F: FnMut(MpegAvEvent),
    {
        let w = f.width as u32;
        let h = f.height as u32;
        let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
        frame_to_rgba_bt601_limited(&f, &mut rgba);

        let pts_ms = pts90k_opt_to_ms(f.pts_90k);
        on_event(MpegAvEvent::Video(MpegRgbaFrame { pts_ms, width: w, height: h, rgba }));
        Ok(())
    }

    fn handle_audio_pkt<F>(&mut self, pkt: &Packet, on_event: &mut F) -> Result<()>
    where
        F: FnMut(MpegAvEvent),
    {
        let pts_ms_opt = pkt.pts_90k.map(pts90k_to_ms);
        self.adec.push_with(&pkt.data, pts_ms_opt, |ch: MpaAudioChunk| {
            on_event(MpegAvEvent::Audio(MpegAudioF32 {
                pts_ms: ch.pts_ms,
                sample_rate: ch.sample_rate,
                channels: ch.channels,
                samples: ch.samples,
            }))
        })?;
        Ok(())
    }
}

#[inline]
fn pts90k_to_ms(v: i64) -> i64 {
    (v * 1000) / 90000
}

#[inline]
fn pts90k_opt_to_ms(v: Option<i64>) -> i64 {
    v.map(pts90k_to_ms).unwrap_or(0)
}

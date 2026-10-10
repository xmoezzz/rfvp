use crate::platform_time::Instant;
use std::collections::VecDeque;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::{anyhow, Context, Result};

use crate::host_api::{AudioParams, AudioStreamId};
use crate::rfvp_audio::{AudioManager, SoundSource, Tween};
use crate::subsystem::resources::vfs::Vfs;

use na_mpeg2_decoder::{MpegAvEvent, MpegAvPipeline};

#[cfg(feature = "mp4")]
use symphonia::core::audio::{SampleBuffer, SignalSpec};
#[cfg(feature = "mp4")]
use symphonia::core::codecs::DecoderOptions;
#[cfg(feature = "mp4")]
use symphonia::core::errors::Error as SymphoniaError;
#[cfg(feature = "mp4")]
use symphonia::core::formats::FormatOptions;
#[cfg(feature = "mp4")]
use symphonia::core::io::MediaSourceStream;
#[cfg(feature = "mp4")]
use symphonia::core::meta::MetadataOptions;
#[cfg(feature = "mp4")]
use symphonia::core::probe::Hint;
#[cfg(feature = "mp4")]
use symphonia::default::{get_codecs, get_probe};

use super::{MovieFrame, MovieMode, MovieRequest, MovieSource};
use crate::utils::file::app_base_path;

/// Cap buffered decoded frames on the main thread.
/// Each frame is screen-sized RGBA and can be huge; keeping this bounded prevents memory spikes.
const MAX_MOVIE_STASH_FRAMES: usize = 4;

#[cfg(feature = "mp4")]
#[derive(Debug)]
struct Mp4Playback {
    stream: video_sys::VideoStream,
    stash: VecDeque<video_sys::VideoFrame>,

    started_at: Option<Instant>,
    base_pts_us: Option<i64>,
    last_presented_pts: i64,

    mp4_path: PathBuf,

    // Audio (decoded to a WAV container and played via Kira).
    audio_manager: Option<Arc<AudioManager>>,
    audio_data: Option<Arc<[u8]>>,
    audio_playing: bool,
    audio_started: bool,
}

#[cfg(feature = "mp4")]
impl Mp4Playback {
    fn open_from_mp4_bytes(
        mp4_path: impl AsRef<Path>,
        mode: MovieMode,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<Self> {
        let stream = video_sys::VideoStream::open(&mp4_path).context("VideoStream::open")?;

        let (audio_data, audio_manager) = match mode {
            MovieMode::ModalWithAudio => {
                let am = audio_manager;
                let data = match am.as_ref() {
                    None => {
                        log::warn!("Movie audio decode skipped: no AudioManager");
                        None
                    }
                    Some(_) => match decode_mp4_audio_to_wav_bytes(&mp4_path) {
                        Ok(Some(wav_bytes)) => {
                            Some(Arc::from(wav_bytes))
                        }
                        Ok(None) => {
                            log::warn!(
                                "Movie audio: no decodable audio track: {}",
                                mp4_path.as_ref().display()
                            );
                            None
                        }
                        Err(e) => {
                            // Audio is best-effort; keep video playing even if audio fails.
                            log::warn!("Movie audio decode failed: {e:?}");
                            None
                        }
                    },
                };
                (data, am)
            }
            MovieMode::LayerNoAudio => (None, None),
        };

        Ok(Self {
            stream,
            stash: VecDeque::new(),
            started_at: None,
            base_pts_us: None,
            last_presented_pts: i64::MIN,
            mp4_path: mp4_path.as_ref().to_path_buf(),
            audio_manager,
            audio_data,
            audio_playing: false,
            audio_started: false,
        })
    }

    fn maybe_start_audio(&mut self) {
        if self.audio_started {
            return;
        }
        self.audio_started = true;

        let Some(am) = self.audio_manager.as_ref() else {
            return;
        };
        let Some(data) = self.audio_data.take() else {
            return;
        };

        self.audio_playing = start_movie_audio(am, data);
    }

    fn next_due_frame(&mut self) -> Option<video_sys::VideoFrame> {
        // 1) Drain decoded frames from the decoder thread.
        while self.stash.len() < MAX_MOVIE_STASH_FRAMES {
            match self.stream.try_recv_one() {
                Some(f) => self.stash.push_back(f),
                None => break,
            }
        }

        // 2) Initialize timing on the first frame.
        if self.started_at.is_none() {
            if let Some(front) = self.stash.front() {
                self.started_at = Some(Instant::now());
                self.base_pts_us = Some(front.pts_us);
                // Align audio start with video start.
                self.maybe_start_audio();
            } else {
                return None;
            }
        }

        let started_at = self.started_at.unwrap();
        let elapsed_us = started_at.elapsed().as_micros() as i64;
        let base = self.base_pts_us.unwrap_or(0);
        let target_pts_us = base.saturating_add(elapsed_us);

        // 3) Pop all frames that are due; keep the latest.
        let mut latest_due = None;
        while let Some(front) = self.stash.front() {
            if front.pts_us <= target_pts_us {
                latest_due = self.stash.pop_front();
            } else {
                break;
            }
        }

        latest_due
    }

    fn is_finished(&self) -> bool {
        self.stream.is_finished() && self.stash.is_empty()
    }

    fn stop_and_cleanup(mut self) {
        // Ensure decode thread exits promptly.
        self.stream.stop();

        // Stop audio promptly.
        if std::mem::take(&mut self.audio_playing) {
            if let Some(am) = self.audio_manager.as_ref() {
                stop_movie_audio(am);
            }
        }
    }
}

#[derive(Clone)]
struct WmvRgbaFrame {
    pts_ms: u32,
    rgba: Vec<u8>,
}

impl std::fmt::Debug for WmvRgbaFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WmvRgbaFrame")
            .field("pts_ms", &self.pts_ms)
            .field("rgba_len", &self.rgba.len())
            .finish()
    }
}

#[derive(Debug)]
struct WmvPlayback {
    rx: crossbeam_channel::Receiver<WmvRgbaFrame>,
    stash: VecDeque<WmvRgbaFrame>,

    started_at: Option<Instant>,
    base_pts_ms: Option<u32>,
    last_presented_pts_ms: u32,

    stop_flag: Arc<AtomicBool>,

    // We always upload scaled frames sized to the screen.
    screen_w: u32,
    screen_h: u32,

    // Audio (decoded to a WAV container and played via Kira).
    audio_manager: Option<Arc<AudioManager>>,
    audio_data: Option<Arc<[u8]>>,
    audio_playing: bool,
    audio_started: bool,
}

impl WmvPlayback {
    fn open_from_wmv_path(
        wmv_path: impl AsRef<Path>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<Self> {
        let wmv_path = wmv_path.as_ref().to_path_buf();

        let (audio_data, audio_manager) = match mode {
            MovieMode::ModalWithAudio => {
                let am = audio_manager;
                let data = match am.as_ref() {
                    None => {
                        log::warn!("WMV movie audio decode skipped: no AudioManager");
                        None
                    }
                    Some(_) => match decode_wmv_audio_to_wav_bytes(&wmv_path) {
                        Ok(Some(wav_bytes)) => {
                            Some(Arc::from(wav_bytes))
                        }
                        Ok(None) => {
                            log::warn!(
                                "WMV movie audio: no decodable audio track: {}",
                                wmv_path.display()
                            );
                            None
                        }
                        Err(e) => {
                            // Audio is best-effort; keep video playing even if audio fails.
                            log::warn!("WMV movie audio decode failed: {e:?}");
                            None
                        }
                    },
                };
                (data, am)
            }
            MovieMode::LayerNoAudio => (None, None),
        };

        let stop_flag = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::bounded::<WmvRgbaFrame>(2);

        // Video decode thread.
        {
            let stop = stop_flag.clone();
            let path = wmv_path.clone();
            std::thread::spawn(move || {
                let f = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(e) => {
                        log::warn!("WMV decode: open failed: {e:?}");
                        return;
                    }
                };

                let mut dec = match wmv_decoder::AsfWmv2Decoder::open(std::io::BufReader::new(f)) {
                    Ok(d) => d,
                    Err(e) => {
                        log::warn!("WMV decode: AsfWmv2Decoder::open failed: {e:?}");
                        return;
                    }
                };

                while !stop.load(Ordering::Relaxed) {
                    let fr = match dec.next_frame() {
                        Ok(Some(fr)) => fr,
                        Ok(None) => break,
                        Err(e) => {
                            log::warn!("WMV decode: next_frame failed: {e:?}");
                            break;
                        }
                    };

                    if stop.load(Ordering::Relaxed) {
                        break;
                    }

                    // Convert+scale into a screen-sized RGBA buffer.
                    let mut rgba = Vec::new();
                    yuv420_to_rgba_scaled(&fr.frame, screen_w, screen_h, &mut rgba);

                    if tx
                        .send(WmvRgbaFrame {
                            pts_ms: fr.pts_ms,
                            rgba,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }

        Ok(Self {
            rx,
            stash: VecDeque::new(),
            started_at: None,
            base_pts_ms: None,
            last_presented_pts_ms: u32::MAX,
            stop_flag,
            screen_w,
            screen_h,
            audio_manager,
            audio_data,
            audio_playing: false,
            audio_started: false,
        })
    }

    fn maybe_start_audio(&mut self) {
        if self.audio_started {
            return;
        }
        self.audio_started = true;

        let Some(am) = self.audio_manager.as_ref() else {
            return;
        };
        let Some(data) = self.audio_data.take() else {
            return;
        };

        self.audio_playing = start_movie_audio(am, data);
    }

    fn next_due_frame(&mut self) -> Option<WmvRgbaFrame> {
        // 1) Drain decoded frames from the decoder thread.
        while self.stash.len() < MAX_MOVIE_STASH_FRAMES {
            match self.rx.try_recv() {
                Ok(f) => self.stash.push_back(f),
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => break,
            }
        }

        // 2) Initialize timing on the first frame.
        if self.started_at.is_none() {
            if let Some(front) = self.stash.front() {
                self.started_at = Some(Instant::now());
                self.base_pts_ms = Some(front.pts_ms);
                self.maybe_start_audio();
            } else {
                return None;
            }
        }

        let started_at = self.started_at.unwrap();
        let elapsed_ms = started_at.elapsed().as_millis() as u32;
        let base = self.base_pts_ms.unwrap_or(0);
        let target_pts_ms = base.saturating_add(elapsed_ms);

        // 3) Pop all frames that are due; keep the latest.
        let mut latest_due = None;
        while let Some(front) = self.stash.front() {
            if front.pts_ms <= target_pts_ms {
                latest_due = self.stash.pop_front();
            } else {
                break;
            }
        }

        latest_due
    }

    fn is_finished(&mut self) -> bool {
        // We only report EOF when:
        //   1) the decode thread has dropped the sender (channel disconnected), and
        //   2) we have no pending frames buffered locally.
        //
        // NOTE: `crossbeam_channel::Receiver` doesn't expose `is_disconnected()` in some
        // versions; detect it via `try_recv()`.
        if !self.stash.is_empty() {
            return false;
        }

        match self.rx.try_recv() {
            Ok(f) => {
                // A frame arrived between the previous drain and this check.
                self.stash.push_back(f);
                false
            }
            Err(crossbeam_channel::TryRecvError::Empty) => false,
            Err(crossbeam_channel::TryRecvError::Disconnected) => true,
        }
    }

    fn stop_and_cleanup(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);

        if std::mem::take(&mut self.audio_playing) {
            if let Some(am) = self.audio_manager.as_ref() {
                stop_movie_audio(am);
            }
        }
    }
}

#[derive(Clone)]
struct MpegRgbaFrame {
    pts_ms: i64,
    rgba: Vec<u8>,
}

impl std::fmt::Debug for MpegRgbaFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MpegRgbaFrame")
            .field("pts_ms", &self.pts_ms)
            .field("rgba_len", &self.rgba.len())
            .finish()
    }
}

#[derive(Debug)]
struct MpegPlayback {
    rx: crossbeam_channel::Receiver<MpegRgbaFrame>,
    stash: VecDeque<MpegRgbaFrame>,

    started_at: Option<Instant>,
    base_pts_ms: Option<i64>,
    last_presented_pts_ms: i64,

    stop_flag: Arc<AtomicBool>,

    // We always upload scaled frames sized to the screen.
    screen_w: u32,
    screen_h: u32,

    // Audio (decoded to a WAV container and played via Kira).
    audio_manager: Option<Arc<AudioManager>>,
    audio_data: Option<Arc<[u8]>>,
    audio_playing: bool,
    audio_started: bool,
}

impl MpegPlayback {
    fn open_from_mpeg_path(
        mpeg_path: impl AsRef<Path>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<Self> {
        let mpeg_path = mpeg_path.as_ref().to_path_buf();

        let (audio_data, audio_manager) = match mode {
            MovieMode::ModalWithAudio => {
                let am = audio_manager;
                let data = match am.as_ref() {
                    None => {
                        log::warn!("MPEG movie audio decode skipped: no AudioManager");
                        None
                    }
                    Some(_) => match decode_mpeg_audio_to_wav_bytes(&mpeg_path) {
                        Ok(Some(wav_bytes)) => {
                            Some(Arc::from(wav_bytes))
                        }
                        Ok(None) => {
                            log::warn!(
                                "MPEG movie audio: no decodable audio track: {}",
                                mpeg_path.display()
                            );
                            None
                        }
                        Err(e) => {
                            // Audio is best-effort; keep video playing even if audio fails.
                            log::warn!("MPEG movie audio decode failed: {e:?}");
                            None
                        }
                    },
                };
                (data, am)
            }
            MovieMode::LayerNoAudio => (None, None),
        };

        let stop_flag = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::bounded::<MpegRgbaFrame>(2);

        // Video decode thread.
        {
            let stop = stop_flag.clone();
            let path = mpeg_path.clone();
            std::thread::spawn(move || {
                let mut f = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(e) => {
                        log::warn!("MPEG decode: open failed: {e:?}");
                        return;
                    }
                };

                let mut pipe = MpegAvPipeline::new();
                let mut buf = vec![0u8; 64 * 1024];

                while !stop.load(Ordering::Relaxed) {
                    let n = match f.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(e) => {
                            log::warn!("MPEG decode: read failed: {e:?}");
                            break;
                        }
                    };

                    if stop.load(Ordering::Relaxed) {
                        break;
                    }

                    let r = pipe.push_with(&buf[..n], None, |ev| {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }

                        if let MpegAvEvent::Video(v) = ev {
                            // Scale to screen in the decode thread (do not touch prim factor_x/factor_y).
                            let rgba = if v.width == screen_w && v.height == screen_h {
                                v.rgba
                            } else {
                                let mut out = Vec::new();
                                rgba_scale_nearest(
                                    &v.rgba, v.width, v.height, screen_w, screen_h, &mut out,
                                );
                                out
                            };

                            let _ = tx.send(MpegRgbaFrame {
                                pts_ms: v.pts_ms,
                                rgba,
                            });
                        }
                    });

                    if let Err(e) = r {
                        log::warn!("MPEG decode: push failed: {e:?}");
                        break;
                    }
                }

                // Flush delayed video frames.
                let _ = pipe.flush_with(|ev| {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    if let MpegAvEvent::Video(v) = ev {
                        let rgba = if v.width == screen_w && v.height == screen_h {
                            v.rgba
                        } else {
                            let mut out = Vec::new();
                            rgba_scale_nearest(
                                &v.rgba, v.width, v.height, screen_w, screen_h, &mut out,
                            );
                            out
                        };
                        let _ = tx.send(MpegRgbaFrame {
                            pts_ms: v.pts_ms,
                            rgba,
                        });
                    }
                });
            });
        }

        Ok(Self {
            rx,
            stash: VecDeque::new(),
            started_at: None,
            base_pts_ms: None,
            last_presented_pts_ms: i64::MIN,
            stop_flag,
            screen_w,
            screen_h,
            audio_manager,
            audio_data,
            audio_playing: false,
            audio_started: false,
        })
    }

    fn maybe_start_audio(&mut self) {
        if self.audio_started {
            return;
        }
        self.audio_started = true;

        let Some(am) = self.audio_manager.as_ref() else {
            return;
        };
        let Some(data) = self.audio_data.take() else {
            return;
        };

        self.audio_playing = start_movie_audio(am, data);
    }

    fn next_due_frame(&mut self) -> Option<MpegRgbaFrame> {
        // 1) Drain decoded frames from the decoder thread.
        while self.stash.len() < MAX_MOVIE_STASH_FRAMES {
            match self.rx.try_recv() {
                Ok(f) => self.stash.push_back(f),
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => break,
            }
        }

        // 2) Initialize timing on the first frame.
        if self.started_at.is_none() {
            if let Some(front) = self.stash.front() {
                self.started_at = Some(Instant::now());
                self.base_pts_ms = Some(front.pts_ms);
                self.maybe_start_audio();
            } else {
                return None;
            }
        }

        let started_at = self.started_at.unwrap();
        let elapsed_ms = started_at.elapsed().as_millis() as i64;
        let base = self.base_pts_ms.unwrap_or(0);
        let target_pts_ms = base.saturating_add(elapsed_ms);

        // 3) Pop all frames that are due; keep the latest.
        let mut latest_due = None;
        while let Some(front) = self.stash.front() {
            if front.pts_ms <= target_pts_ms {
                latest_due = self.stash.pop_front();
            } else {
                break;
            }
        }

        latest_due
    }

    fn is_finished(&mut self) -> bool {
        if !self.stash.is_empty() {
            return false;
        }

        match self.rx.try_recv() {
            Ok(f) => {
                self.stash.push_back(f);
                false
            }
            Err(crossbeam_channel::TryRecvError::Empty) => false,
            Err(crossbeam_channel::TryRecvError::Disconnected) => true,
        }
    }

    fn stop_and_cleanup(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);

        if std::mem::take(&mut self.audio_playing) {
            if let Some(am) = self.audio_manager.as_ref() {
                stop_movie_audio(am);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WMV video conversion (YUV420p -> RGBA) with scaling to screen size
// ─────────────────────────────────────────────────────────────────────────────

#[inline]
fn clamp_u8(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

fn yuv420_to_rgba_scaled(src: &wmv_decoder::YuvFrame, dst_w: u32, dst_h: u32, out: &mut Vec<u8>) {
    let sw = src.width.max(1);
    let sh = src.height.max(1);

    let dw = dst_w.max(1);
    let dh = dst_h.max(1);

    let out_len = (dw as usize).saturating_mul(dh as usize).saturating_mul(4);
    out.clear();
    out.resize(out_len, 0);

    let uv_w = (sw / 2).max(1);
    // NOTE: uv_h is implicit via indexing.

    for dy in 0..dh {
        let sy = (dy as u64 * sh as u64 / dh as u64) as u32;
        let sy = sy.min(sh - 1);
        let suv = (sy / 2).min((sh / 2).saturating_sub(1));

        for dx in 0..dw {
            let sx = (dx as u64 * sw as u64 / dw as u64) as u32;
            let sx = sx.min(sw - 1);
            let sux = (sx / 2).min(uv_w - 1);

            let y = src.y[(sy * sw + sx) as usize] as i32;
            let u = src.cb[(suv * uv_w + sux) as usize] as i32;
            let v = src.cr[(suv * uv_w + sux) as usize] as i32;

            // BT.601 full-range-ish integer conversion (matches typical WMV2 output expectations).
            let c = y - 16;
            let d = u - 128;
            let e = v - 128;

            let r = (298 * c + 409 * e + 128) >> 8;
            let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
            let b = (298 * c + 516 * d + 128) >> 8;

            let off = ((dy * dw + dx) as usize) * 4;
            out[off + 0] = clamp_u8(r);
            out[off + 1] = clamp_u8(g);
            out[off + 2] = clamp_u8(b);
            out[off + 3] = 255;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MPEG video scaling helper (RGBA -> RGBA)
// ─────────────────────────────────────────────────────────────────────────────

fn rgba_scale_nearest(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    out: &mut Vec<u8>,
) {
    let sw = src_w.max(1);
    let sh = src_h.max(1);

    let dw = dst_w.max(1);
    let dh = dst_h.max(1);

    let out_len = (dw as usize).saturating_mul(dh as usize).saturating_mul(4);
    out.clear();
    out.resize(out_len, 0);

    for dy in 0..dh {
        let sy = (dy as u64 * sh as u64 / dh as u64) as u32;
        let sy = sy.min(sh - 1);
        for dx in 0..dw {
            let sx = (dx as u64 * sw as u64 / dw as u64) as u32;
            let sx = sx.min(sw - 1);

            let so = ((sy * sw + sx) as usize) * 4;
            let doff = ((dy * dw + dx) as usize) * 4;
            out[doff..doff + 4].copy_from_slice(&src[so..so + 4]);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MPEG audio (MP1/MP2/MP3) best-effort decode to WAV
// ─────────────────────────────────────────────────────────────────────────────

/// Best-effort: decode MPEG audio into WAV(PCM16LE).
///
/// Returns `Ok(None)` if no audio is present.
fn decode_mpeg_audio_to_wav_bytes(mpeg_path: impl AsRef<Path>) -> Result<Option<Vec<u8>>> {
    let mut f = match std::fs::File::open(mpeg_path.as_ref()) {
        Ok(f) => f,
        Err(e) => {
            log::warn!(
                "mpeg audio: open failed: {}: {e:?}",
                mpeg_path.as_ref().display()
            );
            return Ok(None);
        }
    };

    let mut pipe = MpegAvPipeline::new();
    let mut buf = vec![0u8; 64 * 1024];

    let mut pcm: Vec<i16> = Vec::new();
    let mut sr: u32 = 0;
    let mut ch: u16 = 0;

    loop {
        let n = match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                log::warn!("mpeg audio: read failed: {e:?}");
                break;
            }
        };

        pipe.push_with(&buf[..n], None, |ev| {
            if let MpegAvEvent::Audio(a) = ev {
                if sr == 0 {
                    sr = a.sample_rate;
                    ch = a.channels;
                }
                if a.sample_rate != sr || a.channels != ch || ch == 0 {
                    return;
                }
                for s in a.samples {
                    let s = s.clamp(-1.0, 1.0);
                    let v = (s * 32767.0).round() as i32;
                    pcm.push(v.clamp(-32768, 32767) as i16);
                }
            }
        })?;
    }

    if pcm.is_empty() || sr == 0 || ch == 0 {
        return Ok(None);
    }

    Ok(Some(build_wav_pcm16le(&pcm, ch, sr)))
}

// ─────────────────────────────────────────────────────────────────────────────
// MP4 audio (AAC) best-effort decode to WAV
// ─────────────────────────────────────────────────────────────────────────────

/// Best-effort: extract the first AAC audio track in the MP4 and decode it to WAV(PCM16LE).
///
/// Returns `Ok(None)` if no decodable audio track is present.
#[cfg(feature = "mp4")]
fn decode_mp4_audio_to_wav_bytes(mp4_path: impl AsRef<Path>) -> Result<Option<Vec<u8>>> {
    // Hint Symphonia that this is MP4.
    let mut hint = Hint::new();
    hint.with_extension("mp4");

    let cursor = std::fs::File::open(mp4_path)?;
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let probed = get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("symphonia probe mp4")?;

    let mut format = probed.format;

    // Prefer an audio track. In MP4 files, the "default" track is often video.
    let track = match format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some() && t.codec_params.channels.is_some())
    {
        Some(t) => t,
        None => return Ok(None),
    };

    let sr = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("mp4 audio: missing sample_rate"))?;
    let ch = track
        .codec_params
        .channels
        .ok_or_else(|| anyhow!("mp4 audio: missing channels"))?
        .count();
    if ch == 0 {
        return Ok(None);
    }

    let mut decoder = get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("symphonia make decoder")?;

    let track_id = track.id;

    // Decode into interleaved PCM16.
    let spec = SignalSpec::new(sr, track.codec_params.channels.unwrap());
    let mut pcm: Vec<i16> = Vec::new();
    let mut sample_buf = SampleBuffer::<i16>::new(0, spec);

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(_)) => break, // EOF
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(anyhow!("symphonia next_packet: {e:?}")),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                sample_buf.copy_interleaved_ref(decoded);
                pcm.extend_from_slice(sample_buf.samples());
            }
            Err(SymphoniaError::IoError(_)) => break,
            Err(SymphoniaError::DecodeError(_)) => {
                // Skip damaged packets.
                continue;
            }
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(anyhow!("symphonia decode: {e:?}")),
        }
    }

    if pcm.is_empty() {
        return Ok(None);
    }

    Ok(Some(build_wav_pcm16le(&pcm, ch as u16, sr)))
}

// ─────────────────────────────────────────────────────────────────────────────
// WMV audio (WMA) best-effort decode to WAV
// ─────────────────────────────────────────────────────────────────────────────

fn decode_wmv_audio_to_wav_bytes(wmv_path: impl AsRef<Path>) -> Result<Option<Vec<u8>>> {
    let f = match std::fs::File::open(wmv_path.as_ref()) {
        Ok(f) => f,
        Err(e) => {
            log::warn!(
                "wmv audio: open failed: {}: {e:?}",
                wmv_path.as_ref().display()
            );
            return Ok(None);
        }
    };

    let mut dec = match wmv_decoder::AsfWmaDecoder::open(std::io::BufReader::new(f)) {
        Ok(d) => d,
        Err(e) => {
            log::warn!(
                "wmv audio: decoder open failed: {}: {e:?}",
                wmv_path.as_ref().display()
            );
            return Ok(None);
        }
    };

    let sr = dec.sample_rate();
    let ch = dec.channels();
    if ch == 0 {
        return Ok(None);
    }

    let mut pcm: Vec<i16> = Vec::new();
    while let Some(fr) = dec.next_frame()? {
        for &s in fr.frame.samples.iter() {
            let s = s.clamp(-1.0, 1.0);
            let v = (s * 32767.0).round() as i32;
            let v = v.clamp(-32768, 32767) as i16;
            pcm.push(v);
        }
    }

    if pcm.is_empty() {
        return Ok(None);
    }

    Ok(Some(build_wav_pcm16le(&pcm, ch, sr)))
}

fn build_wav_pcm16le(samples: &[i16], channels: u16, sample_rate: u32) -> Vec<u8> {
    let bytes_per_sample = 2u16;
    let block_align = channels.saturating_mul(bytes_per_sample);
    let byte_rate = sample_rate.saturating_mul(block_align as u32);
    let data_size = (samples.len() as u32).saturating_mul(2);
    let riff_size = 36u32.saturating_add(data_size);

    let mut out = Vec::with_capacity((44 + data_size) as usize);

    // RIFF header
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");

    // fmt chunk
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // audio_format = PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes()); // bits_per_sample

    // data chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());

    for &s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }

    out
}

/// Play a movie's soundtrack (a WAV decoded from the movie) through the audio backend.
/// Returns whether it started; the movie plays on without it otherwise.
fn start_movie_audio(audio: &AudioManager, wav: Arc<[u8]>) -> bool {
    // The soundtrack is in memory; no game files are read.
    let vfs = Vfs::default();
    let backend = audio.backend();
    let started = backend
        .load(AudioStreamId::MOVIE, &SoundSource::Bytes(wav), &vfs)
        .and_then(|()| {
            backend.play(
                AudioStreamId::MOVIE,
                AudioParams {
                    volume: 1.0,
                    pan: 0.0,
                    repeat: false,
                },
                Tween::default(),
                &vfs,
            )
        });
    if let Err(e) = &started {
        log::warn!("Movie audio could not be played: {e:?}");
    }
    started.is_ok()
}

fn stop_movie_audio(audio: &AudioManager) {
    audio.backend().stop(AudioStreamId::MOVIE, Tween::default());
    audio.backend().unload(AudioStreamId::MOVIE);
}

#[cfg(feature = "mp4")]
impl MovieSource for Mp4Playback {
    fn next_frame(&mut self) -> Result<Option<MovieFrame>> {
        let Some(frame) = self.next_due_frame() else {
            return Ok(None);
        };
        if frame.pts_us == self.last_presented_pts {
            return Ok(None);
        }
        self.last_presented_pts = frame.pts_us;
        Ok(Some(MovieFrame {
            rgba: frame.data.into_vec(),
            width: frame.width,
            height: frame.height,
        }))
    }

    fn is_finished(&mut self) -> Result<bool> {
        Ok(Mp4Playback::is_finished(self))
    }

    fn stop(self: Box<Self>) {
        self.stop_and_cleanup();
    }
}

impl MovieSource for WmvPlayback {
    fn next_frame(&mut self) -> Result<Option<MovieFrame>> {
        let Some(frame) = self.next_due_frame() else {
            return Ok(None);
        };
        if frame.pts_ms == self.last_presented_pts_ms {
            return Ok(None);
        }
        self.last_presented_pts_ms = frame.pts_ms;
        Ok(Some(MovieFrame {
            rgba: frame.rgba,
            width: self.screen_w,
            height: self.screen_h,
        }))
    }

    fn is_finished(&mut self) -> Result<bool> {
        Ok(WmvPlayback::is_finished(self))
    }

    fn stop(self: Box<Self>) {
        self.stop_and_cleanup();
    }
}

impl MovieSource for MpegPlayback {
    fn next_frame(&mut self) -> Result<Option<MovieFrame>> {
        let Some(frame) = self.next_due_frame() else {
            return Ok(None);
        };
        if frame.pts_ms == self.last_presented_pts_ms {
            return Ok(None);
        }
        self.last_presented_pts_ms = frame.pts_ms;
        Ok(Some(MovieFrame {
            rgba: frame.rgba,
            width: self.screen_w,
            height: self.screen_h,
        }))
    }

    fn is_finished(&mut self) -> Result<bool> {
        Ok(MpegPlayback::is_finished(self))
    }

    fn stop(self: Box<Self>) {
        self.stop_and_cleanup();
    }
}

/// Open a movie for the threaded decoders, which read a file: the movie itself when it is a
/// loose file of the game, otherwise a copy extracted from its pack into a cache directory.
pub(super) fn open(request: &MovieRequest<'_>) -> Result<Box<dyn MovieSource>> {
    let path = resolve_movie_real_path(request.vfs, request.name)?;
    let audio = request.audio.clone();
    let (mode, screen_w, screen_h) = (request.mode, request.screen_w, request.screen_h);
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "wmv" || ext == "asf" {
        Ok(Box::new(WmvPlayback::open_from_wmv_path(
            &path, mode, screen_w, screen_h, audio,
        )?))
    } else if matches!(
        ext.as_str(),
        "mpg" | "mpeg" | "m2v" | "ts" | "ps" | "vob" | "dat"
    ) {
        Ok(Box::new(MpegPlayback::open_from_mpeg_path(
            &path, mode, screen_w, screen_h, audio,
        )?))
    } else {
        #[cfg(feature = "mp4")]
        {
            Ok(Box::new(Mp4Playback::open_from_mp4_bytes(&path, mode, audio)?))
        }
        #[cfg(not(feature = "mp4"))]
        {
            let _ = (mode, audio);
            Err(anyhow!(
                "mp4 movie backend is disabled (build without feature \"mp4\")"
            ))
        }
    }
}

fn safe_rel_path_from_vfs(path: &str) -> Result<PathBuf> {
    use std::path::Component;

    let mut out = PathBuf::new();
    for c in Path::new(path).components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
                anyhow::bail!("invalid vfs path for cache: {path}");
            }
        }
    }
    if out.as_os_str().is_empty() {
        anyhow::bail!("empty vfs path for cache: {path}");
    }
    Ok(out)
}

fn video_cache_root_dir() -> PathBuf {
    let preferred = app_base_path()
        .join(".rfvp_cache")
        .join("video")
        .get_path()
        .clone();
    if std::fs::create_dir_all(&preferred).is_ok() {
        return preferred;
    }

    let fallback = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".rfvp_cache")
        .join("video");
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

fn resolve_movie_real_path(vfs: &Vfs, mapped: &str) -> Result<PathBuf> {
    let fs_path = app_base_path().join(mapped).get_path().clone();
    if fs_path.exists() {
        return Ok(fs_path);
    }

    let cache_root = video_cache_root_dir();
    let rel = safe_rel_path_from_vfs(mapped)?;
    let cache_path = cache_root.join(rel);

    if cache_path.exists() {
        return Ok(cache_path);
    }

    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create cache dir {}", parent.display()))?;
    }

    let pid = std::process::id();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_name = match cache_path.file_name().and_then(|s| s.to_str()) {
        Some(name) => format!("{name}.part_{pid}_{now}"),
        None => format!("movie.part_{pid}_{now}"),
    };
    let tmp_path = cache_path.with_file_name(tmp_name);

    {
        let mut src = vfs
            .open_stream(mapped)
            .with_context(|| format!("vfs.open_stream({mapped})"))?;
        let mut dst = std::fs::File::create(&tmp_path)
            .with_context(|| format!("create cache tmp {}", tmp_path.display()))?;
        std::io::copy(&mut src, &mut dst)
            .with_context(|| format!("copy movie bytes to {}", tmp_path.display()))?;
        let _ = dst.sync_all();
    }

    match std::fs::rename(&tmp_path, &cache_path) {
        Ok(()) => {}
        Err(e) if cache_path.exists() => {
            let _ = std::fs::remove_file(&tmp_path);
            log::debug!(
                "Movie cache race: {} already exists ({e:?})",
                cache_path.display()
            );
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(anyhow!(
                "rename cache tmp {} -> {}: {e}",
                tmp_path.display(),
                cache_path.display()
            ));
        }
    }

    Ok(cache_path)
}

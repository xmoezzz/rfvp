use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

use anyhow::{anyhow, Context, Result};

use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::Tween;

use crate::rfvp_audio::AudioManager;

use symphonia::core::audio::{SampleBuffer, SignalSpec};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::{get_codecs, get_probe};

use super::motion_manager::MotionManager;
use super::prim::PrimType;

/// GraphBuff slot reserved for Movie textures.
///
/// Text buffers use 4064..4095, so we pick 4063.
pub const MOVIE_GRAPH_ID: u16 = 4063;

/// Reserved primitive ids for the Movie layer.
/// These are internal slots; public scripts generally operate on 1..=4095.
pub const MOVIE_GROUP_PRIM_ID: i16 = 4095;
pub const MOVIE_SPRT_PRIM_ID: i16 = 4094;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieMode {
    /// Normal playback: video + audio while freezing other engine actions.
    ModalWithAudio,
    /// Render as a layer: video only, engine continues.
    LayerNoAudio,
}

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
    audio_data: Option<StaticSoundData>,
    audio_handle: Option<StaticSoundHandle>,
    audio_started: bool,
}

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
                let data = match (am.as_ref(), decode_mp4_audio_to_wav_bytes(&mp4_path)) {
                    (Some(_), Ok(Some(wav_bytes))) => {
                        let cursor = std::io::Cursor::new(wav_bytes);
                        Some(
                            StaticSoundData::from_cursor(cursor)
                                .context("kira StaticSoundData::from_cursor")?,
                        )
                    }
                    (_, Ok(None)) => None,
                    (_, Err(e)) => {
                        // Audio is best-effort; keep video playing even if audio fails.
                        log::warn!("Movie audio decode failed: {e:?}");
                        None
                    }
                    (_, _) => {
                        log::warn!("Movie audio decode skipped: no AudioManager");
                        None
                    }
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
            audio_handle: None,
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

        // NOTE: Movie audio is typically non-looping.
        let settings = StaticSoundSettings::new().volume(1.0);
        let handle = am.play(data.with_settings(settings));
        self.audio_handle = Some(handle);
    }

    fn next_due_frame(&mut self) -> Option<video_sys::VideoFrame> {
        // 1) Drain decoded frames from the decoder thread.
        while let Some(f) = self.stream.try_recv_one() {
            self.stash.push_back(f);
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
        if let Some(mut h) = self.audio_handle.take() {
            h.stop(Tween::default());
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
    audio_data: Option<StaticSoundData>,
    audio_handle: Option<StaticSoundHandle>,
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
                let data = match (am.as_ref(), decode_wmv_audio_to_wav_bytes(&wmv_path)) {
                    (Some(_), Ok(Some(wav_bytes))) => {
                        let cursor = std::io::Cursor::new(wav_bytes);
                        Some(
                            StaticSoundData::from_cursor(cursor)
                                .context("kira StaticSoundData::from_cursor")?,
                        )
                    }
                    (_, Ok(None)) => None,
                    (_, Err(e)) => {
                        // Audio is best-effort; keep video playing even if audio fails.
                        log::warn!("WMV movie audio decode failed: {e:?}");
                        None
                    }
                    (_, _) => {
                        log::warn!("WMV movie audio decode skipped: no AudioManager");
                        None
                    }
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
            audio_handle: None,
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

        let settings = StaticSoundSettings::new().volume(1.0);
        let handle = am.play(data.with_settings(settings));
        self.audio_handle = Some(handle);
    }

    fn next_due_frame(&mut self) -> Option<WmvRgbaFrame> {
        // 1) Drain decoded frames from the decoder thread.
        while let Ok(f) = self.rx.try_recv() {
            self.stash.push_back(f);
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

        if let Some(mut h) = self.audio_handle.take() {
            h.stop(Tween::default());
        }
    }
}

#[derive(Debug)]
enum Playback {
    Mp4(Mp4Playback),
    Wmv(WmvPlayback),
}

/// A Movie manager that wires the syscall to the render tree.
///
/// MP4 decoding uses `video-sys` and yields RGBA frames.
///
/// WMV decoding uses the local ASF/WMV2 decoder crate (`wmv-decoder`) and yields YUV frames.
/// For WMV, we convert+scale frames into a **screen-sized** RGBA buffer before uploading to the
/// reserved GraphBuff. This avoids touching prim scale fields (which scripts may manipulate).
#[derive(Debug, Default)]
pub struct VideoPlayerManager {
    playing: bool,
    modal: bool,
    playback: Option<Playback>,
}

impl VideoPlayerManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Whether a movie graph/resources are currently loaded.
    ///
    /// This matches the original engine's `is_loaded` flag used by `MovieState(1)`.
    pub fn is_loaded(&self) -> bool {
        self.playback.is_some()
    }

    pub fn is_modal_active(&self) -> bool {
        self.playing && self.modal
    }

    pub fn start(
        &mut self,
        movie_path: impl AsRef<Path>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        motion: &mut MotionManager,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<()> {
        // Replace any currently-playing movie.
        if self.playing {
            self.stop(motion);
        }

        let p = movie_path.as_ref();
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let playback = if ext == "wmv" || ext == "asf" {
            Playback::Wmv(WmvPlayback::open_from_wmv_path(
                p,
                mode,
                screen_w,
                screen_h,
                audio_manager,
            )?)
        } else {
            Playback::Mp4(Mp4Playback::open_from_mp4_bytes(p, mode, audio_manager)?)
        };

        self.playing = true;
        self.modal = matches!(mode, MovieMode::ModalWithAudio);
        self.playback = Some(playback);

        self.ensure_layer(motion, screen_w as i16, screen_h as i16);

        // Prime the first frame.
        self.tick(motion)?;

        Ok(())
    }

    pub fn tick(&mut self, motion: &mut MotionManager) -> Result<()> {
        if !self.playing {
            return Ok(());
        }

        let Some(pb) = self.playback.as_mut() else {
            self.playing = false;
            self.modal = false;
            return Ok(());
        };

        match pb {
            Playback::Mp4(mp4) => {
                if let Some(frame) = mp4.next_due_frame() {
                    if frame.pts_us != mp4.last_presented_pts {
                        mp4.last_presented_pts = frame.pts_us;
                        motion.load_texture_from_buff(
                            MOVIE_GRAPH_ID,
                            frame.data.into_vec(),
                            frame.width,
                            frame.height,
                        )?;
                        motion.refresh_prims(MOVIE_GRAPH_ID);
                    }
                }

                if mp4.is_finished() {
                    self.stop(motion);
                }
            }
            Playback::Wmv(wmv) => {
                if let Some(frame) = wmv.next_due_frame() {
                    if frame.pts_ms != wmv.last_presented_pts_ms {
                        wmv.last_presented_pts_ms = frame.pts_ms;
                        motion.load_texture_from_buff(
                            MOVIE_GRAPH_ID,
                            frame.rgba,
                            wmv.screen_w,
                            wmv.screen_h,
                        )?;
                        motion.refresh_prims(MOVIE_GRAPH_ID);
                    }
                }

                if wmv.is_finished() {
                    self.stop(motion);
                }
            }
        }

        Ok(())
    }

    pub fn stop(&mut self, motion: &mut MotionManager) {
        self.playing = false;
        self.modal = false;

        // Hide and unlink the movie layer primitives.
        // Keep the GraphBuff intact; it can be overwritten on next start.
        let pm = &mut motion.prim_manager;
        pm.prim_set_draw(MOVIE_SPRT_PRIM_ID as i32, 0);
        pm.prim_set_draw(MOVIE_GROUP_PRIM_ID as i32, 0);
        pm.unlink_prim(MOVIE_SPRT_PRIM_ID);
        pm.unlink_prim(MOVIE_GROUP_PRIM_ID);

        if let Some(pb) = self.playback.take() {
            match pb {
                Playback::Mp4(mp4) => mp4.stop_and_cleanup(),
                Playback::Wmv(wmv) => wmv.stop_and_cleanup(),
            }
        }
    }

    fn ensure_layer(&self, motion: &mut MotionManager, screen_w: i16, screen_h: i16) {
        let pm = &mut motion.prim_manager;

        // Match the original engine draw order: the movie layer belongs to the root=0 prim tree
        // (drawn in the "root0" pass), not the custom/overlay root.
        let root = 0i32;

        // Group container.
        pm.prim_init_with_type(MOVIE_GROUP_PRIM_ID, PrimType::PrimTypeGroup);
        pm.prim_set_pos(MOVIE_GROUP_PRIM_ID as i32, 0, 0);
        pm.prim_set_z(MOVIE_GROUP_PRIM_ID as i32, 32767);
        pm.prim_set_draw(MOVIE_GROUP_PRIM_ID as i32, 1);

        // Sprite: bind texture_id = -2 (movie).
        pm.prim_init_with_type(MOVIE_SPRT_PRIM_ID, PrimType::PrimTypeSprt);
        pm.prim_set_texture_id(MOVIE_SPRT_PRIM_ID as i32, -2);
        pm.prim_set_uv(MOVIE_SPRT_PRIM_ID as i32, 0, 0);
        pm.prim_set_pos(MOVIE_SPRT_PRIM_ID as i32, 0, 0);
        pm.prim_set_size(MOVIE_SPRT_PRIM_ID as i32, screen_w as i32, screen_h as i32);
        pm.prim_set_alpha(MOVIE_SPRT_PRIM_ID as i32, 255);
        pm.prim_set_blend(MOVIE_SPRT_PRIM_ID as i32, 0);
        pm.prim_set_z(MOVIE_SPRT_PRIM_ID as i32, 32767);
        pm.prim_set_draw(MOVIE_SPRT_PRIM_ID as i32, 1);

        // Attach sprite to movie group, then group to root (append-to-end semantics).
        pm.set_prim_group_in(MOVIE_GROUP_PRIM_ID as i32, MOVIE_SPRT_PRIM_ID as i32);
        pm.set_prim_group_in(root, MOVIE_GROUP_PRIM_ID as i32);
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

    let out_len = (dw as usize)
        .saturating_mul(dh as usize)
        .saturating_mul(4);
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
// MP4 audio (AAC) best-effort decode to WAV
// ─────────────────────────────────────────────────────────────────────────────

/// Best-effort: extract the first AAC audio track in the MP4 and decode it to WAV(PCM16LE).
///
/// Returns `Ok(None)` if no decodable audio track is present.
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
        Err(_) => return Ok(None),
    };

    let mut dec = match wmv_decoder::AsfWmaDecoder::open(std::io::BufReader::new(f)) {
        Ok(d) => d,
        Err(_) => return Ok(None),
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

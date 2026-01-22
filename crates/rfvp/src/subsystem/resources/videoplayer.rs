use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
struct MoviePlayback {
    stream: video_sys::VideoStream,
    stash: VecDeque<video_sys::VideoFrame>,

    started_at: Option<Instant>,
    base_pts_us: Option<i64>,
    last_presented_pts: i64,

    // The system decoder requires a real filesystem path; we materialize the VFS bytes here.
    temp_path: PathBuf,

    // Audio (decoded to a WAV container and played via Kira).
    audio_manager: Option<Arc<AudioManager>>,
    audio_data: Option<StaticSoundData>,
    audio_handle: Option<StaticSoundHandle>,
    audio_started: bool,
}

impl MoviePlayback {
    fn open_from_mp4_bytes(
        mp4_bytes: &[u8],
        mode: MovieMode,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<Self> {
        let temp_path = write_temp_mp4(mp4_bytes).context("write temp mp4")?;
        let stream = video_sys::VideoStream::open(&temp_path).context("VideoStream::open")?;

        let (audio_data, audio_manager) = match mode {
            MovieMode::ModalWithAudio => {
                let am = audio_manager;
                let data = match (am.as_ref(), decode_mp4_audio_to_wav_bytes(mp4_bytes)) {
                    (Some(_), Ok(Some(wav_bytes))) => {
                        let cursor = std::io::Cursor::new(wav_bytes);
                        Some(StaticSoundData::from_cursor(cursor).context("kira StaticSoundData::from_cursor")?)
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
            temp_path,
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

        // Best-effort cleanup.
        if let Err(e) = std::fs::remove_file(&self.temp_path) {
            log::debug!("remove temp mp4 {} failed: {e:?}", self.temp_path.display());
        }
    }
}

/// A Movie manager that wires the syscall to the render tree.
///
/// Video decoding uses the user's system-decoder implementation (`video-sys`), producing RGBA frames
/// in a background thread. The engine selects the latest due frame based on wall-clock and PTS.
///
/// For modal playback (flag==0), we additionally decode MP4/AAC to PCM using Symphonia, wrap it into
/// an in-memory WAV, and play it through Kira.
#[derive(Debug, Default)]
pub struct VideoPlayerManager {
    playing: bool,
    modal: bool,
    playback: Option<MoviePlayback>,
}

impl VideoPlayerManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn is_modal_active(&self) -> bool {
        self.playing && self.modal
    }

    pub fn start(
        &mut self,
        mp4_bytes: Vec<u8>,
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

        let playback = MoviePlayback::open_from_mp4_bytes(&mp4_bytes, mode, audio_manager)?;

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

        if let Some(frame) = pb.next_due_frame() {
            // Avoid redundant uploads.
            if frame.pts_us != pb.last_presented_pts {
                pb.last_presented_pts = frame.pts_us;
                motion.load_texture_from_buff(MOVIE_GRAPH_ID, frame.rgba, frame.width, frame.height)?;
                motion.refresh_prims(MOVIE_GRAPH_ID);
            }
        }

        if pb.is_finished() {
            self.stop(motion);
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

        // Stop decoder thread, audio, and delete the temp mp4 file.
        if let Some(pb) = self.playback.take() {
            pb.stop_and_cleanup();
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

fn write_temp_mp4(mp4_bytes: &[u8]) -> Result<PathBuf> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();

    let name = format!("rfvp_movie_{pid}_{now}.mp4");
    let path = std::env::temp_dir().join(name);

    std::fs::write(&path, mp4_bytes)
        .with_context(|| format!("write temp mp4 {}", path.display()))?;

    Ok(path)
}

/// Best-effort: extract the first AAC audio track in the MP4 and decode it to WAV(PCM16LE).
///
/// Returns `Ok(None)` if no decodable audio track is present.
fn decode_mp4_audio_to_wav_bytes(mp4_bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    // Hint Symphonia that this is MP4.
    let mut hint = Hint::new();
    hint.with_extension("mp4");

    let cursor = std::io::Cursor::new(mp4_bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let probed = get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
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

    let sr = track.codec_params.sample_rate.ok_or_else(|| anyhow!("mp4 audio: missing sample_rate"))?;
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

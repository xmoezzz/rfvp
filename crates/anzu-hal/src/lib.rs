//! anzu-hal — bare-metal audio HAL for rfvp.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────┐
//! │  AudioSystem  (public API)           │
//! │  ┌──────────┐  ┌──────────────────┐  │
//! │  │SoftMixer │→ │  TaskScheduler   │  │
//! │  │(software │  │(fade/stop tasks) │  │
//! │  │ mixing)  │  └──────────────────┘  │
//! │  └────┬─────┘                        │
//! │       │ mix_into()                   │
//! │  ┌────▼─────────────────────────┐    │
//! │  │  AudioDriver  (hardware I/O) │    │
//! │  │  AC97 (x86_64) / null (else) │    │
//! │  └──────────────────────────────┘    │
//! └──────────────────────────────────────┘
//! ```
//!
//! Call `AudioSystem::tick(delta_ms)` every frame to advance fade tasks and
//! refill the hardware DMA ring.

pub mod decode;
pub mod driver;
pub mod mixer;
pub mod task;

/// Set the PCIe ECAM base address from platform init code (aarch64 UEFI).
/// Must be called before `AudioSystem::new()` on aarch64.
pub fn set_pcie_ecam_base(base: u64) {
    driver::set_ecam_base(base);
}

use std::io::{Cursor, Read, Seek};
use std::sync::{Arc, Mutex};

use decode::DecodedPcm;
use driver::{create_driver, AudioDriver};
use mixer::SoftMixer;
use task::{Easing, FadeTask, PanTask, TaskScheduler};

// ─── Tween ────────────────────────────────────────────────────────────────────

/// Describes a smooth parameter transition (kira::Tween-compatible).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    /// Duration of the transition in milliseconds.
    pub duration_ms: u32,
}

impl Tween {
    pub const IMMEDIATE: Tween = Tween { duration_ms: 0 };

    pub fn ms(ms: u32) -> Self { Self { duration_ms: ms } }
    pub fn secs(s: f64) -> Self { Self { duration_ms: (s * 1000.0) as u32 } }
}

impl Default for Tween {
    fn default() -> Self { Self::IMMEDIATE }
}

// ─── Region (loop points) ─────────────────────────────────────────────────────

/// Loop region within a sound (kira::sound::Region-compatible).
#[derive(Clone, Copy, Debug, Default)]
pub struct Region {
    /// Loop start in stereo frames. 0 = beginning of file.
    pub start: u64,
    /// Loop end in stereo frames. None = end of file.
    pub end: Option<u64>,
}

impl Region {
    /// Loop the entire sound.
    pub fn full() -> Self { Self { start: 0, end: None } }
}

// ─── Panning ─────────────────────────────────────────────────────────────────

/// Stereo panning (kira::Panning-compatible).
///
/// 0.0 = fully left, 0.5 = centre, 1.0 = fully right.
/// Note: kira uses −1..1 with 0 = centre; rfvp passes 0.0–1.0 through the
/// existing bgm/se player code, so we keep the 0–1 convention here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Panning(pub f32);

impl Panning {
    pub const CENTER: Panning = Panning(0.5);
}

impl From<f32> for Panning {
    fn from(v: f32) -> Self { Panning(v.clamp(0.0, 1.0)) }
}

// ─── SoundData ───────────────────────────────────────────────────────────────

/// In-memory decoded audio, ready for playback (replaces kira StaticSoundData
/// and StreamingSoundData for anzu-hal builds).
#[derive(Clone)]
pub struct SoundData {
    pub(crate) pcm: Arc<DecodedPcm>,
}

impl SoundData {
    /// Decode audio from an in-memory byte slice (WAV or OGG Vorbis).
    pub fn from_bytes(data: &[u8]) -> anyhow::Result<Self> {
        let pcm = decode::decode_bytes(data)?;
        Ok(Self { pcm: Arc::new(pcm) })
    }

    /// Decode audio from a `Read` + `Seek` source.
    pub fn from_reader<R: Read + Seek>(mut reader: R) -> anyhow::Result<Self> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        Self::from_bytes(&buf)
    }

    /// Decode from a `Cursor<impl AsRef<[u8]>>` (matches kira's `from_cursor` API).
    pub fn from_cursor<T: AsRef<[u8]>>(cursor: Cursor<T>) -> anyhow::Result<Self> {
        Self::from_bytes(cursor.get_ref().as_ref())
    }

    /// Total stereo frames (samples / 2).
    pub fn frames(&self) -> u64 {
        self.pcm.samples.len() as u64 / 2
    }

    pub fn sample_rate(&self) -> u32 { self.pcm.sample_rate }
}

// ─── SoundState ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundState {
    Playing,
    Stopped,
}

impl SoundState {
    /// Returns true when the sound is still advancing (matches kira's API).
    pub fn is_advancing(self) -> bool { self == SoundState::Playing }
}

// ─── SoundHandle ─────────────────────────────────────────────────────────────

/// Handle to a sound currently managed by the audio system.
///
/// All methods are cheap (no allocation); they communicate with the `AudioSystem`
/// through the shared `Mutex`.
pub struct SoundHandle {
    channel: usize,
    system: Arc<Mutex<AudioSystemInner>>,
}

impl SoundHandle {
    fn with<F: FnOnce(&mut AudioSystemInner)>(&self, f: F) {
        if let Ok(mut g) = self.system.lock() { f(&mut g); }
    }

    /// Immediately stop the channel, or fade out over `tween.duration_ms`.
    pub fn stop(&mut self, tween: Tween) {
        let ch = self.channel;
        self.with(|s| {
            if tween.duration_ms == 0 {
                s.mixer.stop_channel(ch);
                s.tasks.cancel_fades_for(ch);
                s.tasks.cancel_stops_for(ch);
            } else {
                let cur_vol = s.mixer.channel_volume(ch);
                s.tasks.schedule_fade(FadeTask {
                    channel: ch,
                    from_vol: cur_vol,
                    to_vol: 0.0,
                    elapsed_ms: 0,
                    total_ms: tween.duration_ms,
                    easing: Easing::Linear,
                    stop_on_done: true,
                });
            }
        });
    }

    /// Set volume, optionally interpolated over `tween.duration_ms`.
    pub fn set_volume(&mut self, volume: f64, tween: Tween) {
        let vol = volume as f32;
        let ch = self.channel;
        self.with(|s| {
            if tween.duration_ms == 0 {
                s.mixer.set_channel_volume(ch, vol);
                s.tasks.cancel_fades_for(ch);
            } else {
                let cur = s.mixer.channel_volume(ch);
                s.tasks.schedule_fade(FadeTask {
                    channel: ch,
                    from_vol: cur,
                    to_vol: vol,
                    elapsed_ms: 0,
                    total_ms: tween.duration_ms,
                    easing: Easing::Linear,
                    stop_on_done: false,
                });
            }
        });
    }

    /// Set stereo pan (0.0 = left, 0.5 = centre, 1.0 = right).
    pub fn set_panning(&mut self, pan: Panning, tween: Tween) {
        let ch = self.channel;
        self.with(|s| {
            if tween.duration_ms == 0 {
                s.mixer.set_channel_pan(ch, pan.0);
                s.tasks.cancel_pans_for(ch);
            } else {
                let cur = s.mixer.channel_pan(ch);
                s.tasks.schedule_pan(PanTask {
                    channel: ch,
                    from_pan: cur,
                    to_pan: pan.0,
                    elapsed_ms: 0,
                    total_ms: tween.duration_ms,
                });
            }
        });
    }

    /// Query playback state.
    pub fn state(&self) -> SoundState {
        let active = self.system.lock()
            .map(|g| g.mixer.is_channel_active(self.channel))
            .unwrap_or(false);
        if active { SoundState::Playing } else { SoundState::Stopped }
    }
}

// ─── TrackHandle (API-compatibility stub) ─────────────────────────────────────

/// Stub for kira's `TrackHandle`. rfvp creates sub-tracks per BGM/SE slot but
/// never routes anything through them — the actual volume is set on the sound
/// handle.  We keep this type so the bgm/se player code compiles unchanged.
pub struct TrackHandle;

impl TrackHandle {
    pub fn set_volume(&mut self, _vol: impl Into<f64>, _tween: Tween) {}
}

/// Builder for sub-tracks (kira::track::TrackBuilder-compatible stub).
pub struct TrackBuilder;
impl TrackBuilder {
    pub fn new() -> Self { TrackBuilder }
}

// ─── Internal audio system ────────────────────────────────────────────────────

struct AudioSystemInner {
    mixer: SoftMixer,
    tasks: TaskScheduler,
    driver: Box<dyn AudioDriver>,
    master_volume: f32,
    /// Reusable mix buffer to avoid per-frame allocation.
    mix_buf: Vec<i16>,
}

impl AudioSystemInner {
    fn play_sound(
        &mut self,
        data: &SoundData,
        volume: f32,
        pan: f32,
        looping: bool,
        loop_region: Option<Region>,
        fade_in: Tween,
    ) -> Option<usize> {
        // Arc::clone is a cheap reference-count bump — no sample data copy.
        let samples = Arc::clone(&data.pcm.samples);
        let loop_start = loop_region.map(|r| r.start).unwrap_or(0);

        let start_vol = if fade_in.duration_ms > 0 { 0.0 } else { volume };
        let ch = self.mixer.play(samples, data.pcm.sample_rate, start_vol, pan, looping, loop_start)?;

        if fade_in.duration_ms > 0 {
            self.tasks.schedule_fade(FadeTask {
                channel: ch,
                from_vol: 0.0,
                to_vol: volume,
                elapsed_ms: 0,
                total_ms: fade_in.duration_ms,
                easing: Easing::Linear,
                stop_on_done: false,
            });
        }
        Some(ch)
    }

    fn tick(&mut self, delta_ms: u32) {
        self.tasks.tick(delta_ms, &mut self.mixer);

        let avail = self.driver.frames_available();
        if avail == 0 { return; }

        // Grow the reusable buffer if needed (never shrinks — that's intentional).
        let needed = avail * 2;
        if self.mix_buf.len() < needed {
            self.mix_buf.resize(needed, 0);
        }

        self.mixer.mix_into(&mut self.mix_buf[..needed], avail);
        self.driver.write_frames(&self.mix_buf[..needed], avail);
    }
}

// ─── Public AudioSystem ───────────────────────────────────────────────────────

/// The top-level audio system.  Create one instance, share it via `Arc`.
///
/// `tick(delta_ms)` must be called each frame to advance fade tasks and keep
/// the hardware DMA ring filled.
pub struct AudioSystem {
    inner: Arc<Mutex<AudioSystemInner>>,
}

impl AudioSystem {
    pub fn new() -> Self {
        let driver = create_driver();
        let inner = AudioSystemInner {
            mixer: SoftMixer::new(),
            tasks: TaskScheduler::default(),
            driver,
            master_volume: 1.0,
            mix_buf: Vec::new(),
        };
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn is_audio_available(&self) -> bool {
        self.inner.lock().map(|g| g.driver.is_available()).unwrap_or(false)
    }

    /// Advance all fade tasks and push audio to hardware.  Call once per frame.
    pub fn tick(&self, delta_ms: u32) {
        if let Ok(mut g) = self.inner.lock() { g.tick(delta_ms); }
    }

    pub fn set_master_volume(&self, vol: f32) {
        if let Ok(mut g) = self.inner.lock() {
            let v = vol.clamp(0.0, 1.0);
            g.master_volume = v;
            g.mixer.set_master_volume(v);
        }
    }

    /// Play a sound. Returns a handle for volume/pan/stop control.
    pub fn play(
        &self,
        data: &SoundData,
        volume: f32,
        pan: f32,
        looping: bool,
        loop_region: Option<Region>,
        fade_in: Tween,
    ) -> Option<SoundHandle> {
        let mut g = self.inner.lock().ok()?;
        let ch = g.play_sound(data, volume, pan, looping, loop_region, fade_in)?;
        Some(SoundHandle { channel: ch, system: Arc::clone(&self.inner) })
    }

    /// Create a dummy sub-track (kira-compat; does not affect audio routing).
    pub fn add_sub_track(&self, _builder: TrackBuilder) -> anyhow::Result<TrackHandle> {
        Ok(TrackHandle)
    }
}

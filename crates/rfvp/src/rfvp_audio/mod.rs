//! Audio output.
//!
//! The BGM and SE players (`crate::audio_player`) hold what the scripts asked for: which sound is
//! in which slot, its volume, pan and type. They play it through an [`AudioBackend`], the one
//! part that differs by platform:
//!
//! - kira on desktops, phones and in the browser ([`kira`]),
//! - the Anzu HAL on UEFI ([`anzu`]),
//! - commands for the host's `RfvpAudio` on the `no_std` ports ([`CommandBackend`]),
//! - nothing, where a build has no audio ([`NullBackend`]).

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use anyhow::Result;

use crate::host_api::{AudioParams, AudioStreamDesc, AudioStreamId, EncodedAudioKind};
use crate::platform_time::Duration;
use crate::subsystem::resources::vfs::Vfs;

#[cfg(all(not(feature = "no_std"), target_os = "uefi", feature = "anzu-audio"))]
pub mod anzu;
mod commands;
/// Live PCM streams need `std`'s condvars; the `no_std` core has no audio output
/// that could consume them.
#[cfg(not(feature = "no_std"))]
pub mod pcm_stream;
#[cfg(all(not(feature = "no_std"), feature = "audio", not(target_os = "uefi")))]
pub mod kira;

pub use commands::{AudioCommand, CommandBackend};

/// How long a change of volume or pan takes, or a fade in or out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub duration: Duration,
}

impl Default for Tween {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(0),
        }
    }
}

impl Tween {
    pub fn duration_ms(&self) -> u32 {
        self.duration.as_millis().min(u128::from(u32::MAX)) as u32
    }
}

/// What a slot plays: a file of the game, or bytes the script handed over.
#[derive(Clone, Debug)]
pub enum SoundSource {
    Named(String),
    Bytes(Arc<[u8]>),
    /// A stream produced while it is read, e.g. a movie's soundtrack decoded on
    /// the fly. Needs `std` for the shared reader.
    #[cfg(not(feature = "no_std"))]
    Stream(AudioStreamSource),
}

/// A byte source the audio backend reads as it plays.
#[cfg(not(feature = "no_std"))]
pub trait ReadSeek: std::io::Read + std::io::Seek {}
#[cfg(not(feature = "no_std"))]
impl<T: std::io::Read + std::io::Seek> ReadSeek for T {}

/// A byte stream produced while it is read.
///
/// Cheap to clone; every clone reads the same stream, which is what lets the
/// backend validate the format and then play it.
#[cfg(not(feature = "no_std"))]
#[derive(Clone)]
pub struct AudioStreamSource(Arc<std::sync::Mutex<Box<dyn ReadSeek + Send + Sync>>>);

#[cfg(not(feature = "no_std"))]
impl AudioStreamSource {
    pub fn new(stream: Box<dyn ReadSeek + Send + Sync>) -> Self {
        Self(Arc::new(std::sync::Mutex::new(stream)))
    }

    /// Run `f` with the underlying stream, locking out other readers.
    pub fn with<R>(&self, f: impl FnOnce(&mut Box<dyn ReadSeek + Send + Sync>) -> R) -> R {
        let mut guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }
}

#[cfg(not(feature = "no_std"))]
impl core::fmt::Debug for AudioStreamSource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AudioStreamSource")
    }
}

impl SoundSource {
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Named(name) => Some(name),
            Self::Bytes(_) => None,
            #[cfg(not(feature = "no_std"))]
            Self::Stream(_) => None,
        }
    }

    pub fn encoded_kind(&self) -> EncodedAudioKind {
        #[cfg(not(feature = "no_std"))]
        if matches!(self, Self::Stream(_)) {
            // A live soundtrack stream is PCM in a WAV container.
            return EncodedAudioKind::Wav;
        }
        let Some(name) = self.name() else {
            return EncodedAudioKind::Unknown;
        };
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".wav") {
            EncodedAudioKind::Wav
        } else if lower.ends_with(".ogg") {
            EncodedAudioKind::Ogg
        } else if lower.ends_with(".mp3") {
            EncodedAudioKind::Mp3
        } else if lower.ends_with(".flac") {
            EncodedAudioKind::Flac
        } else {
            EncodedAudioKind::Unknown
        }
    }

    /// The whole encoded sound.
    pub fn read(&self, vfs: &Vfs) -> Result<Vec<u8>> {
        match self {
            Self::Named(name) => vfs.read_file(name),
            Self::Bytes(bytes) => Ok(bytes.to_vec()),
            #[cfg(not(feature = "no_std"))]
            Self::Stream(_) => Err(anyhow::anyhow!(
                "a live stream cannot be read whole; load it with `load_pcm_stream`"
            )),
        }
    }
}

/// A platform's audio output, addressed by stream id (one per BGM or SE slot).
pub trait AudioBackend: Send + Sync {
    /// Get `id` ready to play `source`. Fails if the sound cannot be read or decoded.
    fn load(&self, id: AudioStreamId, source: &SoundSource, vfs: &Vfs) -> Result<()>;

    /// Forget the sound loaded on `id` (it is stopped first).
    fn unload(&self, id: AudioStreamId);

    /// Start the sound loaded on `id` from the beginning, replacing what was playing there.
    fn play(&self, id: AudioStreamId, params: AudioParams, fade_in: Tween, vfs: &Vfs)
        -> Result<()>;

    /// Change the volume and pan of what is playing on `id`.
    fn set_params(&self, id: AudioStreamId, params: AudioParams, tween: Tween);

    fn stop(&self, id: AudioStreamId, fade_out: Tween);

    fn is_playing(&self, id: AudioStreamId) -> bool;

    fn set_master_volume(&self, volume: f32);

    /// Whether the host consumes [`AudioCommand::CreateStream`] and
    /// [`AudioCommand::SubmitI16`] / [`AudioCommand::SubmitF32`].
    ///
    /// Only the host knows, because it is the host that implements
    /// [`crate::host_api::audio::RfvpAudio`]. The default is `false`, which keeps
    /// consumers on the whole-clip `load` path.
    fn supports_pcm_stream(&self) -> bool {
        false
    }

    /// Get `id` ready to play a source that is produced while it is read.
    ///
    /// The backend must pull from `source` on its own playback thread; that pull
    /// is what paces the producer. Backends without such a path leave this alone
    /// and callers fall back to a whole-clip load.
    #[cfg(not(feature = "no_std"))]
    fn load_pcm_stream(
        &self,
        _id: AudioStreamId,
        _source: AudioStreamSource,
        _vfs: &Vfs,
    ) -> Result<()> {
        Err(anyhow::anyhow!("this backend cannot play a live PCM stream"))
    }

    /// Advance a backend that mixes on the engine's thread.
    fn tick(&self, _delta_ms: u32) {}

    /// Commands for the host, for backends that produce them ([`CommandBackend`]).
    fn drain_commands(&self, _out: &mut Vec<AudioCommand>) {}

    /// Queue one command for the host. Backends that do not produce commands drop
    /// it, which is why callers must check [`AudioBackend::supports_pcm_stream`]
    /// before queueing a PCM-stream command.
    fn push_command(&self, _command: AudioCommand) {}
}

/// The audio output shared by the BGM and SE players.
pub struct AudioManager {
    backend: Box<dyn AudioBackend>,
}

impl core::fmt::Debug for AudioManager {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AudioManager")
    }
}

impl Default for AudioManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioManager {
    /// The build's audio output.
    pub fn new() -> Self {
        Self::with_backend(default_backend())
    }

    pub fn with_backend(backend: Box<dyn AudioBackend>) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &dyn AudioBackend {
        self.backend.as_ref()
    }

    pub fn master_vol(&self, volume: f32) {
        self.backend.set_master_volume(volume);
    }

    /// Whether [`Self::create_stream`] / [`Self::submit_i16`] are supported.
    ///
    /// The host opts in by implementing `RfvpAudio::create_stream` / `submit_i16`
    /// and building the backend with [`CommandBackend::with_pcm_stream`].
    pub fn pcm_stream_supported(&self) -> bool {
        self.backend.supports_pcm_stream()
    }

    /// Hand the backend a source that is produced while it is read.
    ///
    /// This is the whole-clip `load`'s streaming counterpart: the caller keeps
    /// producing samples while the backend plays them, so memory stays at the
    /// producer's buffer instead of the clip's size.
    #[cfg(not(feature = "no_std"))]
    pub fn load_pcm_stream(&self, id: AudioStreamId, source: AudioStreamSource) -> Result<()> {
        self.backend
            .load_pcm_stream(id, source, &Vfs::default())
    }

    /// Declare a decoded-PCM stream that the caller will fill with `submit_i16`.
    ///
    /// This is the streaming counterpart of `load`: instead of handing the host a
    /// whole decoded clip, the caller pushes chunks as it decodes, so a movie's
    /// soundtrack costs seconds of audio rather than the whole clip.
    ///
    /// Check [`Self::pcm_stream_supported`] first and fall back to `load`.
    pub fn create_stream(&self, id: AudioStreamId, desc: AudioStreamDesc) {
        if !self.backend.supports_pcm_stream() {
            log::warn!("pcm stream unsupported by host, stream {id:?} ignored");
            return;
        }
        self.backend.push_command(AudioCommand::CreateStream { id, desc });
    }

    /// Push one chunk of decoded PCM onto a stream created with
    /// [`Self::create_stream`].
    ///
    /// Keep chunks small and paced: every chunk is copied by the host, so this is
    /// the backpressure point of the streaming path.
    pub fn submit_i16(&self, id: AudioStreamId, samples: Vec<i16>) {
        if !self.backend.supports_pcm_stream() {
            return;
        }
        self.backend.push_command(AudioCommand::SubmitI16 { id, samples });
    }

    /// Tear down a stream created with [`Self::create_stream`].
    ///
    /// Without this a finished movie would leave the host holding its buffers, so
    /// the stop path must always pair `Stop` with `DestroyStream`.
    pub fn destroy_stream(&self, id: AudioStreamId) {
        if !self.backend.supports_pcm_stream() {
            return;
        }
        self.backend.push_command(AudioCommand::DestroyStream { id });
    }

    /// Stop a stream and release the host's buffers for it.
    ///
    /// The teardown has to pair the stop with the stream's destruction: without
    /// the destroy the host would keep holding the buffers of a stream nothing
    /// plays any more.
    pub fn stop_stream(&self, id: AudioStreamId) {
        if !self.backend.supports_pcm_stream() {
            return;
        }
        self.backend.push_command(AudioCommand::Stop { id, fade_ms: 0 });
        self.backend.push_command(AudioCommand::DestroyStream { id });
    }

    pub fn tick(&self, delta_ms: u32) {
        self.backend.tick(delta_ms);
    }

    pub fn drain_commands(&self, out: &mut Vec<AudioCommand>) {
        self.backend.drain_commands(out);
    }
}

#[cfg(feature = "no_std")]
fn default_backend() -> Box<dyn AudioBackend> {
    Box::new(CommandBackend::new())
}

#[cfg(all(not(feature = "no_std"), feature = "audio", not(target_os = "uefi")))]
fn default_backend() -> Box<dyn AudioBackend> {
    Box::new(kira::KiraBackend::new())
}

#[cfg(all(not(feature = "no_std"), target_os = "uefi", feature = "anzu-audio"))]
fn default_backend() -> Box<dyn AudioBackend> {
    Box::new(anzu::AnzuBackend::new())
}

#[cfg(all(
    not(feature = "no_std"),
    not(all(feature = "audio", not(target_os = "uefi"))),
    not(all(target_os = "uefi", feature = "anzu-audio"))
))]
fn default_backend() -> Box<dyn AudioBackend> {
    Box::new(NullBackend)
}

#[cfg(all(not(feature = "no_std"), feature = "audio", feature = "no-audio", not(target_os = "uefi")))]
compile_error!("features `audio` and `no-audio` are mutually exclusive outside UEFI builds.");

/// No audio output. Sounds count as finished as soon as they start, so scripts waiting for one
/// carry on.
pub struct NullBackend;

impl AudioBackend for NullBackend {
    fn load(&self, _id: AudioStreamId, _source: &SoundSource, _vfs: &Vfs) -> Result<()> {
        Ok(())
    }
    fn unload(&self, _id: AudioStreamId) {}
    fn play(&self, _: AudioStreamId, _: AudioParams, _: Tween, _: &Vfs) -> Result<()> {
        Ok(())
    }
    fn set_params(&self, _id: AudioStreamId, _params: AudioParams, _tween: Tween) {}
    fn stop(&self, _id: AudioStreamId, _fade_out: Tween) {}
    fn is_playing(&self, _id: AudioStreamId) -> bool {
        false
    }
    fn set_master_volume(&self, _volume: f32) {}
}

#[cfg(test)]
mod pcm_stream_tests {
    use super::*;
    use crate::host_api::AudioSampleFormat;

    fn desc() -> AudioStreamDesc {
        AudioStreamDesc {
            sample_rate: 44100,
            channels: 2,
            sample_format: AudioSampleFormat::I16,
        }
    }

    /// A host that has not opted in must receive nothing at all: a movie player
    /// checks `pcm_stream_supported()` and falls back to the whole-clip path, so
    /// even a stray command would mean the fallback is not actually taken.
    #[test]
    fn host_without_support_never_sees_stream_commands() {
        let am = AudioManager::with_backend(Box::new(CommandBackend::new()));
        assert!(!am.pcm_stream_supported());
        am.create_stream(AudioStreamId::MOVIE, desc());
        am.submit_i16(AudioStreamId::MOVIE, vec![0; 4]);
        am.stop_stream(AudioStreamId::MOVIE);

        let mut out = Vec::new();
        am.drain_commands(&mut out);
        assert!(out.is_empty(), "an unsupported host must receive nothing");
    }

    /// The full lifecycle, in order: declare the stream, feed it, stop it, and
    /// release the host's buffers. Missing the final `DestroyStream` is the leak
    /// that started this: the host would keep holding the buffers of a stream
    /// nothing plays any more.
    #[test]
    fn host_with_support_receives_the_full_stream_lifecycle() {
        let am = AudioManager::with_backend(Box::new(CommandBackend::with_pcm_stream()));
        assert!(am.pcm_stream_supported());
        am.create_stream(AudioStreamId::MOVIE, desc());
        am.submit_i16(AudioStreamId::MOVIE, vec![1, 2, 3, 4]);
        am.submit_i16(AudioStreamId::MOVIE, vec![5, 6, 7, 8]);
        am.stop_stream(AudioStreamId::MOVIE);

        let mut out = Vec::new();
        am.drain_commands(&mut out);
        assert_eq!(out.len(), 5, "create, two submits, stop, destroy");
        assert!(matches!(out[0], AudioCommand::CreateStream { .. }));
        assert!(matches!(out[1], AudioCommand::SubmitI16 { .. }));
        assert!(matches!(out[2], AudioCommand::SubmitI16 { .. }));
        assert!(matches!(out[3], AudioCommand::Stop { .. }));
        assert!(matches!(out[4], AudioCommand::DestroyStream { .. }));
    }
}

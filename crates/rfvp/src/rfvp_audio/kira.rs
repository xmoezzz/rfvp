//! The kira backend: desktops, phones and the browser.
//!
//! Native builds stream each sound from the VFS while it plays; the browser cannot read the packs
//! from kira's audio thread, so there a sound is decoded whole when it is loaded.

use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

use super::AudioStreamSource;
use anyhow::{anyhow, Context, Result};
#[cfg(target_arch = "wasm32")]
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
#[cfg(not(target_arch = "wasm32"))]
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
#[cfg(not(target_arch = "wasm32"))]
use kira::sound::FromFileError;
use kira::sound::Region;
use kira::{AudioManager as KiraAudioManager, AudioManagerSettings, Panning};

use super::{AudioBackend, SoundSource, Tween};
use crate::host_api::{AudioParams, AudioStreamId};
#[cfg(not(target_arch = "wasm32"))]
use crate::subsystem::resources::vfs::VfsStream;
use crate::subsystem::resources::vfs::Vfs;

/// kira takes volume in decibels; the engine's is linear.
fn decibels(volume: f32) -> kira::Decibels {
    if volume <= 0.0 {
        kira::Decibels::SILENCE
    } else {
        kira::Decibels(20.0 * volume.log10())
    }
}

fn kira_tween(tween: Tween) -> kira::Tween {
    kira::Tween {
        duration: tween.duration,
        ..Default::default()
    }
}

#[cfg(not(target_arch = "wasm32"))]
type Loaded = SoundSource;
#[cfg(not(target_arch = "wasm32"))]
type Handle = StreamingSoundHandle<FromFileError>;

#[cfg(target_arch = "wasm32")]
type Loaded = StaticSoundData;
#[cfg(target_arch = "wasm32")]
type Handle = StaticSoundHandle;

#[derive(Default)]
struct Channel {
    loaded: Option<Loaded>,
    handle: Option<Handle>,
}

pub struct KiraBackend {
    manager: Mutex<KiraAudioManager>,
    channels: Mutex<HashMap<u32, Channel>>,
}

impl KiraBackend {
    pub fn new() -> Self {
        // Every sound plays on the main track: Kira's default room for 128 sub-tracks (and send
        // tracks, clocks, modulators, listeners), each with buffers made up front, held tens of
        // MiB for nothing.
        let settings = AudioManagerSettings {
            capacities: ::kira::Capacities {
                sub_track_capacity: 1,
                send_track_capacity: 1,
                clock_capacity: 1,
                modulator_capacity: 1,
                listener_capacity: 1,
            },
            ..AudioManagerSettings::default()
        };
        let manager = KiraAudioManager::new(settings).expect("failed to create Kira AudioManager");
        Self {
            manager: Mutex::new(manager),
            channels: Mutex::new(HashMap::new()),
        }
    }

    fn channels(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Channel>> {
        self.channels.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    fn with_handle(&self, id: AudioStreamId, f: impl FnOnce(&mut Handle)) {
        if let Some(handle) = self
            .channels()
            .get_mut(&id.0)
            .and_then(|channel| channel.handle.as_mut())
        {
            f(handle);
        }
    }
}

impl Default for KiraBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn streaming_data(source: &SoundSource, vfs: &Vfs) -> Result<StreamingSoundData<FromFileError>> {
    match source {
        SoundSource::Named(name) => {
            let (stream, byte_len) = vfs
                .open_stream_with_len(name)
                .with_context(|| format!("open audio stream from vfs: {name}"))?;
            StreamingSoundData::from_media_source(VfsMediaSource { stream, byte_len })
                .with_context(|| format!("decode audio from vfs stream: {name}"))
        }
        SoundSource::Bytes(bytes) => Ok(StreamingSoundData::from_cursor(std::io::Cursor::new(
            bytes.clone(),
        ))?),
        SoundSource::Stream(source) => Ok(StreamingSoundData::from_media_source(
            PcmMediaSource {
                inner: source.clone(),
            },
        )?),
    }
}

impl AudioBackend for KiraBackend {
    fn supports_pcm_stream(&self) -> bool {
        // kira pulls from a `MediaSource` on its own thread, so a producer can
        // feed it while it plays.
        cfg!(not(target_arch = "wasm32"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_pcm_stream(
        &self,
        id: AudioStreamId,
        source: AudioStreamSource,
        vfs: &Vfs,
    ) -> Result<()> {
        // No format probe here, unlike `load`: building a `StreamingSoundData`
        // consumes the stream (symphonia parses the header), and `play` builds
        // its own from the stored source. A live stream cannot be rewound, so a
        // probe would leave `play` starting after the header.
        let _ = vfs;
        self.channels().entry(id.0).or_default().loaded = Some(SoundSource::Stream(source));
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load(&self, id: AudioStreamId, source: &SoundSource, vfs: &Vfs) -> Result<()> {
        // Decode the header now so a bad file fails here rather than when it is played.
        streaming_data(source, vfs)?;
        self.channels().entry(id.0).or_default().loaded = Some(source.clone());
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    fn load(&self, id: AudioStreamId, source: &SoundSource, vfs: &Vfs) -> Result<()> {
        let bytes = source.read(vfs)?;
        let sound = StaticSoundData::from_cursor(std::io::Cursor::new(bytes))
            .with_context(|| format!("decode audio {:?}", source.name()))?;
        self.channels().entry(id.0).or_default().loaded = Some(sound);
        Ok(())
    }

    fn unload(&self, id: AudioStreamId) {
        if let Some(channel) = self.channels().get_mut(&id.0) {
            if let Some(mut handle) = channel.handle.take() {
                handle.stop(kira::Tween::default());
            }
            channel.loaded = None;
        }
    }

    fn play(&self, id: AudioStreamId, params: AudioParams, fade_in: Tween, vfs: &Vfs) -> Result<()> {
        let loaded = self
            .channels()
            .get(&id.0)
            .and_then(|channel| channel.loaded.clone())
            .ok_or_else(|| anyhow!("nothing is loaded on audio stream {}", id.0))?;
        let pan = Panning::from(params.pan);

        #[cfg(not(target_arch = "wasm32"))]
        let handle = {
            let mut sound = streaming_data(&loaded, vfs)?
                .panning(pan)
                .volume(decibels(params.volume))
                .fade_in_tween(kira_tween(fade_in));
            if params.repeat {
                sound = sound.loop_region(Region::default());
            }
            self.manager
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .play(sound)
                .map_err(|err| anyhow!("play audio stream {}: {err:?}", id.0))?
        };

        #[cfg(target_arch = "wasm32")]
        let handle = {
            let _ = vfs;
            let settings = StaticSoundSettings::new()
                .panning(pan)
                .volume(decibels(params.volume))
                .fade_in_tween(kira_tween(fade_in))
                .loop_region(params.repeat.then_some(Region::default()))
                .playback_rate(1.0);
            self.manager
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .play(loaded.with_settings(settings))
                .map_err(|err| anyhow!("play audio stream {}: {err:?}", id.0))?
        };

        let mut channels = self.channels();
        let channel = channels.entry(id.0).or_default();
        if let Some(mut old) = channel.handle.replace(handle) {
            old.stop(kira_tween(fade_in));
        }
        Ok(())
    }

    fn set_params(&self, id: AudioStreamId, params: AudioParams, tween: Tween) {
        self.with_handle(id, |handle| {
            handle.set_volume(decibels(params.volume), kira_tween(tween));
            handle.set_panning(Panning::from(params.pan), kira_tween(tween));
        });
    }

    fn stop(&self, id: AudioStreamId, fade_out: Tween) {
        if let Some(mut handle) = self
            .channels()
            .get_mut(&id.0)
            .and_then(|channel| channel.handle.take())
        {
            handle.stop(kira_tween(fade_out));
        }
    }

    fn is_playing(&self, id: AudioStreamId) -> bool {
        self.channels()
            .get(&id.0)
            .and_then(|channel| channel.handle.as_ref())
            .is_some_and(|handle| handle.state().is_advancing())
    }

    fn set_master_volume(&self, volume: f32) {
        self.manager
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .main_track()
            .set_volume(decibels(volume), kira::Tween::default());
    }
}

/// A VFS stream as a Symphonia media source, so kira streams sounds from the packs.
#[cfg(not(target_arch = "wasm32"))]
struct VfsMediaSource {
    stream: VfsStream,
    byte_len: Option<u64>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Read for VfsMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stream.read(buf)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Seek for VfsMediaSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.stream.seek(pos)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl symphonia_core::io::MediaSource for VfsMediaSource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}

/// A live PCM stream, presented to kira as a WAV byte source.
///
/// Not seekable: the bytes behind the read position were produced once and are
/// gone. kira reads it sequentially, which is all a soundtrack needs.
#[cfg(not(target_arch = "wasm32"))]
struct PcmMediaSource {
    inner: AudioStreamSource,
}

#[cfg(not(target_arch = "wasm32"))]
impl Read for PcmMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.with(|stream| stream.read(buf))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Seek for PcmMediaSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.with(|stream| stream.seek(pos))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl symphonia_core::io::MediaSource for PcmMediaSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod pcm_source_tests {
    use super::*;
    use crate::rfvp_audio::pcm_stream::PcmWavStream;

    /// kira parses the WAV header when it builds the sound, and our header
    /// declares a maximum data length because a live stream has no known end.
    /// If symphonia rejected that, `play` would fail and the movie would be
    /// silent instead of falling back, so this pins the acceptance.
    #[test]
    fn symphonia_accepts_a_live_pcm_wav_header() {
        let (stream, writer) = PcmWavStream::new(44100, 2);
        let samples: Vec<i16> = (0..44100).map(|i| ((i % 100) as i16) * 10).collect();
        writer.push(&samples);
        writer.finish();

        let source = AudioStreamSource::new(Box::new(stream));
        let built = StreamingSoundData::from_media_source(PcmMediaSource { inner: source });
        assert!(
            built.is_ok(),
            "symphonia rejected the live WAV header: {:?}",
            built.err()
        );
    }
}

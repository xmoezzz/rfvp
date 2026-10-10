//! The Anzu HAL backend: UEFI, where sounds are decoded whole and mixed on the engine's thread.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use anzu_hal::{AudioSystem, Panning, Region, SoundData, SoundHandle};

use super::{AudioBackend, SoundSource, Tween};
use crate::host_api::{AudioParams, AudioStreamId};
use crate::subsystem::resources::vfs::Vfs;

/// Anzu pans from 0 (left) to 1 (right); `AudioParams` from -1 to 1.
fn anzu_pan(pan: f32) -> f32 {
    (pan + 1.0) / 2.0
}

fn anzu_tween(tween: Tween) -> anzu_hal::Tween {
    anzu_hal::Tween::ms(tween.duration_ms())
}

#[derive(Default)]
struct Channel {
    data: Option<SoundData>,
    handle: Option<SoundHandle>,
}

pub struct AnzuBackend {
    system: Arc<AudioSystem>,
    channels: Mutex<HashMap<u32, Channel>>,
}

// SAFETY: UEFI boot services run the engine on a single processor; the audio system is never
// touched from two threads.
unsafe impl Send for AnzuBackend {}
unsafe impl Sync for AnzuBackend {}

impl AnzuBackend {
    pub fn new() -> Self {
        Self {
            system: Arc::new(AudioSystem::new()),
            channels: Mutex::new(HashMap::new()),
        }
    }

    fn channels(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Channel>> {
        self.channels.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

impl AudioBackend for AnzuBackend {
    fn load(&self, id: AudioStreamId, source: &SoundSource, vfs: &Vfs) -> Result<()> {
        let bytes = source.read(vfs)?;
        let data = SoundData::from_bytes(&bytes)
            .with_context(|| format!("decode audio {:?}", source.name()))?;
        self.channels().entry(id.0).or_default().data = Some(data);
        Ok(())
    }

    fn unload(&self, id: AudioStreamId) {
        if let Some(channel) = self.channels().get_mut(&id.0) {
            if let Some(mut handle) = channel.handle.take() {
                handle.stop(anzu_hal::Tween::IMMEDIATE);
            }
            channel.data = None;
        }
    }

    fn play(&self, id: AudioStreamId, params: AudioParams, fade_in: Tween, _vfs: &Vfs) -> Result<()> {
        let mut channels = self.channels();
        let channel = channels.entry(id.0).or_default();
        let Some(data) = channel.data.as_ref() else {
            anyhow::bail!("nothing is loaded on audio stream {}", id.0);
        };
        let handle = self.system.play(
            data,
            params.volume,
            anzu_pan(params.pan),
            params.repeat,
            params.repeat.then_some(Region::full()),
            anzu_tween(fade_in),
        );
        if let Some(mut old) = channel.handle.take() {
            old.stop(anzu_tween(fade_in));
        }
        channel.handle = handle;
        Ok(())
    }

    fn set_params(&self, id: AudioStreamId, params: AudioParams, tween: Tween) {
        if let Some(handle) = self
            .channels()
            .get_mut(&id.0)
            .and_then(|channel| channel.handle.as_mut())
        {
            handle.set_volume(params.volume as f64, anzu_tween(tween));
            handle.set_panning(Panning::from(anzu_pan(params.pan)), anzu_tween(tween));
        }
    }

    fn stop(&self, id: AudioStreamId, fade_out: Tween) {
        if let Some(mut handle) = self
            .channels()
            .get_mut(&id.0)
            .and_then(|channel| channel.handle.take())
        {
            handle.stop(anzu_tween(fade_out));
        }
    }

    fn is_playing(&self, id: AudioStreamId) -> bool {
        self.channels()
            .get(&id.0)
            .and_then(|channel| channel.handle.as_ref())
            .is_some_and(|handle| handle.state().is_advancing())
    }

    fn set_master_volume(&self, volume: f32) {
        self.system.set_master_volume(volume);
    }

    fn tick(&self, delta_ms: u32) {
        self.system.tick(delta_ms);
    }
}

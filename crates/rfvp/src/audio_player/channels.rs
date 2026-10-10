//! What BGM and SE share: a bank of slots the scripts load sounds into and play, each with its
//! own volume, pan, mute and sound type, played through the build's audio backend.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use anyhow::{anyhow, Result};

use crate::host_api::{AudioParams, AudioStreamId};
use crate::rfvp_audio::{AudioManager, SoundSource, Tween};
use crate::subsystem::resources::vfs::Vfs;

/// Sound types a slot can be given; each has a volume of its own (`SoundTypeVolume`).
pub const SOUND_TYPE_COUNT: usize = 10;

/// The state of one slot, as saved and restored with the game.
#[derive(Debug, Clone)]
pub struct SlotState {
    pub slot: usize,
    pub path: Option<String>,
    pub sound_type: Option<i32>,
    pub volume: f32,
    pub muted: bool,
    pub playing: bool,
    pub repeat: bool,
    pub pan: f32,
}

#[derive(Debug, Clone, Default)]
pub struct SlotInfo {
    pub slot: usize,
    pub name: Option<String>,
    pub volume: f64,
    pub muted: bool,
    pub kind: Option<i32>,
    pub data_loaded: bool,
    pub playing: bool,
}

pub struct Channels<const N: usize> {
    audio: Arc<AudioManager>,
    label: &'static str,
    stream_id: fn(usize) -> AudioStreamId,
    sources: [Option<SoundSource>; N],
    /// The slot was unloaded by the script; `names` still holds the source so a later play can
    /// load it again on demand.
    released: [bool; N],
    names: [Option<String>; N],
    kinds: [Option<i32>; N],
    type_volumes: [f32; SOUND_TYPE_COUNT],
    muted: [bool; N],
    volumes: [f32; N],
    repeat: [bool; N],
    pan: [f64; N],
}

impl<const N: usize> Channels<N> {
    pub fn new(
        audio: Arc<AudioManager>,
        label: &'static str,
        stream_id: fn(usize) -> AudioStreamId,
    ) -> Self {
        Self {
            audio,
            label,
            stream_id,
            sources: [(); N].map(|_| None),
            released: [false; N],
            names: [(); N].map(|_| None),
            kinds: [None; N],
            type_volumes: [1.0; SOUND_TYPE_COUNT],
            muted: [false; N],
            volumes: [1.0; N],
            repeat: [false; N],
            pan: [0.5; N],
        }
    }

    fn slot(&self, slot: i32) -> Result<usize> {
        usize::try_from(slot)
            .ok()
            .filter(|&slot| slot < N)
            .ok_or_else(|| anyhow!("invalid {} slot {slot}", self.label))
    }

    fn id(&self, slot: usize) -> AudioStreamId {
        (self.stream_id)(slot)
    }

    /// The slot's sound settings for the backend: linear volume, pan from -1 (left) to 1
    /// (right). Scripts keep pan from 0 to 1.
    fn params(&self, slot: usize) -> AudioParams {
        let type_volume = match self.kinds[slot] {
            Some(kind) if (0..SOUND_TYPE_COUNT as i32).contains(&kind) => {
                self.type_volumes[kind as usize]
            }
            _ => 1.0,
        };
        AudioParams {
            volume: if self.muted[slot] {
                0.0
            } else {
                self.volumes[slot] * type_volume
            },
            pan: (self.pan[slot] as f32) * 2.0 - 1.0,
            repeat: self.repeat[slot],
        }
    }

    fn apply_params(&self, slot: usize, tween: Tween) {
        self.audio
            .backend()
            .set_params(self.id(slot), self.params(slot), tween);
    }

    fn load_source(&mut self, slot: i32, source: SoundSource, vfs: &Vfs) -> Result<()> {
        let slot = self.slot(slot)?;
        self.audio.backend().load(self.id(slot), &source, vfs)?;
        self.names[slot] = source.name().map(String::from);
        self.sources[slot] = Some(source);
        self.released[slot] = false;
        Ok(())
    }

    pub fn load(&mut self, slot: i32, bytes: Vec<u8>, vfs: &Vfs) -> Result<()> {
        self.load_source(slot, SoundSource::Bytes(Arc::from(bytes)), vfs)
    }

    pub fn load_named(&mut self, slot: i32, name: String, vfs: &Vfs) -> Result<()> {
        self.load_source(slot, SoundSource::Named(name), vfs)
    }

    pub fn is_loaded(&self, slot: usize) -> bool {
        self.sources.get(slot).is_some_and(Option::is_some)
    }

    /// Script-level unload (`SoundLoad(ch, nil)`): stop the slot and drop its loaded sound.
    ///
    /// The slot name is kept so snapshots and a later play behave exactly as before (see
    /// [`Self::released_path`]).
    pub fn unload(&mut self, slot: i32, fade_out: Tween) {
        let Ok(slot) = self.slot(slot) else { return };
        self.audio.backend().stop(self.id(slot), fade_out);
        if self.sources[slot].take().is_some() {
            self.audio.backend().unload(self.id(slot));
            if self.names[slot].is_some() {
                self.released[slot] = true;
            }
        }
    }

    /// Source path of a slot that was unloaded by the script, if it must be loaded again before
    /// playing.
    pub fn released_path(&self, slot: i32) -> Option<&str> {
        let slot = self.slot(slot).ok()?;
        if self.released[slot] && !self.is_loaded(slot) {
            self.names[slot].as_deref()
        } else {
            None
        }
    }

    pub fn play(
        &mut self,
        slot: i32,
        repeat: bool,
        volume: f32,
        pan: f64,
        fade_in: Tween,
        vfs: &Vfs,
    ) -> Result<()> {
        let slot = self.slot(slot)?;
        self.repeat[slot] = repeat;
        self.volumes[slot] = volume;
        self.pan[slot] = pan;
        if !self.is_loaded(slot) {
            log::error!(
                "Tried to play {} slot {slot}, but nothing was loaded",
                self.label
            );
            return Ok(());
        }
        log::info!("Playing {} slot {slot}", self.label);
        self.audio
            .backend()
            .play(self.id(slot), self.params(slot), fade_in, vfs)
    }

    pub fn set_volume(&mut self, slot: i32, volume: f32, tween: Tween) {
        let Ok(slot) = self.slot(slot) else { return };
        self.volumes[slot] = volume;
        self.apply_params(slot, tween);
    }

    pub fn set_panning(&mut self, slot: i32, pan: f64, tween: Tween) {
        let Ok(slot) = self.slot(slot) else { return };
        self.pan[slot] = pan;
        self.apply_params(slot, tween);
    }

    /// Permanently silence a slot (`AudioSilentOn` / `SoundSilentOn`).
    pub fn silent_on(&mut self, slot: i32, tween: Tween) {
        let Ok(slot) = self.slot(slot) else { return };
        self.muted[slot] = true;
        self.apply_params(slot, tween);
    }

    pub fn stop(&mut self, slot: i32, fade_out: Tween) {
        let Ok(slot) = self.slot(slot) else { return };
        self.audio.backend().stop(self.id(slot), fade_out);
    }

    pub fn stop_all(&mut self, fade_out: Tween) {
        for slot in 0..N {
            if self.is_playing(slot as i32) {
                self.stop(slot as i32, fade_out);
            }
        }
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        self.slot(slot)
            .is_ok_and(|slot| self.audio.backend().is_playing(self.id(slot)))
    }

    pub fn playing_slots(&self) -> Vec<bool> {
        (0..N).map(|slot| self.is_playing(slot as i32)).collect()
    }

    pub fn set_type(&mut self, slot: i32, kind: i32) {
        let Ok(slot) = self.slot(slot) else { return };
        self.kinds[slot] = Some(kind);
        self.apply_params(slot, Tween::default());
    }

    pub fn set_type_volume(&mut self, kind: i32, volume: f32, tween: Tween) {
        if !(0..SOUND_TYPE_COUNT as i32).contains(&kind) {
            return;
        }
        self.type_volumes[kind as usize] = volume;
        for slot in 0..N {
            if self.kinds[slot] == Some(kind) {
                self.apply_params(slot, tween);
            }
        }
    }

    pub fn slot_infos(&self) -> Vec<SlotInfo> {
        (0..N)
            .filter_map(|slot| {
                let playing = self.is_playing(slot as i32);
                let data_loaded = self.is_loaded(slot);
                (playing || data_loaded || self.names[slot].is_some() || self.kinds[slot].is_some())
                    .then(|| SlotInfo {
                        slot,
                        name: self.names[slot].clone(),
                        volume: self.volumes[slot] as f64,
                        muted: self.muted[slot],
                        kind: self.kinds[slot],
                        data_loaded,
                        playing,
                    })
            })
            .collect()
    }

    pub fn capture(&self) -> Vec<SlotState> {
        (0..N)
            .filter(|&slot| {
                self.is_loaded(slot) || self.is_playing(slot as i32) || self.names[slot].is_some()
            })
            .map(|slot| SlotState {
                slot,
                path: self.names[slot].clone(),
                sound_type: self.kinds[slot],
                volume: self.volumes[slot],
                muted: self.muted[slot],
                playing: self.is_playing(slot as i32),
                repeat: self.repeat[slot],
                pan: self.pan[slot] as f32,
            })
            .collect()
    }

    /// Replace every slot with the saved ones.
    pub fn restore(&mut self, saved: impl IntoIterator<Item = SlotState>, vfs: &Vfs) -> Result<()> {
        for slot in 0..N {
            self.audio.backend().stop(self.id(slot), Tween::default());
            if self.sources[slot].take().is_some() {
                self.audio.backend().unload(self.id(slot));
            }
            self.released[slot] = false;
            self.kinds[slot] = None;
            self.names[slot] = None;
            self.muted[slot] = false;
            self.volumes[slot] = 1.0;
            self.repeat[slot] = false;
            self.pan[slot] = 0.5;
        }

        for state in saved {
            let slot = state.slot;
            if slot >= N {
                continue;
            }
            self.kinds[slot] = state.sound_type;
            self.muted[slot] = state.muted;
            self.volumes[slot] = state.volume;
            self.repeat[slot] = state.repeat;
            self.pan[slot] = state.pan as f64;
            if let Some(path) = state.path {
                self.load_named(slot as i32, path, vfs)?;
            }
            if state.playing && self.is_loaded(slot) {
                self.play(
                    slot as i32,
                    self.repeat[slot],
                    self.volumes[slot],
                    self.pan[slot],
                    Tween::default(),
                    vfs,
                )?;
            }
        }
        Ok(())
    }
}

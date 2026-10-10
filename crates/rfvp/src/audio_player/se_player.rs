//! Sound effects and voices: 256 channels (`SoundLoad`, `SoundPlay`, ...).

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::channels::{Channels, SlotInfo, SlotState};
use crate::host_api::AudioStreamId;
use crate::rfvp_audio::{AudioManager, Tween};
use crate::subsystem::resources::vfs::Vfs;

pub const SE_SLOT_COUNT: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeSlotSnapshotV1 {
    pub slot: u16,
    pub path: Option<String>,
    pub sound_type: Option<i32>,
    pub volume: f32,
    pub muted: bool,
    pub playing: bool,
    pub repeat: bool,
    pub pan: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SePlayerSnapshotV1 {
    pub version: u16,
    pub slots: Vec<SeSlotSnapshotV1>,
}

pub type SeSlotInfo = SlotInfo;

#[derive(Debug, Clone, Default)]
pub struct SeDebugSummary {
    pub max_datas: usize,
    pub max_slots: usize,
    pub loaded_datas: usize,
    pub playing_slots: usize,
    pub slots: Vec<SeSlotInfo>,
}

pub struct SePlayer {
    channels: Channels<SE_SLOT_COUNT>,
}

impl SePlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        Self {
            channels: Channels::new(audio_manager, "SE", AudioStreamId::se),
        }
    }

    /// Load an encoded sound handed over as bytes.
    pub fn load(&mut self, slot: i32, se: Vec<u8>) -> Result<()> {
        // Bytes need no game files.
        self.channels.load(slot, se, &Vfs::default())
    }

    pub fn load_named(&mut self, slot: i32, name: impl Into<String>, vfs: &Vfs) -> Result<()> {
        self.channels.load_named(slot, name.into(), vfs)
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
        self.channels.play(slot, repeat, volume, pan, fade_in, vfs)
    }

    pub fn set_volume(&mut self, slot: i32, volume: f32, tween: Tween) {
        self.channels.set_volume(slot, volume, tween);
    }

    pub fn silent_on(&mut self, slot: i32, tween: Tween) {
        self.channels.silent_on(slot, tween);
    }

    pub fn stop(&mut self, slot: i32, fade_out: Tween) {
        self.channels.stop(slot, fade_out);
    }

    pub fn stop_all(&mut self, fade_out: Tween) {
        self.channels.stop_all(fade_out);
    }

    pub fn set_panning(&mut self, slot: i32, pan: f64, tween: Tween) {
        self.channels.set_panning(slot, pan, tween);
    }

    /// Script-level unload (`SoundLoad(ch, nil)`): stop the slot and drop its loaded sound.
    pub fn unload(&mut self, slot: i32, fade_out: Tween) {
        self.channels.unload(slot, fade_out);
    }

    /// Source path of a slot that was unloaded by the script, if it must be loaded again before
    /// playing.
    pub fn released_path(&self, slot: i32) -> Option<&str> {
        self.channels.released_path(slot)
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        self.channels.is_playing(slot)
    }

    pub fn set_type(&mut self, slot: i32, kind: i32) {
        self.channels.set_type(slot, kind);
    }

    pub fn set_type_volume(&mut self, kind: i32, volume: f32, tween: Tween) {
        self.channels.set_type_volume(kind, volume, tween);
    }

    pub fn debug_summary(&self) -> SeDebugSummary {
        let slots = self.channels.slot_infos();
        SeDebugSummary {
            max_datas: SE_SLOT_COUNT,
            max_slots: SE_SLOT_COUNT,
            loaded_datas: slots.iter().filter(|slot| slot.data_loaded).count(),
            playing_slots: slots.iter().filter(|slot| slot.playing).count(),
            slots,
        }
    }

    pub fn capture_snapshot_v1(&self) -> SePlayerSnapshotV1 {
        let slots = self
            .channels
            .capture()
            .into_iter()
            .map(|state| SeSlotSnapshotV1 {
                slot: state.slot as u16,
                path: state.path,
                sound_type: state.sound_type,
                volume: state.volume,
                muted: state.muted,
                playing: state.playing,
                repeat: state.repeat,
                pan: state.pan,
            })
            .collect();
        SePlayerSnapshotV1 { version: 1, slots }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &SePlayerSnapshotV1, vfs: &Vfs) -> Result<()> {
        if snap.version != 1 {
            return Err(anyhow!(
                "unsupported SePlayerSnapshotV1 version: {}",
                snap.version
            ));
        }
        let saved = snap.slots.iter().map(|slot| SlotState {
            slot: slot.slot as usize,
            path: slot.path.clone(),
            sound_type: slot.sound_type,
            volume: slot.volume,
            muted: slot.muted,
            playing: slot.playing,
            repeat: slot.repeat,
            pan: slot.pan,
        });
        self.channels.restore(saved, vfs)
    }
}

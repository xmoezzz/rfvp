use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::bgm_player::SOUND_TYPE_COUNT;
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

pub struct SePlayer {
    _audio_manager: Arc<AudioManager>,
    se_loaded: [bool; SE_SLOT_COUNT],
    se_kinds: [Option<i32>; SE_SLOT_COUNT],
    se_names: [Option<String>; SE_SLOT_COUNT],
    se_type_volumes: [f32; SOUND_TYPE_COUNT],
    se_muted: [bool; SE_SLOT_COUNT],
    se_volumes: [f32; SE_SLOT_COUNT],
    se_repeat: [bool; SE_SLOT_COUNT],
    se_pan: [f64; SE_SLOT_COUNT],
    se_playing: [bool; SE_SLOT_COUNT],
}

impl SePlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        Self {
            _audio_manager: audio_manager,
            se_loaded: [false; SE_SLOT_COUNT],
            se_kinds: [(); SE_SLOT_COUNT].map(|_| None),
            se_names: [(); SE_SLOT_COUNT].map(|_| None),
            se_type_volumes: [1.0; SOUND_TYPE_COUNT],
            se_muted: [false; SE_SLOT_COUNT],
            se_volumes: [1.0; SE_SLOT_COUNT],
            se_repeat: [false; SE_SLOT_COUNT],
            se_pan: [0.5; SE_SLOT_COUNT],
            se_playing: [false; SE_SLOT_COUNT],
        }
    }

    pub fn load(&mut self, slot: i32, _se: Vec<u8>) -> Result<()> {
        self.se_loaded[slot as usize] = true;
        Ok(())
    }

    pub fn load_named(&mut self, slot: i32, name: impl Into<String>, se: Vec<u8>) -> Result<()> {
        let slot_usize = slot as usize;
        self.se_names[slot_usize] = Some(name.into());
        self.load(slot, se)
    }

    fn type_volume_for_slot(&self, slot: usize) -> f32 {
        match self.se_kinds[slot] {
            Some(kind) if (0..SOUND_TYPE_COUNT as i32).contains(&kind) => {
                self.se_type_volumes[kind as usize]
            }
            _ => 1.0,
        }
    }

    fn effective_volume_for_slot(&self, slot: usize) -> f32 {
        self.se_volumes[slot] * self.type_volume_for_slot(slot)
    }

    pub fn play(
        &mut self,
        slot: i32,
        repeat: bool,
        volume: f32,
        pan: f64,
        _fade_in: Tween,
    ) -> Result<()> {
        let slot = slot as usize;
        self.se_repeat[slot] = repeat;
        self.se_pan[slot] = pan;
        self.se_volumes[slot] = volume;
        if !self.se_loaded[slot] {
            log::error!("Tried to play SE slot {}, but no SE was loaded", slot);
            return Ok(());
        }
        let _ = self.effective_volume_for_slot(slot);
        self.se_playing[slot] = true;
        Ok(())
    }

    pub fn set_volume(&mut self, slot: i32, volume: f32, _tween: Tween) {
        self.se_volumes[slot as usize] = volume;
        let _ = self.effective_volume_for_slot(slot as usize);
    }

    pub fn set_type_volume(&mut self, kind: i32, volume: f32, _tween: Tween) {
        if (0..SOUND_TYPE_COUNT as i32).contains(&kind) {
            self.se_type_volumes[kind as usize] = volume;
        }
    }

    pub fn set_panning(&mut self, slot: i32, pan: f64, _tween: Tween) {
        self.se_pan[slot as usize] = pan;
    }

    pub fn silent_on(&mut self, slot: i32, _tween: Tween) {
        self.se_muted[slot as usize] = true;
    }

    pub fn stop(&mut self, slot: i32, _fade_out: Tween) {
        self.se_playing[slot as usize] = false;
    }

    pub fn stop_all(&mut self, fade_out: Tween) {
        for slot in 0..SE_SLOT_COUNT {
            self.stop(slot as i32, fade_out);
        }
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        self.se_playing[slot as usize]
    }

    pub fn set_type(&mut self, slot: i32, kind: i32) {
        self.se_kinds[slot as usize] = Some(kind);
    }

    pub fn debug_summary(&self) -> SeDebugSummary {
        let loaded_datas = self.se_loaded.iter().filter(|&&x| x).count();
        let playing_slots = self.se_playing.iter().filter(|&&x| x).count();
        let mut slots = Vec::new();
        for slot in 0..SE_SLOT_COUNT {
            let playing = self.se_playing[slot];
            let data_loaded = self.se_loaded[slot];
            let has_name = self.se_names[slot].is_some();
            let has_kind = self.se_kinds[slot].is_some();
            if playing || data_loaded || has_name || has_kind {
                slots.push(SeSlotInfo {
                    slot,
                    name: self.se_names[slot].clone(),
                    volume: self.se_volumes[slot] as f64,
                    muted: self.se_muted[slot],
                    kind: self.se_kinds[slot],
                    data_loaded,
                    playing,
                });
            }
        }
        SeDebugSummary {
            max_datas: SE_SLOT_COUNT,
            max_slots: SE_SLOT_COUNT,
            loaded_datas,
            playing_slots,
            slots,
        }
    }

    pub fn capture_snapshot_v1(&self) -> SePlayerSnapshotV1 {
        let mut slots = Vec::new();
        for i in 0..SE_SLOT_COUNT {
            let has_any = self.se_loaded[i] || self.se_playing[i] || self.se_names[i].is_some();
            if !has_any {
                continue;
            }
            slots.push(SeSlotSnapshotV1 {
                slot: i as u16,
                path: self.se_names[i].clone(),
                sound_type: self.se_kinds[i],
                volume: self.se_volumes[i],
                muted: self.se_muted[i],
                playing: self.se_playing[i],
                repeat: self.se_repeat[i],
                pan: self.se_pan[i] as f32,
            });
        }
        SePlayerSnapshotV1 { version: 1, slots }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &SePlayerSnapshotV1, vfs: &Vfs) -> Result<()> {
        if snap.version != 1 {
            return Err(anyhow!(
                "unsupported SePlayerSnapshotV1 version: {}",
                snap.version
            ));
        }
        for i in 0..SE_SLOT_COUNT {
            self.se_loaded[i] = false;
            self.se_kinds[i] = None;
            self.se_names[i] = None;
            self.se_muted[i] = false;
            self.se_volumes[i] = 100.0;
            self.se_repeat[i] = false;
            self.se_pan[i] = 0.5;
            self.se_playing[i] = false;
        }
        for s in &snap.slots {
            let slot = s.slot as usize;
            if slot >= SE_SLOT_COUNT {
                continue;
            }
            self.se_kinds[slot] = s.sound_type;
            self.se_muted[slot] = s.muted;
            self.se_volumes[slot] = s.volume;
            self.se_repeat[slot] = s.repeat;
            self.se_pan[slot] = s.pan as f64;
            if let Some(path) = s.path.clone() {
                let data = vfs
                    .read_file(&path)
                    .with_context(|| format!("read SE from vfs: {}", path))?;
                self.load_named(slot as i32, path, data)?;
            }
            self.se_playing[slot] = s.playing && self.se_loaded[slot];
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct SeSlotInfo {
    pub slot: usize,
    pub name: Option<String>,
    pub volume: f64,
    pub muted: bool,
    pub kind: Option<i32>,
    pub data_loaded: bool,
    pub playing: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SeDebugSummary {
    pub max_datas: usize,
    pub max_slots: usize,
    pub loaded_datas: usize,
    pub playing_slots: usize,
    pub slots: Vec<SeSlotInfo>,
}

use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::rfvp_audio::{AudioManager, Tween};
use crate::subsystem::resources::vfs::Vfs;

pub const BGM_SLOT_COUNT: usize = 4;
pub const SOUND_TYPE_COUNT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BgmSlotSnapshotV1 {
    pub slot: u8,
    pub path: Option<String>,
    pub sound_type: Option<i32>,
    pub volume: f32,
    pub muted: bool,
    pub playing: bool,
    pub repeat: bool,
    pub pan: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BgmPlayerSnapshotV1 {
    pub version: u16,
    pub slots: Vec<BgmSlotSnapshotV1>,
}

pub struct BgmPlayer {
    _audio_manager: Arc<AudioManager>,
    bgm_loaded: [bool; BGM_SLOT_COUNT],
    bgm_kinds: [Option<i32>; BGM_SLOT_COUNT],
    bgm_names: [Option<String>; BGM_SLOT_COUNT],
    bgm_type_volumes: [f32; SOUND_TYPE_COUNT],
    bgm_muted: [bool; BGM_SLOT_COUNT],
    bgm_volumes: [f32; BGM_SLOT_COUNT],
    bgm_repeat: [bool; BGM_SLOT_COUNT],
    bgm_pan: [f64; BGM_SLOT_COUNT],
    bgm_playing: [bool; BGM_SLOT_COUNT],
}

impl BgmPlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        Self {
            _audio_manager: audio_manager,
            bgm_loaded: [false; BGM_SLOT_COUNT],
            bgm_kinds: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_names: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_type_volumes: [1.0; SOUND_TYPE_COUNT],
            bgm_muted: [false; BGM_SLOT_COUNT],
            bgm_volumes: [1.0; BGM_SLOT_COUNT],
            bgm_repeat: [false; BGM_SLOT_COUNT],
            bgm_pan: [0.5; BGM_SLOT_COUNT],
            bgm_playing: [false; BGM_SLOT_COUNT],
        }
    }

    pub fn load(&mut self, slot: i32, _bgm: Vec<u8>) -> Result<()> {
        let slot = slot as usize;
        self.bgm_loaded[slot] = true;
        self.bgm_names[slot] = None;
        Ok(())
    }

    pub fn load_named(&mut self, slot: i32, name: impl Into<String>, _vfs: &Vfs) -> Result<()> {
        let slot = slot as usize;
        self.bgm_loaded[slot] = true;
        self.bgm_names[slot] = Some(name.into());
        Ok(())
    }

    fn type_volume_for_slot(&self, slot: usize) -> f32 {
        match self.bgm_kinds[slot] {
            Some(kind) if (0..SOUND_TYPE_COUNT as i32).contains(&kind) => {
                self.bgm_type_volumes[kind as usize]
            }
            _ => 1.0,
        }
    }

    fn effective_volume_for_slot(&self, slot: usize) -> f32 {
        self.bgm_volumes[slot] * self.type_volume_for_slot(slot)
    }

    pub fn play(
        &mut self,
        slot: i32,
        repeat: bool,
        volume: f32,
        pan: f64,
        _fade_in: Tween,
        _vfs: &Vfs,
    ) -> Result<()> {
        let slot = slot as usize;
        self.bgm_pan[slot] = pan;
        self.bgm_repeat[slot] = repeat;
        self.bgm_volumes[slot] = volume;
        if !self.bgm_loaded[slot] {
            log::error!("Tried to play BGM slot {}, but no BGM was loaded", slot);
            return Ok(());
        }
        let _ = self.effective_volume_for_slot(slot);
        self.bgm_playing[slot] = true;
        Ok(())
    }

    pub fn set_volume(&mut self, slot: i32, volume: f32, _tween: Tween) {
        let slot = slot as usize;
        self.bgm_volumes[slot] = volume;
        let _ = self.effective_volume_for_slot(slot);
    }

    pub fn silent_on(&mut self, slot: i32, _tween: Tween) {
        self.bgm_muted[slot as usize] = true;
    }

    pub fn stop(&mut self, slot: i32, _fade_out: Tween) {
        self.bgm_playing[slot as usize] = false;
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        self.bgm_playing[slot as usize]
    }

    pub fn get_playing_slots(&self) -> Vec<bool> {
        self.bgm_playing.to_vec()
    }

    pub fn set_type(&mut self, slot: i32, kind: i32) {
        self.bgm_kinds[slot as usize] = Some(kind);
    }

    pub fn set_type_volume(&mut self, kind: i32, volume: f32, _tween: Tween) {
        if (0..SOUND_TYPE_COUNT as i32).contains(&kind) {
            self.bgm_type_volumes[kind as usize] = volume;
        }
    }

    pub fn debug_summary(&self) -> BgmDebugSummary {
        let loaded_datas = self.bgm_loaded.iter().filter(|&&x| x).count();
        let playing_slots = self.bgm_playing.iter().filter(|&&x| x).count();
        let mut slots = Vec::new();
        for slot in 0..BGM_SLOT_COUNT {
            let playing = self.bgm_playing[slot];
            let data_loaded = self.bgm_loaded[slot];
            let has_name = self.bgm_names[slot].is_some();
            let has_kind = self.bgm_kinds[slot].is_some();
            if playing || data_loaded || has_name || has_kind {
                slots.push(BgmSlotInfo {
                    slot,
                    name: self.bgm_names[slot].clone(),
                    volume: self.bgm_volumes[slot] as f64,
                    muted: self.bgm_muted[slot],
                    kind: self.bgm_kinds[slot],
                    data_loaded,
                    playing,
                });
            }
        }
        BgmDebugSummary {
            max_slots: BGM_SLOT_COUNT,
            loaded_datas,
            playing_slots,
            slots,
        }
    }

    pub fn capture_snapshot_v1(&self) -> BgmPlayerSnapshotV1 {
        let mut slots = Vec::new();
        for i in 0..BGM_SLOT_COUNT {
            let has_any = self.bgm_loaded[i] || self.bgm_playing[i] || self.bgm_names[i].is_some();
            if !has_any {
                continue;
            }
            slots.push(BgmSlotSnapshotV1 {
                slot: i as u8,
                path: self.bgm_names[i].clone(),
                sound_type: self.bgm_kinds[i],
                volume: self.bgm_volumes[i],
                muted: self.bgm_muted[i],
                playing: self.bgm_playing[i],
                repeat: self.bgm_repeat[i],
                pan: self.bgm_pan[i] as f32,
            });
        }
        BgmPlayerSnapshotV1 { version: 1, slots }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &BgmPlayerSnapshotV1, vfs: &Vfs) -> Result<()> {
        if snap.version != 1 {
            return Err(anyhow!(
                "unsupported BgmPlayerSnapshotV1 version: {}",
                snap.version
            ));
        }
        for i in 0..BGM_SLOT_COUNT {
            self.bgm_loaded[i] = false;
            self.bgm_kinds[i] = None;
            self.bgm_names[i] = None;
            self.bgm_muted[i] = false;
            self.bgm_volumes[i] = 100.0;
            self.bgm_repeat[i] = false;
            self.bgm_pan[i] = 0.5;
            self.bgm_playing[i] = false;
        }
        for s in &snap.slots {
            let slot = s.slot as usize;
            if slot >= BGM_SLOT_COUNT {
                continue;
            }
            self.bgm_kinds[slot] = s.sound_type;
            self.bgm_muted[slot] = s.muted;
            self.bgm_volumes[slot] = s.volume;
            self.bgm_repeat[slot] = s.repeat;
            self.bgm_pan[slot] = s.pan as f64;
            if let Some(path) = s.path.clone() {
                self.load_named(slot as i32, path, vfs)?;
            }
            self.bgm_playing[slot] = s.playing && self.bgm_loaded[slot];
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct BgmSlotInfo {
    pub slot: usize,
    pub name: Option<String>,
    pub volume: f64,
    pub muted: bool,
    pub kind: Option<i32>,
    pub data_loaded: bool,
    pub playing: bool,
}

#[derive(Debug, Clone, Default)]
pub struct BgmDebugSummary {
    pub max_slots: usize,
    pub loaded_datas: usize,
    pub playing_slots: usize,
    pub slots: Vec<BgmSlotInfo>,
}

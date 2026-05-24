use std::io::Cursor;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use anzu_hal::{AudioSystem, Region, SoundData, SoundHandle};
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

fn anzu_tween(t: Tween) -> anzu_hal::Tween {
    anzu_hal::Tween::ms(t.duration.as_millis() as u32)
}

pub struct BgmPlayer {
    audio_system: Arc<AudioSystem>,
    bgm_datas: [Option<SoundData>; BGM_SLOT_COUNT],
    bgm_slots: [Option<SoundHandle>; BGM_SLOT_COUNT],
    bgm_kinds: [Option<i32>; BGM_SLOT_COUNT],
    bgm_names: [Option<String>; BGM_SLOT_COUNT],
    bgm_type_volumes: [f32; SOUND_TYPE_COUNT],
    bgm_muted: [bool; BGM_SLOT_COUNT],
    bgm_volumes: [f32; BGM_SLOT_COUNT],
    bgm_repeat: [bool; BGM_SLOT_COUNT],
    bgm_pan: [f64; BGM_SLOT_COUNT],
}

impl BgmPlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        let audio_system = audio_manager.anzu_system();
        // Allocate a dummy sub-track for API compatibility; anzu routing is flat.
        let _ = audio_system.add_sub_track(anzu_hal::TrackBuilder::new());
        for _ in 0..BGM_SLOT_COUNT {
            let _ = audio_system.add_sub_track(anzu_hal::TrackBuilder::new());
        }
        Self {
            audio_system,
            bgm_datas: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_slots: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_kinds: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_names: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_type_volumes: [1.0; SOUND_TYPE_COUNT],
            bgm_muted: [false; BGM_SLOT_COUNT],
            bgm_volumes: [1.0; BGM_SLOT_COUNT],
            bgm_repeat: [false; BGM_SLOT_COUNT],
            bgm_pan: [0.5; BGM_SLOT_COUNT],
        }
    }

    pub fn load(&mut self, slot: i32, bgm: Vec<u8>) -> Result<()> {
        let slot = slot as usize;
        let data = SoundData::from_bytes(&bgm)
            .with_context(|| format!("decode BGM for slot {}", slot))?;
        self.bgm_datas[slot] = Some(data);
        self.bgm_names[slot] = None;
        Ok(())
    }

    pub fn load_named(&mut self, slot: i32, name: impl Into<String>, vfs: &Vfs) -> Result<()> {
        let slot_usize = slot as usize;
        let name = name.into();
        let bytes = vfs
            .read_file(&name)
            .with_context(|| format!("read BGM from vfs: {}", name))?;
        let data = SoundData::from_bytes(&bytes)
            .with_context(|| format!("decode BGM from vfs: {}", name))?;
        self.bgm_names[slot_usize] = Some(name);
        self.bgm_datas[slot_usize] = Some(data);
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
        fade_in: Tween,
        _vfs: &Vfs,
    ) -> Result<()> {
        let slot = slot as usize;
        self.bgm_pan[slot] = pan;
        self.bgm_repeat[slot] = repeat;
        self.bgm_volumes[slot] = volume;

        let actual_volume = if self.bgm_muted[slot] {
            0.0
        } else {
            self.effective_volume_for_slot(slot)
        };

        let data = match &self.bgm_datas[slot] {
            Some(d) => d,
            None => {
                log::error!("Tried to play BGM slot {}, but no BGM was loaded", slot);
                return Ok(());
            }
        };

        let loop_region = repeat.then_some(Region::full());
        let handle = self.audio_system.play(
            data,
            actual_volume,
            pan as f32,
            repeat,
            loop_region,
            anzu_tween(fade_in),
        );

        if let Some(mut old_handle) = self.bgm_slots[slot].take() {
            old_handle.stop(anzu_tween(fade_in));
        }
        self.bgm_slots[slot] = handle;

        Ok(())
    }

    pub fn set_volume(&mut self, slot: i32, volume: f32, tween: Tween) {
        let slot = slot as usize;
        self.bgm_volumes[slot] = volume;
        let actual_volume = if self.bgm_muted[slot] {
            0.0
        } else {
            self.effective_volume_for_slot(slot)
        };
        if let Some(handle) = self.bgm_slots[slot].as_mut() {
            handle.set_volume(actual_volume as f64, anzu_tween(tween));
        } else {
            log::warn!("Tried to set volume of BGM slot {}, but there was no BGM playing", slot);
        }
    }

    pub fn silent_on(&mut self, slot: i32, tween: Tween) {
        let slot = slot as usize;
        self.bgm_muted[slot] = true;
        if let Some(handle) = self.bgm_slots[slot].as_mut() {
            handle.set_volume(0.0, anzu_tween(tween));
        }
    }

    pub fn stop(&mut self, slot: i32, fade_out: Tween) {
        let slot = slot as usize;
        if let Some(mut bgm) = self.bgm_slots[slot].take() {
            bgm.stop(anzu_tween(fade_out));
        } else {
            log::warn!("Tried to stop a BGM that was not playing (slot {})", slot);
        }
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        let slot = slot as usize;
        self.bgm_slots[slot]
            .as_ref()
            .map(|h| h.state().is_advancing())
            .unwrap_or(false)
    }

    pub fn get_playing_slots(&self) -> Vec<bool> {
        (0..BGM_SLOT_COUNT)
            .map(|i| {
                self.bgm_slots[i]
                    .as_ref()
                    .map(|h| h.state().is_advancing())
                    .unwrap_or(false)
            })
            .collect()
    }

    pub fn set_type(&mut self, slot: i32, kind: i32) {
        let slot = slot as usize;
        self.bgm_kinds[slot] = Some(kind);
        let actual_volume = if self.bgm_muted[slot] {
            0.0
        } else {
            self.effective_volume_for_slot(slot)
        };
        if let Some(handle) = self.bgm_slots[slot].as_mut() {
            handle.set_volume(actual_volume as f64, anzu_hal::Tween::IMMEDIATE);
        }
    }

    pub fn set_type_volume(&mut self, kind: i32, volume: f32, tween: Tween) {
        if !(0..SOUND_TYPE_COUNT as i32).contains(&kind) {
            return;
        }
        self.bgm_type_volumes[kind as usize] = volume;
        for slot in 0..BGM_SLOT_COUNT {
            if self.bgm_kinds[slot] == Some(kind) {
                let actual_volume = if self.bgm_muted[slot] {
                    0.0
                } else {
                    self.effective_volume_for_slot(slot)
                };
                if let Some(handle) = self.bgm_slots[slot].as_mut() {
                    handle.set_volume(actual_volume as f64, anzu_tween(tween));
                }
            }
        }
    }

    pub fn debug_summary(&self) -> BgmDebugSummary {
        let loaded_datas = self.bgm_datas.iter().filter(|x| x.is_some()).count();
        let playing_slots = (0..BGM_SLOT_COUNT)
            .filter(|&i| {
                self.bgm_slots[i]
                    .as_ref()
                    .map(|h| h.state().is_advancing())
                    .unwrap_or(false)
            })
            .count();
        let mut slots = Vec::new();
        for slot in 0..BGM_SLOT_COUNT {
            let playing = self.bgm_slots[slot]
                .as_ref()
                .map(|h| h.state().is_advancing())
                .unwrap_or(false);
            let data_loaded = self.bgm_datas[slot].is_some();
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
        BgmDebugSummary { max_slots: BGM_SLOT_COUNT, loaded_datas, playing_slots, slots }
    }

    pub fn capture_snapshot_v1(&self) -> BgmPlayerSnapshotV1 {
        let mut slots = Vec::new();
        for i in 0..BGM_SLOT_COUNT {
            let data_loaded = self.bgm_datas[i].is_some();
            let has_any = data_loaded || self.bgm_slots[i].is_some() || self.bgm_names[i].is_some();
            if !has_any {
                continue;
            }
            slots.push(BgmSlotSnapshotV1 {
                slot: i as u8,
                path: self.bgm_names[i].clone(),
                sound_type: self.bgm_kinds[i],
                volume: self.bgm_volumes[i],
                muted: self.bgm_muted[i],
                playing: self.bgm_slots[i]
                    .as_ref()
                    .map(|h| h.state().is_advancing())
                    .unwrap_or(false),
                repeat: self.bgm_repeat[i],
                pan: self.bgm_pan[i] as f32,
            });
        }
        BgmPlayerSnapshotV1 { version: 1, slots }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &BgmPlayerSnapshotV1, vfs: &Vfs) -> Result<()> {
        if snap.version != 1 {
            return Err(anyhow!("unsupported BgmPlayerSnapshotV1 version: {}", snap.version));
        }
        for i in 0..BGM_SLOT_COUNT {
            self.stop(i as i32, Tween::default());
            self.bgm_datas[i] = None;
            self.bgm_kinds[i] = None;
            self.bgm_names[i] = None;
            self.bgm_muted[i] = false;
            self.bgm_volumes[i] = 100.0;
            self.bgm_repeat[i] = false;
            self.bgm_pan[i] = 0.5;
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
            if s.playing && self.bgm_datas[slot].is_some() {
                self.play(
                    slot as i32,
                    self.bgm_repeat[slot],
                    self.bgm_volumes[slot],
                    self.bgm_pan[slot],
                    Tween::default(),
                    vfs,
                )?;
                if self.bgm_muted[slot] {
                    self.silent_on(slot as i32, Tween::default());
                }
            }
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

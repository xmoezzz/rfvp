use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use anzu_hal::{AudioSystem, Region, SoundData, SoundHandle};
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

fn anzu_tween(t: Tween) -> anzu_hal::Tween {
    anzu_hal::Tween::ms(t.duration.as_millis() as u32)
}

pub struct SePlayer {
    audio_system: Arc<AudioSystem>,
    se_datas: [Option<SoundData>; SE_SLOT_COUNT],
    se_slots: [Option<SoundHandle>; SE_SLOT_COUNT],
    se_kinds: [Option<i32>; SE_SLOT_COUNT],
    se_names: [Option<String>; SE_SLOT_COUNT],
    se_type_volumes: [f32; SOUND_TYPE_COUNT],
    se_muted: [bool; SE_SLOT_COUNT],
    se_volumes: [f32; SE_SLOT_COUNT],
    se_repeat: [bool; SE_SLOT_COUNT],
    se_pan: [f64; SE_SLOT_COUNT],
}

impl SePlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        let audio_system = audio_manager.anzu_system();
        Self {
            audio_system,
            se_datas: [(); SE_SLOT_COUNT].map(|_| None),
            se_slots: [(); SE_SLOT_COUNT].map(|_| None),
            se_kinds: [(); SE_SLOT_COUNT].map(|_| None),
            se_names: [(); SE_SLOT_COUNT].map(|_| None),
            se_type_volumes: [1.0; SOUND_TYPE_COUNT],
            se_muted: [false; SE_SLOT_COUNT],
            se_volumes: [1.0; SE_SLOT_COUNT],
            se_repeat: [false; SE_SLOT_COUNT],
            se_pan: [0.5; SE_SLOT_COUNT],
        }
    }

    pub fn load(&mut self, slot: i32, se: Vec<u8>) -> Result<()> {
        let slot = slot as usize;
        let data =
            SoundData::from_bytes(&se).with_context(|| format!("decode SE for slot {}", slot))?;
        self.se_datas[slot] = Some(data);
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
        fade_in: Tween,
    ) -> Result<()> {
        let slot = slot as usize;
        self.se_repeat[slot] = repeat;
        self.se_pan[slot] = pan;
        self.se_volumes[slot] = volume;

        let actual_volume = if self.se_muted[slot] {
            0.0
        } else {
            self.effective_volume_for_slot(slot)
        };

        let data = match &self.se_datas[slot] {
            Some(d) => d,
            None => {
                log::error!("Tried to play SE slot {}, but no SE was loaded", slot);
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

        if let Some(mut old_handle) = self.se_slots[slot].take() {
            old_handle.stop(anzu_hal::Tween::IMMEDIATE);
        }
        self.se_slots[slot] = handle;

        Ok(())
    }

    pub fn set_volume(&mut self, slot: i32, volume: f32, tween: Tween) {
        let slot = slot as usize;
        self.se_volumes[slot] = volume;
        let actual_volume = if self.se_muted[slot] {
            0.0
        } else {
            self.effective_volume_for_slot(slot)
        };
        if let Some(handle) = self.se_slots[slot].as_mut() {
            handle.set_volume(actual_volume as f64, anzu_tween(tween));
        }
    }

    pub fn set_type_volume(&mut self, kind: i32, volume: f32, tween: Tween) {
        if !(0..SOUND_TYPE_COUNT as i32).contains(&kind) {
            return;
        }
        self.se_type_volumes[kind as usize] = volume;
        for slot in 0..SE_SLOT_COUNT {
            if self.se_kinds[slot] == Some(kind) {
                let actual_volume = if self.se_muted[slot] {
                    0.0
                } else {
                    self.effective_volume_for_slot(slot)
                };
                if let Some(handle) = self.se_slots[slot].as_mut() {
                    handle.set_volume(actual_volume as f64, anzu_tween(tween));
                }
            }
        }
    }

    pub fn set_panning(&mut self, slot: i32, pan: f64, tween: Tween) {
        let slot = slot as usize;
        self.se_pan[slot] = pan;
        if let Some(handle) = self.se_slots[slot].as_mut() {
            handle.set_panning(anzu_hal::Panning::from(pan as f32), anzu_tween(tween));
        }
    }

    pub fn silent_on(&mut self, slot: i32, tween: Tween) {
        let slot = slot as usize;
        self.se_muted[slot] = true;
        if let Some(handle) = self.se_slots[slot].as_mut() {
            handle.set_volume(0.0, anzu_tween(tween));
        }
    }

    pub fn stop(&mut self, slot: i32, fade_out: Tween) {
        let slot = slot as usize;
        if let Some(mut h) = self.se_slots[slot].take() {
            h.stop(anzu_tween(fade_out));
        }
    }

    pub fn stop_all(&mut self, fade_out: Tween) {
        for slot in 0..SE_SLOT_COUNT {
            self.stop(slot as i32, fade_out);
        }
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        self.se_slots[slot as usize]
            .as_ref()
            .map(|h| h.state().is_advancing())
            .unwrap_or(false)
    }

    pub fn set_type(&mut self, slot: i32, kind: i32) {
        let slot = slot as usize;
        self.se_kinds[slot] = Some(kind);
        let actual_volume = if self.se_muted[slot] {
            0.0
        } else {
            self.effective_volume_for_slot(slot)
        };
        if let Some(handle) = self.se_slots[slot].as_mut() {
            handle.set_volume(actual_volume as f64, anzu_hal::Tween::IMMEDIATE);
        }
    }

    pub fn debug_summary(&self) -> SeDebugSummary {
        let loaded_datas = self.se_datas.iter().filter(|x| x.is_some()).count();
        let playing_slots = (0..SE_SLOT_COUNT)
            .filter(|&i| {
                self.se_slots[i]
                    .as_ref()
                    .map(|h| h.state().is_advancing())
                    .unwrap_or(false)
            })
            .count();
        let mut slots = Vec::new();
        for slot in 0..SE_SLOT_COUNT {
            let playing = self.se_slots[slot]
                .as_ref()
                .map(|h| h.state().is_advancing())
                .unwrap_or(false);
            let data_loaded = self.se_datas[slot].is_some();
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
            let has_any = self.se_datas[i].is_some()
                || self.se_slots[i].is_some()
                || self.se_names[i].is_some();
            if !has_any {
                continue;
            }
            slots.push(SeSlotSnapshotV1 {
                slot: i as u16,
                path: self.se_names[i].clone(),
                sound_type: self.se_kinds[i],
                volume: self.se_volumes[i],
                muted: self.se_muted[i],
                playing: self.se_slots[i]
                    .as_ref()
                    .map(|h| h.state().is_advancing())
                    .unwrap_or(false),
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
            self.stop(i as i32, Tween::default());
            self.se_datas[i] = None;
            self.se_kinds[i] = None;
            self.se_names[i] = None;
            self.se_muted[i] = false;
            self.se_volumes[i] = 100.0;
            self.se_repeat[i] = false;
            self.se_pan[i] = 0.5;
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
            if s.playing && self.se_datas[slot].is_some() {
                self.play(
                    slot as i32,
                    self.se_repeat[slot],
                    self.se_volumes[slot],
                    self.se_pan[slot],
                    Tween::default(),
                )?;
            }
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

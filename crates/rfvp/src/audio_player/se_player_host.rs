use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::bgm_player::{encoded_kind_from_path, SOUND_TYPE_COUNT};
use crate::host_api::{AudioParams, AudioStreamId};
use crate::rfvp_audio::{AudioCommand, AudioManager, Tween};
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
    audio_manager: Arc<AudioManager>,
    loaded: [bool; SE_SLOT_COUNT],
    names: [Option<String>; SE_SLOT_COUNT],
    kinds: [Option<i32>; SE_SLOT_COUNT],
    type_volumes: [f32; SOUND_TYPE_COUNT],
    muted: [bool; SE_SLOT_COUNT],
    volumes: [f32; SE_SLOT_COUNT],
    repeat: [bool; SE_SLOT_COUNT],
    pan: [f64; SE_SLOT_COUNT],
    playing: [bool; SE_SLOT_COUNT],
}

impl SePlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        Self {
            audio_manager,
            loaded: [false; SE_SLOT_COUNT],
            names: [(); SE_SLOT_COUNT].map(|_| None),
            kinds: [(); SE_SLOT_COUNT].map(|_| None),
            type_volumes: [1.0; SOUND_TYPE_COUNT],
            muted: [false; SE_SLOT_COUNT],
            volumes: [1.0; SE_SLOT_COUNT],
            repeat: [false; SE_SLOT_COUNT],
            pan: [0.5; SE_SLOT_COUNT],
            playing: [false; SE_SLOT_COUNT],
        }
    }

    fn id(slot: usize) -> AudioStreamId {
        AudioStreamId::se(slot)
    }

    pub fn load(&mut self, slot: i32, se: Vec<u8>) -> Result<()> {
        let slot = checked_slot(slot)?;
        self.audio_manager.push_command(AudioCommand::LoadEncoded {
            id: Self::id(slot),
            kind: crate::host_api::EncodedAudioKind::Unknown,
            bytes: se,
        });
        self.loaded[slot] = true;
        Ok(())
    }

    pub fn load_named(&mut self, slot: i32, name: impl Into<String>, se: Vec<u8>) -> Result<()> {
        let name = name.into();
        let slot = checked_slot(slot)?;
        self.audio_manager.push_command(AudioCommand::LoadEncoded {
            id: Self::id(slot),
            kind: encoded_kind_from_path(&name),
            bytes: se,
        });
        self.loaded[slot] = true;
        self.names[slot] = Some(name);
        Ok(())
    }

    pub fn play(
        &mut self,
        slot: i32,
        repeat: bool,
        volume: f32,
        pan: f64,
        fade_in: Tween,
    ) -> Result<()> {
        let slot = checked_slot(slot)?;
        if !self.loaded[slot] {
            return Err(anyhow!("SE slot {slot} is not loaded"));
        }
        self.repeat[slot] = repeat;
        self.volumes[slot] = volume;
        self.pan[slot] = pan;
        self.playing[slot] = true;
        self.audio_manager.push_command(AudioCommand::Play {
            id: Self::id(slot),
            params: AudioParams {
                volume: self.effective_volume_for_slot(slot),
                pan: pan as f32,
                repeat,
            },
            fade_in_ms: duration_ms_u32(fade_in.duration),
        });
        Ok(())
    }

    pub fn set_volume(&mut self, slot: i32, volume: f32, _tween: Tween) {
        if let Ok(slot) = checked_slot(slot) {
            self.volumes[slot] = volume;
            self.audio_manager.push_command(AudioCommand::SetParams {
                id: Self::id(slot),
                params: AudioParams {
                    volume: self.effective_volume_for_slot(slot),
                    pan: self.pan[slot] as f32,
                    repeat: self.repeat[slot],
                },
            });
        }
    }

    pub fn set_panning(&mut self, slot: i32, pan: f64, _tween: Tween) {
        if let Ok(slot) = checked_slot(slot) {
            self.pan[slot] = pan;
            self.set_volume(slot as i32, self.volumes[slot], Tween::default());
        }
    }

    pub fn set_type_volume(&mut self, kind: i32, volume: f32, _tween: Tween) {
        if (0..SOUND_TYPE_COUNT as i32).contains(&kind) {
            self.type_volumes[kind as usize] = volume;
        }
    }

    pub fn silent_on(&mut self, slot: i32, _tween: Tween) {
        if let Ok(slot) = checked_slot(slot) {
            self.muted[slot] = true;
            self.set_volume(slot as i32, self.volumes[slot], Tween::default());
        }
    }

    pub fn stop(&mut self, slot: i32, fade_out: Tween) {
        if let Ok(slot) = checked_slot(slot) {
            self.playing[slot] = false;
            self.audio_manager.push_command(AudioCommand::Stop {
                id: Self::id(slot),
                fade_ms: duration_ms_u32(fade_out.duration),
            });
        }
    }

    pub fn stop_all(&mut self, fade_out: Tween) {
        for slot in 0..SE_SLOT_COUNT {
            self.stop(slot as i32, fade_out);
        }
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        checked_slot(slot)
            .ok()
            .map(|slot| self.playing[slot])
            .unwrap_or(false)
    }

    pub fn set_type(&mut self, slot: i32, kind: i32) {
        if let Ok(slot) = checked_slot(slot) {
            self.kinds[slot] = Some(kind);
        }
    }

    fn effective_volume_for_slot(&self, slot: usize) -> f32 {
        let type_volume = self.kinds[slot]
            .filter(|kind| (0..SOUND_TYPE_COUNT as i32).contains(kind))
            .map(|kind| self.type_volumes[kind as usize])
            .unwrap_or(1.0);
        if self.muted[slot] {
            0.0
        } else {
            self.volumes[slot] * type_volume
        }
    }

    pub fn capture_snapshot_v1(&self) -> SePlayerSnapshotV1 {
        let mut slots = Vec::new();
        for slot in 0..SE_SLOT_COUNT {
            if self.loaded[slot] || self.playing[slot] || self.names[slot].is_some() {
                slots.push(SeSlotSnapshotV1 {
                    slot: slot as u16,
                    path: self.names[slot].clone(),
                    sound_type: self.kinds[slot],
                    volume: self.volumes[slot],
                    muted: self.muted[slot],
                    playing: self.playing[slot],
                    repeat: self.repeat[slot],
                    pan: self.pan[slot] as f32,
                });
            }
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
        for slot in &snap.slots {
            let i = slot.slot as usize;
            if i >= SE_SLOT_COUNT {
                continue;
            }
            self.kinds[i] = slot.sound_type;
            self.volumes[i] = slot.volume;
            self.muted[i] = slot.muted;
            self.repeat[i] = slot.repeat;
            self.pan[i] = slot.pan as f64;
            if let Some(path) = slot.path.as_ref() {
                let bytes = vfs.read_file(path)?;
                self.load_named(i as i32, path.clone(), bytes)?;
            }
            if slot.playing {
                self.play(
                    i as i32,
                    slot.repeat,
                    slot.volume,
                    slot.pan as f64,
                    Tween::default(),
                )?;
            }
        }
        Ok(())
    }

    pub fn debug_summary(&self) -> SeDebugSummary {
        SeDebugSummary
    }
}

pub struct SeDebugSummary;
pub struct SeSlotInfo;

fn checked_slot(slot: i32) -> Result<usize> {
    let slot = usize::try_from(slot).map_err(|_| anyhow!("invalid SE slot {slot}"))?;
    if slot >= SE_SLOT_COUNT {
        return Err(anyhow!("invalid SE slot {slot}"));
    }
    Ok(slot)
}

fn duration_ms_u32(duration: crate::platform_time::Duration) -> u32 {
    duration.as_millis().min(u128::from(u32::MAX)) as u32
}

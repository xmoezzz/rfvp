use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::host_api::{AudioParams, AudioStreamId, EncodedAudioKind};
use crate::rfvp_audio::{AudioCommand, AudioManager, Tween};
use crate::subsystem::resources::vfs::Vfs;

pub const BGM_SLOT_COUNT: usize = 256;
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
    audio_manager: Arc<AudioManager>,
    loaded: [bool; BGM_SLOT_COUNT],
    names: [Option<String>; BGM_SLOT_COUNT],
    kinds: [Option<i32>; BGM_SLOT_COUNT],
    type_volumes: [f32; SOUND_TYPE_COUNT],
    muted: [bool; BGM_SLOT_COUNT],
    volumes: [f32; BGM_SLOT_COUNT],
    repeat: [bool; BGM_SLOT_COUNT],
    pan: [f64; BGM_SLOT_COUNT],
    playing: [bool; BGM_SLOT_COUNT],
}

impl BgmPlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        Self {
            audio_manager,
            loaded: [false; BGM_SLOT_COUNT],
            names: [(); BGM_SLOT_COUNT].map(|_| None),
            kinds: [(); BGM_SLOT_COUNT].map(|_| None),
            type_volumes: [1.0; SOUND_TYPE_COUNT],
            muted: [false; BGM_SLOT_COUNT],
            volumes: [1.0; BGM_SLOT_COUNT],
            repeat: [false; BGM_SLOT_COUNT],
            pan: [0.5; BGM_SLOT_COUNT],
            playing: [false; BGM_SLOT_COUNT],
        }
    }

    fn id(slot: usize) -> AudioStreamId {
        AudioStreamId::bgm(slot)
    }

    pub fn load(&mut self, slot: i32, bgm: Vec<u8>) -> Result<()> {
        let slot = checked_slot(slot)?;
        self.audio_manager.push_command(AudioCommand::LoadEncoded {
            id: Self::id(slot),
            kind: EncodedAudioKind::Unknown,
            bytes: bgm,
        });
        self.loaded[slot] = true;
        self.names[slot] = None;
        Ok(())
    }

    pub fn load_named(&mut self, slot: i32, name: impl Into<String>, vfs: &Vfs) -> Result<()> {
        let name = name.into();
        let bytes = vfs.read_file(&name)?;
        let slot = checked_slot(slot)?;
        self.audio_manager.push_command(AudioCommand::LoadEncoded {
            id: Self::id(slot),
            kind: encoded_kind_from_path(&name),
            bytes,
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
        _vfs: &Vfs,
    ) -> Result<()> {
        let slot = checked_slot(slot)?;
        ensure_loaded(self.loaded[slot], "BGM", slot)?;
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

    pub fn is_playing(&self, slot: i32) -> bool {
        checked_slot(slot)
            .ok()
            .map(|slot| self.playing[slot])
            .unwrap_or(false)
    }

    pub fn get_playing_slots(&self) -> Vec<bool> {
        self.playing.to_vec()
    }

    pub fn set_type(&mut self, slot: i32, kind: i32) {
        if let Ok(slot) = checked_slot(slot) {
            self.kinds[slot] = Some(kind);
        }
    }

    pub fn set_type_volume(&mut self, kind: i32, volume: f32, _tween: Tween) {
        if (0..SOUND_TYPE_COUNT as i32).contains(&kind) {
            self.type_volumes[kind as usize] = volume;
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

    pub fn capture_snapshot_v1(&self) -> BgmPlayerSnapshotV1 {
        let mut slots = Vec::new();
        for slot in 0..BGM_SLOT_COUNT {
            if self.loaded[slot] || self.playing[slot] || self.names[slot].is_some() {
                slots.push(BgmSlotSnapshotV1 {
                    slot: slot as u8,
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
        BgmPlayerSnapshotV1 { version: 1, slots }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &BgmPlayerSnapshotV1, vfs: &Vfs) -> Result<()> {
        if snap.version != 1 {
            return Err(anyhow!(
                "unsupported BgmPlayerSnapshotV1 version: {}",
                snap.version
            ));
        }
        for slot in &snap.slots {
            let i = slot.slot as usize;
            if i >= BGM_SLOT_COUNT {
                continue;
            }
            self.kinds[i] = slot.sound_type;
            self.volumes[i] = slot.volume;
            self.muted[i] = slot.muted;
            self.repeat[i] = slot.repeat;
            self.pan[i] = slot.pan as f64;
            if let Some(path) = slot.path.as_ref() {
                self.load_named(i as i32, path.clone(), vfs)?;
            }
            if slot.playing {
                self.play(
                    i as i32,
                    slot.repeat,
                    slot.volume,
                    slot.pan as f64,
                    Tween::default(),
                    vfs,
                )?;
            }
        }
        Ok(())
    }
}

pub struct BgmDebugSummary;
pub struct BgmSlotInfo;

impl BgmPlayer {
    pub fn debug_summary(&self) -> BgmDebugSummary {
        BgmDebugSummary
    }
}

fn checked_slot(slot: i32) -> Result<usize> {
    let slot = usize::try_from(slot).map_err(|_| anyhow!("invalid BGM slot {slot}"))?;
    if slot >= BGM_SLOT_COUNT {
        return Err(anyhow!("invalid BGM slot {slot}"));
    }
    Ok(slot)
}

fn ensure_loaded(loaded: bool, label: &str, slot: usize) -> Result<()> {
    if loaded {
        Ok(())
    } else {
        Err(anyhow!("{label} slot {slot} is not loaded"))
    }
}

pub fn encoded_kind_from_path(path: &str) -> EncodedAudioKind {
    let lower = path.to_ascii_lowercase();
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

fn duration_ms_u32(duration: crate::platform_time::Duration) -> u32 {
    duration.as_millis().min(u128::from(u32::MAX)) as u32
}

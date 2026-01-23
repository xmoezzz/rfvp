use std::sync::Arc;
use anyhow::{anyhow, Context, Result};

use kira::Tween;
use kira::track::{TrackBuilder, TrackHandle};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use crate::rfvp_audio::AudioManager;
use kira::sound::Region;
use serde::{Deserialize, Serialize};
use tracing::warn;

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
    se_tracks: [TrackHandle; SE_SLOT_COUNT],
    se_slots: [Option<StaticSoundHandle>; SE_SLOT_COUNT],
    se_datas: [Option<StaticSoundData>; SE_SLOT_COUNT],
    se_kinds: [Option<i32>; SE_SLOT_COUNT],
    se_names: [Option<String>; SE_SLOT_COUNT],
    se_muted: [bool; SE_SLOT_COUNT],
    se_volumes: [f32; SE_SLOT_COUNT],

    se_repeat: [bool; SE_SLOT_COUNT],
    se_pan: [f64; SE_SLOT_COUNT],
}

impl SePlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        let mut manager = audio_manager.kira_manager().lock().unwrap();

        let se_tracks = [(); SE_SLOT_COUNT].map(|_| {
            manager
                .add_sub_track(TrackBuilder::new())
                .expect("Failed to create se track")
        });

        drop(manager);

        Self {
            audio_manager,
            se_tracks,
            se_slots: [(); SE_SLOT_COUNT].map(|_| None),
            se_datas: [(); SE_SLOT_COUNT].map(|_| None),
            se_kinds: [(); SE_SLOT_COUNT].map(|_| None),
            se_names: [(); SE_SLOT_COUNT].map(|_| None),
            se_muted: [false; SE_SLOT_COUNT],
            se_volumes: [1.0; SE_SLOT_COUNT],
            se_repeat: [false; SE_SLOT_COUNT],
            se_pan: [0.5; SE_SLOT_COUNT],
        }
    }

    pub fn load(&mut self, slot: i32, se: Vec<u8>) -> anyhow::Result<()> {
        let slot = slot as usize;
        let cursor = std::io::Cursor::new(se);
        let sound = StaticSoundData::from_cursor(cursor)?;
        self.se_datas[slot] = Some(sound);
        Ok(())
    }

    pub fn load_named(&mut self, slot: i32, name: impl Into<String>, se: Vec<u8>) -> anyhow::Result<()> {
        let slot_usize = slot as usize;
        self.se_names[slot_usize] = Some(name.into());
        self.load(slot, se)
    }

    pub fn play(
        &mut self,
        slot: i32,
        repeat: bool,
        volume: f32,
        pan: f64,
        fade_in: kira::Tween,
    ) -> anyhow::Result<()> {
        let slot = slot as usize;

        self.se_repeat[slot] = repeat;
        self.se_pan[slot] = pan;
        self.se_volumes[slot] = volume;
        let actual_volume = if self.se_muted[slot] { 0.0 } else { volume };let bgm = match &self.se_datas[slot] {
            Some(data) => data.clone(),
            None => {
                log::error!("Tried to play BGM slot {}, but no BGM was loaded", slot);
                return Ok(());
            }
        };

        log::info!("Playing SE slot {}", slot);

        let loop_region = repeat.then_some(Region::default());
        let pan = kira::Panning::from(pan as f32);
        let settings = StaticSoundSettings::new()
            .panning(pan)
            .volume(actual_volume)
            .fade_in_tween(fade_in)
            .loop_region(loop_region)
            .playback_rate(1.0);

        let bgm = bgm.with_settings(settings);
        let handle = self.audio_manager.play(bgm);

        if let Some(mut old_handle) = self.se_slots[slot].take() {
            old_handle.stop(fade_in);
        }

        self.se_slots[slot] = Some(handle);
        Ok(())
    }

    pub fn set_volume(&mut self, slot: i32, volume: f32, tween: kira::Tween) {
        let slot = slot as usize;

        
        self.se_volumes[slot] = volume;
        let actual_volume = if self.se_muted[slot] { 0.0 } else { volume };if let Some(handle) = self.se_slots[slot].as_mut() {
            handle.set_volume(actual_volume, tween);
        } else {
            warn!(
                "Tried to set volume of se slot {}, but there was no se playing",
                slot
            );
        }
    }

    pub fn set_type_volume(&mut self, kind: i32, volume: f32, tween: kira::Tween) {
        for slot in 0..SE_SLOT_COUNT {
            if self.se_kinds[slot] == Some(kind) {
                self.set_volume(slot as i32, volume, tween);
            }
        }
    }

    pub fn set_panning(&mut self, slot: i32, pan: f64, tween: kira::Tween) {
        let slot = slot as usize;

        self.se_pan[slot] = pan;

        let pan = kira::Panning::from(pan as f32);
        if let Some(handle) = self.se_slots[slot].as_mut() {
            handle.set_panning(pan, tween);
        } else {
            warn!(
                "Tried to set pan of se slot {}, but there was no se playing",
                slot
            );
        }
    }

    /// Permanently silence a slot. This mirrors the engine's `SoundSilentOn` semantics.
    pub fn silent_on(&mut self, slot: i32, tween: kira::Tween) {
        let slot = slot as usize;
        self.se_muted[slot] = true;

        if let Some(handle) = self.se_slots[slot].as_mut() {
            handle.set_volume(0.0, tween);
        }
    }

    pub fn stop(&mut self, slot: i32, fade_out: kira::Tween) {
        let slot = slot as usize;

        if let Some(mut se) = self.se_slots[slot].take() {
            se.stop(fade_out);
        } else {
            warn!("Tried to stop a SE that was not playing");
        }
    }

    pub fn stop_all(&mut self, fade_out: kira::Tween) {
        for slot in 0..SE_SLOT_COUNT {
            if self.se_slots[slot].is_some() {
                self.stop(slot as i32, fade_out);
            }
        }
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        let slot = slot as usize;
        self.se_slots[slot].is_some()
    }

    pub fn set_type(&mut self, slot: i32, kind: i32) {
        let slot = slot as usize;
        self.se_kinds[slot] = Some(kind);
    }
    pub fn debug_summary(&self) -> SeDebugSummary {
        let loaded_datas = self.se_datas.iter().filter(|x| x.is_some()).count();
        let playing_slots = self.se_slots.iter().filter(|x| x.is_some()).count();

        let mut slots = Vec::new();
        for slot in 0..SE_SLOT_COUNT {
            let playing = self.se_slots[slot].is_some();
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
            max_datas: self.se_datas.len(),
            max_slots: self.se_slots.len(),
            loaded_datas,
            playing_slots,
            slots,
        }
    }

    pub fn capture_snapshot_v1(&self) -> SePlayerSnapshotV1 {
        let mut slots: Vec<SeSlotSnapshotV1> = Vec::new();

        for i in 0..SE_SLOT_COUNT {
            let has_any = self.se_datas[i].is_some() || self.se_slots[i].is_some() || self.se_names[i].is_some();
            if !has_any {
                continue;
            }

            slots.push(SeSlotSnapshotV1 {
                slot: i as u16,
                path: self.se_names[i].clone(),
                sound_type: self.se_kinds[i],
                volume: self.se_volumes[i] as f32,
                muted: self.se_muted[i],
                playing: self.se_slots[i].is_some(),
                repeat: self.se_repeat[i],
                pan: self.se_pan[i] as f32,
            });
        }

        SePlayerSnapshotV1 { version: 1, slots }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &SePlayerSnapshotV1, vfs: &Vfs) -> Result<()> {
        if snap.version != 1 {
            return Err(anyhow!("unsupported SePlayerSnapshotV1 version: {}", snap.version));
        }

        // Stop and clear current state.
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

            if s.playing {
                if self.se_datas[slot].is_some() {
                    self.play(
                        slot as i32,
                        self.se_repeat[slot],
                        self.se_volumes[slot] as f32,
                        self.se_pan[slot],
                        Tween::default(),
                    )?;

                    if self.se_muted[slot] {
                        self.silent_on(slot as i32, Tween::default());
                    }
                }
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
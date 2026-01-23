use std::sync::Arc;
use anyhow::{anyhow, Context, Result};

use kira::track::{TrackBuilder, TrackHandle};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use crate::rfvp_audio::AudioManager;
use kira::sound::Region;
use kira::Tween;
use kira::Panning;
use tracing::warn;

use serde::{Deserialize, Serialize};

use crate::subsystem::resources::vfs::Vfs;

pub const BGM_SLOT_COUNT: usize = 4;

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
    bgm_tracks: [TrackHandle; BGM_SLOT_COUNT],
    bgm_slots: [Option<StaticSoundHandle>; BGM_SLOT_COUNT],
    bgm_datas: [Option<StaticSoundData>; BGM_SLOT_COUNT],
    bgm_kinds: [Option<i32>; BGM_SLOT_COUNT],
    bgm_names: [Option<String>; BGM_SLOT_COUNT],
    bgm_muted: [bool; BGM_SLOT_COUNT],
    bgm_volumes: [f32; BGM_SLOT_COUNT],
    bgm_repeat: [bool; BGM_SLOT_COUNT],
    bgm_pan: [f64; BGM_SLOT_COUNT],
}

impl BgmPlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        let mut manager = audio_manager.kira_manager().lock().unwrap();

        let a = manager
                .add_sub_track(kira::track::TrackBuilder::new())
                .expect("Failed to create bgm track");

        let bgm_tracks = [(); BGM_SLOT_COUNT].map(|_| {
            manager
                .add_sub_track(TrackBuilder::new())
                .expect("Failed to create bgm track")
        });

        drop(manager);

        Self {
            audio_manager,
            bgm_tracks,
            bgm_slots: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_datas: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_kinds: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_names: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_muted: [false; BGM_SLOT_COUNT],
            bgm_volumes: [1.0; BGM_SLOT_COUNT],
            bgm_repeat: [false; BGM_SLOT_COUNT],
            bgm_pan: [0.5; BGM_SLOT_COUNT],
        }
    }

    pub fn load(&mut self, slot: i32, bgm: Vec<u8>) -> anyhow::Result<()> {
        let slot = slot as usize;
        let cursor = std::io::Cursor::new(bgm);
        let sound = StaticSoundData::from_cursor(cursor)?;
        self.bgm_datas[slot] = Some(sound);
        Ok(())
    }

    pub fn load_named(&mut self, slot: i32, name: impl Into<String>, bgm: Vec<u8>) -> anyhow::Result<()> {
        let slot_usize = slot as usize;
        self.bgm_names[slot_usize] = Some(name.into());
        self.load(slot, bgm)
    }

    pub fn play(
        &mut self,
        slot: i32,
        repeat: bool,
        volume: f32,
        pan: f64,
        fade_in: Tween,
    ) -> anyhow::Result<()> {
        let slot = slot as usize;

        self.bgm_pan[slot] = pan;
        self.bgm_repeat[slot] = repeat;
        self.bgm_volumes[slot] = volume;
        let actual_volume = if self.bgm_muted[slot] { 0.0 } else { volume };let bgm = match &self.bgm_datas[slot] {
            Some(data) => data.clone(),
            None => {
                log::error!("Tried to play BGM slot {}, but no BGM was loaded", slot);
                return Ok(());
            }
        };

        log::info!("Playing BGM slot {}", slot);

        let loop_region = repeat.then_some(Region::default());
        let pan = Panning::from(pan as f32);
        let settings = StaticSoundSettings::new()
            .panning(pan)
            .volume(actual_volume)
            .fade_in_tween(fade_in)
            .loop_region(loop_region);

        let bgm = bgm.with_settings(settings);

        let handle = self.audio_manager.play(bgm);

        if let Some(mut old_handle) = self.bgm_slots[slot].take() {
            old_handle.stop(fade_in);
        }

        self.bgm_slots[slot] = Some(handle);
        Ok(())
    }

    pub fn set_volume(&mut self, slot: i32, volume: f32, tween: Tween) {
        let slot = slot as usize;

        
        self.bgm_volumes[slot] = volume;
        let actual_volume = if self.bgm_muted[slot] { 0.0 } else { volume };if let Some(handle) = self.bgm_slots[slot].as_mut() {
            handle.set_volume(actual_volume, tween);
        } else {
            warn!(
                "Tried to set volume of BGM slot {}, but there was no se playing",
                slot
            );
        }
    }

    /// Permanently silence a slot. This mirrors the engine's `AudioSilentOn` semantics.
    pub fn silent_on(&mut self, slot: i32, tween: Tween) {
        let slot = slot as usize;
        self.bgm_muted[slot] = true;

        if let Some(handle) = self.bgm_slots[slot].as_mut() {
            handle.set_volume(0.0, tween);
        }
    }

    pub fn stop(&mut self, slot: i32, fade_out: Tween) {
        let slot = slot as usize;

        if let Some(mut se) = self.bgm_slots[slot].take() {
            se.stop(fade_out);
        } else {
            warn!("Tried to stop a BGM that was not playing");
        }
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        let slot = slot as usize;
        self.bgm_slots[slot].is_some()
    }

    pub fn set_type(&mut self, slot: i32, kind: i32) {
        let slot = slot as usize;
        self.bgm_kinds[slot] = Some(kind);
    }
    pub fn debug_summary(&self) -> BgmDebugSummary {
        let loaded_datas = self.bgm_datas.iter().filter(|x| x.is_some()).count();
        let playing_slots = self.bgm_slots.iter().filter(|x| x.is_some()).count();

        let mut slots = Vec::new();
        for slot in 0..BGM_SLOT_COUNT {
            let playing = self.bgm_slots[slot].is_some();
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

        BgmDebugSummary {
            max_slots: BGM_SLOT_COUNT,
            loaded_datas,
            playing_slots,
            slots,
        }
    }

    pub fn capture_snapshot_v1(&self) -> BgmPlayerSnapshotV1 {
        let mut slots: Vec<BgmSlotSnapshotV1> = Vec::new();
        for i in 0..BGM_SLOT_COUNT {
            let has_any = self.bgm_datas[i].is_some() || self.bgm_slots[i].is_some() || self.bgm_names[i].is_some();
            if !has_any {
                continue;
            }

            slots.push(BgmSlotSnapshotV1 {
                slot: i as u8,
                path: self.bgm_names[i].clone(),
                sound_type: self.bgm_kinds[i],
                volume: self.bgm_volumes[i] as f32,
                muted: self.bgm_muted[i],
                playing: self.bgm_slots[i].is_some(),
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

        // Stop and clear current state.
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
                let data = vfs
                    .read_file(&path)
                    .with_context(|| format!("read BGM from vfs: {}", path))?;
                self.load_named(slot as i32, path, data)?;
            }

            if s.playing {
                // Only restart if data is present.
                if self.bgm_datas[slot].is_some() {
                    self.play(
                        slot as i32,
                        self.bgm_repeat[slot],
                        self.bgm_volumes[slot] as f32,
                        self.bgm_pan[slot],
                        Tween::default(),
                    )?;

                    if self.bgm_muted[slot] {
                        self.silent_on(slot as i32, Tween::default());
                    }
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

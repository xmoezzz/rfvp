use std::io::Cursor;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::sound::{FromFileError, Region};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{Panning, Tween};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::rfvp_audio::AudioManager;
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

enum BgmSource {
    VfsPath(String),
    Memory(Arc<[u8]>),
}

pub struct BgmPlayer {
    audio_manager: Arc<AudioManager>,
    bgm_tracks: [TrackHandle; BGM_SLOT_COUNT],
    bgm_slots: [Option<StreamingSoundHandle<FromFileError>>; BGM_SLOT_COUNT],
    bgm_sources: [Option<BgmSource>; BGM_SLOT_COUNT],
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
        let mut manager = audio_manager.kira_manager().lock().unwrap();

        let _ = manager
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
            bgm_sources: [(); BGM_SLOT_COUNT].map(|_| None),
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
        let data: Arc<[u8]> = Arc::from(bgm.into_boxed_slice());
        let cursor = Cursor::new(data.clone());
        let _ = StreamingSoundData::from_cursor(cursor)?;
        self.bgm_sources[slot] = Some(BgmSource::Memory(data));
        self.bgm_names[slot] = None;
        Ok(())
    }

    pub fn load_named(&mut self, slot: i32, name: impl Into<String>, vfs: &Vfs) -> Result<()> {
        let slot = slot as usize;
        let name = name.into();
        let stream = vfs
            .open_media_source(&name)
            .with_context(|| format!("open BGM stream from vfs: {}", name))?;
        let _ = StreamingSoundData::from_media_source(stream)
            .with_context(|| format!("decode BGM header from vfs stream: {}", name))?;
        self.bgm_names[slot] = Some(name.clone());
        self.bgm_sources[slot] = Some(BgmSource::VfsPath(name));
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

    fn streaming_data_for_source(
        source: &BgmSource,
        vfs: &Vfs,
    ) -> Result<StreamingSoundData<FromFileError>> {
        match source {
            BgmSource::VfsPath(path) => {
                let stream = vfs
                    .open_media_source(path)
                    .with_context(|| format!("open BGM stream from vfs: {}", path))?;
                Ok(StreamingSoundData::from_media_source(stream)
                    .with_context(|| format!("create streaming BGM from vfs: {}", path))?)
            }
            BgmSource::Memory(data) => {
                let cursor = Cursor::new(data.clone());
                Ok(StreamingSoundData::from_cursor(cursor)?)
            }
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
        let slot = slot as usize;

        self.bgm_pan[slot] = pan;
        self.bgm_repeat[slot] = repeat;
        self.bgm_volumes[slot] = volume;
        let actual_volume = if self.bgm_muted[slot] {
            0.0
        } else {
            self.effective_volume_for_slot(slot)
        };

        let source = match &self.bgm_sources[slot] {
            Some(source) => source,
            None => {
                log::error!("Tried to play BGM slot {}, but no BGM was loaded", slot);
                return Ok(());
            }
        };

        log::info!("Playing BGM slot {}", slot);

        let pan = Panning::from(pan as f32);
        let mut bgm = Self::streaming_data_for_source(source, vfs)?
            .panning(pan)
            .volume(actual_volume)
            .fade_in_tween(fade_in);
        if repeat {
            bgm = bgm.loop_region(Region::default());
        }

        let handle = self.audio_manager.play_streaming(bgm);

        if let Some(mut old_handle) = self.bgm_slots[slot].take() {
            old_handle.stop(fade_in);
        }

        self.bgm_slots[slot] = Some(handle);
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
            handle.set_volume(actual_volume, tween);
        } else {
            warn!(
                "Tried to set volume of BGM slot {}, but there was no BGM playing",
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

        if let Some(mut bgm) = self.bgm_slots[slot].take() {
            bgm.stop(fade_out);
        } else {
            warn!("Tried to stop a BGM that was not playing");
        }
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        let slot = slot as usize;
        self.bgm_slots[slot]
            .as_ref()
            .map(|handle| handle.state().is_advancing())
            .unwrap_or(false)
    }

    pub fn get_playing_slots(&self) -> Vec<bool> {
        let mut playing = Vec::with_capacity(BGM_SLOT_COUNT);
        for i in 0..BGM_SLOT_COUNT {
            playing.push(
                self.bgm_slots[i]
                    .as_ref()
                    .map(|handle| handle.state().is_advancing())
                    .unwrap_or(false),
            );
        }
        playing
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
            handle.set_volume(actual_volume, Tween::default());
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
                    handle.set_volume(actual_volume, tween);
                }
            }
        }
    }

    pub fn debug_summary(&self) -> BgmDebugSummary {
        let loaded_datas = self.bgm_sources.iter().filter(|x| x.is_some()).count();
        let playing_slots = self
            .bgm_slots
            .iter()
            .filter(|x| {
                x.as_ref()
                    .map(|handle| handle.state().is_advancing())
                    .unwrap_or(false)
            })
            .count();

        let mut slots = Vec::new();
        for slot in 0..BGM_SLOT_COUNT {
            let playing = self.bgm_slots[slot]
                .as_ref()
                .map(|handle| handle.state().is_advancing())
                .unwrap_or(false);
            let data_loaded = self.bgm_sources[slot].is_some();
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
            let has_any = self.bgm_sources[i].is_some()
                || self.bgm_slots[i].is_some()
                || self.bgm_names[i].is_some();
            if !has_any {
                continue;
            }

            slots.push(BgmSlotSnapshotV1 {
                slot: i as u8,
                path: self.bgm_names[i].clone(),
                sound_type: self.bgm_kinds[i],
                volume: self.bgm_volumes[i] as f32,
                muted: self.bgm_muted[i],
                playing: self.bgm_slots[i]
                    .as_ref()
                    .map(|handle| handle.state().is_advancing())
                    .unwrap_or(false),
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
            self.stop(i as i32, Tween::default());
            self.bgm_sources[i] = None;
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

            if s.playing && self.bgm_sources[slot].is_some() {
                self.play(
                    slot as i32,
                    self.bgm_repeat[slot],
                    self.bgm_volumes[slot] as f32,
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

use std::sync::Arc;

use kira::track::{TrackBuilder, TrackHandle, TrackId, TrackRoutes};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use rfvp_audio::AudioManager;
use kira::sound::Region;
use kira::tween::Tween;
use kira::Volume;
use tracing::warn;

pub const BGM_SLOT_COUNT: usize = 4;

pub struct BgmPlayer {
    audio_manager: Arc<AudioManager>,
    bgm_tracks: [TrackHandle; BGM_SLOT_COUNT],
    bgm_slots: [Option<StaticSoundHandle>; BGM_SLOT_COUNT],
    bgm_datas: [Option<Vec<u8>>; BGM_SLOT_COUNT],
    bgm_kinds: [Option<i32>; BGM_SLOT_COUNT],
}

impl BgmPlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        let mut manager = audio_manager.kira_manager().lock().unwrap();

        let bgm_tracks = [(); BGM_SLOT_COUNT].map(|_| {
            manager
                .add_sub_track(TrackBuilder::new().routes(TrackRoutes::parent(TrackId::Main)))
                .expect("Failed to create bgm track")
        });

        drop(manager);

        Self {
            audio_manager,
            bgm_tracks,
            bgm_slots: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_datas: [(); BGM_SLOT_COUNT].map(|_| None),
            bgm_kinds: [(); BGM_SLOT_COUNT].map(|_| None),
        }
    }

    pub fn load(&mut self, slot: i32, bgm: Vec<u8>) {
        let slot = slot as usize;
        self.bgm_datas[slot] = Some(bgm);
    }

    pub fn play(
        &mut self,
        slot: i32,
        repeat: bool,
        volume: Volume,
        pan: f64,
        fade_in: Tween,
    ) -> anyhow::Result<()> {
        let slot = slot as usize;

        let bgm_data = match &self.bgm_datas[slot] {
            Some(data) => data,
            None => {
                log::error!("Tried to play BGM slot {}, but no BGM was loaded", slot);
                return Ok(());
            }
        };

        let cursor = std::io::Cursor::new(bgm_data);
        let loop_region = repeat.then_some(Region::default());
        let settings = StaticSoundSettings::new()
            .panning(pan)
            .volume(volume)
            .fade_in_tween(fade_in)
            .loop_region(loop_region);

        let bgm = StaticSoundData::from_cursor(cursor, settings)?;
        let handle = self.audio_manager.play(bgm);

        if let Some(mut old_handle) = self.bgm_slots[slot].take() {
            old_handle.stop(fade_in).unwrap();
        }

        self.bgm_slots[slot] = Some(handle);
        Ok(())
    }

    pub fn set_volume(&mut self, slot: i32, volume: Volume, tween: Tween) {
        let slot = slot as usize;

        if let Some(handle) = self.bgm_slots[slot].as_mut() {
            handle.set_volume(volume, tween).unwrap();
        } else {
            warn!(
                "Tried to set volume of se slot {}, but there was no se playing",
                slot
            );
        }
    }

    pub fn stop(&mut self, slot: i32, fade_out: Tween) {
        let slot = slot as usize;

        if let Some(mut se) = self.bgm_slots[slot].take() {
            se.stop(fade_out).unwrap();
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
}

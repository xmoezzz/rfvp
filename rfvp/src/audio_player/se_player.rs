use std::sync::Arc;

use kira::track::{TrackBuilder, TrackHandle, TrackId, TrackRoutes};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use rfvp_audio::AudioManager;
use kira::sound::Region;
use kira::tween::Tween;
use kira::Volume;
use tracing::warn;

pub const SE_SLOT_COUNT: usize = 256;

pub struct SePlayer {
    audio_manager: Arc<AudioManager>,
    se_tracks: [TrackHandle; SE_SLOT_COUNT],
    se_slots: [Option<StaticSoundHandle>; SE_SLOT_COUNT],
    se_datas: [Option<Vec<u8>>; SE_SLOT_COUNT],
    se_kinds: [Option<i32>; SE_SLOT_COUNT],
}

impl SePlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        let mut manager = audio_manager.kira_manager().lock().unwrap();

        let se_tracks = [(); SE_SLOT_COUNT].map(|_| {
            manager
                .add_sub_track(TrackBuilder::new().routes(TrackRoutes::parent(TrackId::Main)))
                .expect("Failed to create se track")
        });

        drop(manager);

        Self {
            audio_manager,
            se_tracks,
            se_slots: [(); SE_SLOT_COUNT].map(|_| None),
            se_datas: [(); SE_SLOT_COUNT].map(|_| None),
            se_kinds: [(); SE_SLOT_COUNT].map(|_| None),
        }
    }

    pub fn load(&mut self, slot: i32, se: Vec<u8>) {
        let slot = slot as usize;
        self.se_datas[slot] = Some(se);
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

        let bgm_data = match &self.se_datas[slot] {
            Some(data) => data.clone(),
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

        if let Some(mut old_handle) = self.se_slots[slot].take() {
            old_handle.stop(fade_in).unwrap();
        }

        self.se_slots[slot] = Some(handle);
        Ok(())
    }

    pub fn set_volume(&mut self, slot: i32, volume: Volume, tween: Tween) {
        let slot = slot as usize;

        if let Some(handle) = self.se_slots[slot].as_mut() {
            handle.set_volume(volume, tween).unwrap();
        } else {
            warn!(
                "Tried to set volume of se slot {}, but there was no se playing",
                slot
            );
        }
    }

    pub fn set_type_volume(&mut self, kind: i32, volume: Volume, tween: Tween) {
        for slot in 0..SE_SLOT_COUNT {
            if self.se_kinds[slot] == Some(kind) {
                self.set_volume(slot as i32, volume, tween);
            }
        }
    }

    pub fn set_panning(&mut self, slot: i32, pan: f64, tween: Tween) {
        let slot = slot as usize;

        if let Some(handle) = self.se_slots[slot].as_mut() {
            handle.set_panning(pan, tween).unwrap();
        } else {
            warn!(
                "Tried to set pan of se slot {}, but there was no se playing",
                slot
            );
        }
    }

    pub fn stop(&mut self, slot: i32, fade_out: Tween) {
        let slot = slot as usize;

        if let Some(mut se) = self.se_slots[slot].take() {
            se.stop(fade_out).unwrap();
        } else {
            warn!("Tried to stop a SE that was not playing");
        }
    }

    pub fn stop_all(&mut self, fade_out: Tween) {
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
}

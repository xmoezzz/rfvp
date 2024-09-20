use std::sync::Arc;

use kira::track::{TrackBuilder, TrackHandle, TrackId, TrackRoutes};
use rfvp_audio::{AudioData, AudioFile, AudioHandle, AudioManager, AudioSettings};
use rfvp_core::{
    time::Tween,
    vm::command::types::{Pan, Volume},
};
use tracing::warn;

pub struct BgmPlayer {
    audio_manager: Arc<AudioManager>,
    bgm_track: TrackHandle,
    // TODO: async track loading?
    current_bgm: Option<AudioHandle>,
}

impl BgmPlayer {
    pub fn new(audio_manager: Arc<AudioManager>) -> Self {
        let mut manager = audio_manager.kira_manager().lock().unwrap();

        let bgm_track = manager
            .add_sub_track(TrackBuilder::new().routes(TrackRoutes::parent(TrackId::Main)))
            .expect("Failed to create bgm track");

        drop(manager);

        Self {
            audio_manager,
            bgm_track,
            current_bgm: None,
        }
    }

    pub fn play(
        &mut self,
        bgm: Arc<AudioFile>,
        _display_name: &str,
        repeat: bool,
        volume: Volume,
        fade_in: Tween,
    ) {
        let loop_start = repeat.then_some(bgm.info().loop_start);
        let kira_data = AudioData::from_audio_file(
            bgm,
            AudioSettings {
                track: self.bgm_track.id(),
                fade_in,
                loop_start,
                volume,
                pan: Pan::default(),
            },
        );

        let handle = self.audio_manager.play(kira_data);

        if let Some(mut old_handle) = self.current_bgm.take() {
            old_handle.stop(Tween::MS_15).unwrap();
        }

        self.current_bgm = Some(handle);
    }

    pub fn set_volume(&mut self, volume: Volume, tween: Tween) {
        if let Some(handle) = self.current_bgm.as_mut() {
            handle.set_volume(volume, tween).unwrap();
        } else {
            warn!("Tried to set volume of BGM, but no BGM is currently playing");
        }
    }

    pub fn stop(&mut self, fade_out: Tween) {
        if let Some(mut handle) = self.current_bgm.take() {
            handle.stop(fade_out).unwrap();
        } else {
            warn!("Tried to stop BGM, but no BGM is currently playing");
        }
    }
}

// TODO: make it renderable and updatable, as it can display they track name when the BGM starts

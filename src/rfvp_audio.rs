use std::fmt::Debug;
use std::sync::Mutex;

use kira::{AudioManager as KiraAudioManager, AudioManagerSettings, Tween};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};

pub struct AudioManager {
    manager: Mutex<KiraAudioManager>,
}

impl Debug for AudioManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioManager").finish()
    }
}

impl AudioManager {
    pub fn new() -> Self {
        // The original engine expects a large number of mixer sub-tracks
        // (e.g., 256 SE slots + BGM tracks). Kira enforces this capacity.
        // Increase the sub-track capacity to avoid ResourceLimitReached when
        // creating the SE tracks.
        let mut settings = AudioManagerSettings::default();
        settings.capacities.sub_track_capacity = 512;

        let mgr = KiraAudioManager::new(settings)
            .expect("failed to create Kira AudioManager");
        Self {
            manager: Mutex::new(mgr),
        }
    }

    pub fn kira_manager(&self) -> &Mutex<KiraAudioManager> {
        &self.manager
    }

    pub fn play(&self, data: StaticSoundData) -> StaticSoundHandle {
        let mut mgr = self.manager.lock().unwrap();
        mgr.play(data).expect("failed to play sound")
    }

    pub fn master_vol(&self, vol: f32) {
        let mut mgr = self.manager.lock().unwrap();
        mgr.main_track().set_volume(vol, Tween::default());
    }
}

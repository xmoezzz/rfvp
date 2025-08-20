use std::sync::Mutex;

use kira::{sound::SoundData, AudioManagerSettings, Capacities, DefaultBackend};


pub struct AudioManager {
    manager: Mutex<kira::AudioManager<DefaultBackend>>,
}

impl AudioManager {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let manager = kira::AudioManager::new(AudioManagerSettings {
            capacities: Capacities {
                sub_track_capacity: 512,
                ..Default::default()
            },
            main_track_builder: Default::default(),
            backend_settings: Default::default(),
            ..Default::default()
        
        })
            .expect("Failed to create kira audio manager");

        Self {
            manager: Mutex::new(manager),
        }
    }

    pub fn play<S: SoundData>(&self, data: S) -> S::Handle
    where
        S::Error: std::fmt::Debug,
    {
        let mut manager = self.manager.lock().unwrap();

        manager.play(data).expect("Failed to start playing audio")
    }

    pub fn kira_manager(&self) -> &Mutex<kira::AudioManager<DefaultBackend>> {
        &self.manager
    }

    pub fn master_vol(&self, volume: f32) -> anyhow::Result<()> {
        let manager = self.manager.lock().unwrap();
        
        // manager.main_track().set_volume(volume as f64, Default::default());
        Ok(())
    }
}

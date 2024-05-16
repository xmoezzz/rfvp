use std::sync::Mutex;

use kira::{manager::AudioManagerSettings, sound::SoundData};

type Backend = kira::manager::backend::cpal::CpalBackend;

pub struct AudioManager {
    manager: Mutex<kira::manager::AudioManager<Backend>>,
}

impl AudioManager {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let manager = kira::manager::AudioManager::new(AudioManagerSettings::default())
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

    pub fn kira_manager(&self) -> &Mutex<kira::manager::AudioManager<Backend>> {
        &self.manager
    }

    pub fn master_vol(&self, volume: f32) -> anyhow::Result<()> {
        let manager = self.manager.lock().unwrap();
        
        manager.main_track().set_volume(volume as f64, Default::default())?;
        Ok(())
    }
}

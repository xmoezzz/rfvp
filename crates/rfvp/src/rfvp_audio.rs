use std::fmt::Debug;

// ─── Kira (desktop audio) ─────────────────────────────────────────────────────

#[cfg(all(feature = "audio", not(target_os = "uefi")))]
mod real {
    use std::sync::Mutex;

    use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
    #[cfg(not(target_arch = "wasm32"))]
    use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
    #[cfg(not(target_arch = "wasm32"))]
    use kira::sound::FromFileError;
    use kira::{AudioManager as KiraAudioManager, AudioManagerSettings};

    pub use kira::Tween;

    pub struct AudioManager {
        manager: Mutex<KiraAudioManager>,
    }

    impl std::fmt::Debug for AudioManager {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("AudioManager").finish()
        }
    }

    impl AudioManager {
        pub fn new() -> Self {
            let mut settings = AudioManagerSettings::default();
            settings.capacities.sub_track_capacity = 512;
            let mgr = KiraAudioManager::new(settings).expect("failed to create Kira AudioManager");
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

        #[cfg(not(target_arch = "wasm32"))]
        pub fn play_streaming(
            &self,
            data: StreamingSoundData<FromFileError>,
        ) -> StreamingSoundHandle<FromFileError> {
            let mut mgr = self.manager.lock().unwrap();
            mgr.play(data).expect("failed to play streaming sound")
        }

        pub fn master_vol(&self, vol: f32) {
            let mut mgr = self.manager.lock().unwrap();
            mgr.main_track().set_volume(vol, Tween::default());
        }

        /// No-op on kira — kira has its own background thread.
        pub fn tick(&self, _delta_ms: u32) {}
    }
}

// ─── Shared no-audio Tween (UEFI and no-audio desktop) ───────────────────────

#[cfg(any(not(feature = "audio"), target_os = "uefi"))]
mod no_audio_tween {
    use crate::platform_time::Duration;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Tween {
        pub duration: Duration,
    }

    impl Default for Tween {
        fn default() -> Self {
            Self {
                duration: Duration::from_secs(0),
            }
        }
    }
}

// ─── Stub AudioManager (no-audio desktop builds) ─────────────────────────────

#[cfg(all(not(feature = "audio"), not(target_os = "uefi")))]
mod stub {
    pub struct AudioManager {
        master_volume: std::sync::Mutex<f32>,
    }

    impl std::fmt::Debug for AudioManager {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("AudioManager").finish()
        }
    }

    impl AudioManager {
        pub fn new() -> Self {
            Self {
                master_volume: std::sync::Mutex::new(1.0),
            }
        }

        pub fn master_vol(&self, vol: f32) {
            if let Ok(mut v) = self.master_volume.lock() {
                *v = vol;
            }
        }

        pub fn tick(&self, _delta_ms: u32) {}
    }
}

// ─── Anzu-HAL AudioManager (UEFI + anzu-audio feature) ───────────────────────

#[cfg(all(target_os = "uefi", feature = "anzu-audio"))]
mod anzu {
    use anzu_hal::AudioSystem;
    use std::sync::Arc;

    pub struct AudioManager {
        system: Arc<AudioSystem>,
    }

    impl std::fmt::Debug for AudioManager {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("AudioManager").finish()
        }
    }

    impl AudioManager {
        pub fn new() -> Self {
            Self {
                system: Arc::new(AudioSystem::new()),
            }
        }

        pub fn anzu_system(&self) -> Arc<AudioSystem> {
            Arc::clone(&self.system)
        }

        pub fn master_vol(&self, vol: f32) {
            self.system.set_master_volume(vol);
        }

        pub fn tick(&self, delta_ms: u32) {
            self.system.tick(delta_ms);
        }
    }
}

// ─── Silent stub AudioManager (UEFI without anzu-audio) ──────────────────────

#[cfg(all(target_os = "uefi", not(feature = "anzu-audio")))]
mod uefi_stub {
    pub struct AudioManager;

    impl std::fmt::Debug for AudioManager {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("AudioManager").finish()
        }
    }

    impl AudioManager {
        pub fn new() -> Self {
            Self
        }
        pub fn master_vol(&self, _vol: f32) {}
        pub fn tick(&self, _delta_ms: u32) {}
    }
}

// ─── Re-exports ───────────────────────────────────────────────────────────────

#[cfg(all(feature = "audio", feature = "no-audio", not(target_os = "uefi")))]
compile_error!("features `audio` and `no-audio` are mutually exclusive outside UEFI builds.");

#[cfg(all(feature = "audio", not(target_os = "uefi")))]
pub use real::{AudioManager, Tween};

#[cfg(all(target_os = "uefi", feature = "anzu-audio"))]
pub use anzu::AudioManager;
#[cfg(target_os = "uefi")]
pub use no_audio_tween::Tween;
#[cfg(all(target_os = "uefi", not(feature = "anzu-audio")))]
pub use uefi_stub::AudioManager;

#[cfg(all(not(feature = "audio"), not(target_os = "uefi")))]
pub use no_audio_tween::Tween;
#[cfg(all(not(feature = "audio"), not(target_os = "uefi")))]
pub use stub::AudioManager;

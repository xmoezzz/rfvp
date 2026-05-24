use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::rfvp_audio::AudioManager;

use super::motion_manager::MotionManager;

pub const MOVIE_GRAPH_ID: u16 = 4063;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieMode {
    ModalWithAudio,
    LayerNoAudio,
}

#[derive(Debug, Default)]
pub struct VideoPlayerManager {
    playing: bool,
    modal: bool,
}

impl VideoPlayerManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn is_loaded(&self) -> bool {
        false
    }

    pub fn is_modal_active(&self) -> bool {
        self.playing && self.modal
    }

    pub fn start(
        &mut self,
        movie_path: impl AsRef<Path>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        motion: &mut MotionManager,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<()> {
        let _ = (movie_path, mode, screen_w, screen_h, motion, audio_manager);
        Err(anyhow!("movie playback requires the native-video feature"))
    }

    pub fn start_from_bytes(
        &mut self,
        movie_name: &str,
        bytes: Vec<u8>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        motion: &mut MotionManager,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<()> {
        let _ = (movie_name, bytes, mode, screen_w, screen_h, motion, audio_manager);
        Err(anyhow!("movie playback requires the native-video feature"))
    }

    pub fn tick(&mut self, _motion: &mut MotionManager) -> Result<()> {
        Ok(())
    }

    pub fn stop(&mut self, _motion: &mut MotionManager) {
        self.playing = false;
        self.modal = false;
    }
}

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::rfvp_audio::AudioManager;

use super::motion_manager::MotionManager;

pub const MOVIE_GRAPH_ID: u16 = 4063;
pub const MOVIE_GROUP_PRIM_ID: i16 = 4095;
pub const MOVIE_SPRT_PRIM_ID: i16 = 4094;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieMode {
    ModalWithAudio,
    LayerNoAudio,
}

#[derive(Debug, Default)]
pub struct VideoPlayerManager {
    playing: bool,
    loaded: bool,
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
        self.loaded
    }

    pub fn is_modal_active(&self) -> bool {
        self.playing && self.modal
    }

    pub fn start(
        &mut self,
        movie_path: impl AsRef<Path>,
        mode: MovieMode,
        _screen_w: u32,
        _screen_h: u32,
        _motion: &mut MotionManager,
        _audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<()> {
        self.playing = false;
        self.loaded = false;
        self.modal = false;
        Err(anyhow!(
            "movie playback is disabled in the wasm build: {}",
            movie_path.as_ref().display()
        ))
    }

    pub fn tick(&mut self, _motion: &mut MotionManager) -> Result<()> {
        Ok(())
    }

    pub fn stop(&mut self, _motion: &mut MotionManager) {
        self.playing = false;
        self.loaded = false;
        self.modal = false;
    }
}

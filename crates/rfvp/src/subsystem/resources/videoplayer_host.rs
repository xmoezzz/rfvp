use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::rfvp_audio::AudioManager;

use super::motion_manager::MotionManager;

pub const MOVIE_GRAPH_ID: u16 = 4063;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieMode {
    ModalWithAudio,
    LayerNoAudio,
}

#[derive(Debug, Clone)]
pub struct HostMovieCommand {
    pub name: String,
    pub bytes: Vec<u8>,
    pub mode: MovieMode,
    pub screen_w: u32,
    pub screen_h: u32,
}

#[derive(Debug, Default)]
pub struct VideoPlayerManager {
    playing: bool,
    loaded: bool,
    modal: bool,
    pending_commands: Vec<HostMovieCommand>,
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
        screen_w: u32,
        screen_h: u32,
        motion: &mut MotionManager,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<()> {
        let name = movie_path.as_ref().as_os_str().to_string();
        self.start_from_bytes(
            &name,
            Vec::new(),
            mode,
            screen_w,
            screen_h,
            motion,
            audio_manager,
        )
    }

    pub fn start_from_bytes(
        &mut self,
        movie_name: &str,
        bytes: Vec<u8>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        _motion: &mut MotionManager,
        _audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<()> {
        self.playing = true;
        self.loaded = true;
        self.modal = matches!(mode, MovieMode::ModalWithAudio);
        self.pending_commands.push(HostMovieCommand {
            name: movie_name.to_string(),
            bytes,
            mode,
            screen_w,
            screen_h,
        });
        Ok(())
    }

    pub fn tick(&mut self, _motion: &mut MotionManager) -> Result<()> {
        Ok(())
    }

    pub fn stop(&mut self, _motion: &mut MotionManager) {
        self.playing = false;
        self.modal = false;
    }

    pub fn drain_host_commands(&mut self, out: &mut Vec<HostMovieCommand>) {
        out.extend(self.pending_commands.drain(..));
    }
}

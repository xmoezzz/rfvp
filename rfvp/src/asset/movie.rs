use std::io::Cursor;

use anyhow::{Context, Result};
use rfvp_audio::AudioManager;
use rfvp_render::GpuCommonResources;
use rfvp_video::{mp4::Mp4, VideoPlayer};

use crate::asset::Asset;

pub struct Movie {
    // TODO: allow to start decoding the video before the first frame is requested
    // TODO: use a streaming reader instead of reading the whole video into memory (they're HUGE)
    mp4: Mp4<Cursor<Vec<u8>>>,
}

impl Asset for Movie {
    fn load_from_bytes(data: Vec<u8>) -> Result<Self> {
        let cursor = Cursor::new(data);
        let mp4 = Mp4::new(cursor).context("Reading Mp4")?;
        Ok(Self { mp4 })
    }
}

impl Movie {
    pub fn play(
        &self,
        resources: &GpuCommonResources,
        audio_manager: &AudioManager,
    ) -> Result<VideoPlayer> {
        VideoPlayer::new(resources, audio_manager, self.mp4.clone())
    }
}

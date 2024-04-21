use anyhow::Result;

use bytes::{Buf};
use mp4::{Mp4Track};

use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use std::sync::{Arc, Mutex};
use std::thread::{spawn, JoinHandle};
use std::time::{Duration, Instant};

const BUF_SIZE: usize = 3;


pub struct VideoPlayer {
    pub width: u32,
    pub height: u32,
    pub playing: bool,
    pub pixel_data: Vec<u8>,
}

impl VideoPlayer {
    pub fn new() -> Result<Self> {
        let player = Self {
            width: 0,
            height: 0,
            playing: false,
            pixel_data: Vec::new(),
        };

        Ok(player)
    }

    fn play_threaded(
        &mut self,
        path: impl AsRef<Path>,
        _width: u32,
        _height: u32,
        _should_play: Arc<AtomicBool>,
    ) -> Result<()> {
        let file = std::fs::File::open(&path)?;
        let file_size = file.metadata()?.len();
        let reader = BufReader::new(file);

        let _mp4 = mp4::Mp4Reader::read_header(reader, file_size)?;

        
        Ok(())
    }

    // fn render_video_to_texture(&mut self, video: &Video, _index: i32) {
    //     let data = video.data(0);
    //     self.pixel_data = data.to_vec();
    // }

    pub fn is_playing(&self) -> bool {
        self.playing
    }
}

pub struct VideoPlayerManager {
    player_thread: Option<JoinHandle<()>>,
    should_play: Arc<AtomicBool>,
}

impl VideoPlayerManager {
    pub fn new() -> Self {
        Self {
            player_thread: None,
            should_play: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn play(&mut self, path: impl AsRef<Path>, width: u32, height: u32) -> Result<()> {
        let mut player = VideoPlayer::new()?;
        let path = path.as_ref().to_path_buf();
        let flag = self.should_play.clone();
        let player_thread = spawn(move || {
            player.play_threaded(path, width, height, flag).unwrap();
        });
        self.player_thread = Some(player_thread);
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        if let Some(player_thread) = &self.player_thread {
            !player_thread.is_finished()
        } else {
            false
        }
    }

    pub fn stop(&mut self) {
        if let Some(player_thread) = self.player_thread.take() {
            self.should_play
                .store(false, std::sync::atomic::Ordering::Relaxed);
            let _ = player_thread.join();
        }
    }
}

impl Default for VideoPlayerManager {
    fn default() -> Self {
        Self::new()
    }
}

use rfvp_core::vm::command::types::{Pan, Volume};

use crate::audio::SE_SLOT_COUNT;

#[derive(Debug, Copy, Clone)]
pub struct BgmState {
    pub bgm_id: i32,
    pub volume: Volume,
}

#[derive(Debug, Copy, Clone)]
pub struct SeState {
    pub se_id: i32,
    pub volume: Volume,
    pub pan: Pan,
    pub play_speed: f32,
}

#[derive(Debug, Copy, Clone)]
pub struct AudioState {
    pub bgm: Option<BgmState>,
    pub se: [Option<SeState>; SE_SLOT_COUNT],
}

impl AudioState {
    pub fn new() -> Self {
        Self {
            bgm: None,
            se: [None; SE_SLOT_COUNT],
        }
    }
}

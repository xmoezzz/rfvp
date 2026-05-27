use alloc::vec::Vec;

use crate::host_api::{AudioParams, AudioStreamDesc, AudioStreamId, EncodedAudioKind};
use crate::platform_time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Debug, Clone)]
pub enum AudioCommand {
    LoadEncoded {
        id: AudioStreamId,
        kind: EncodedAudioKind,
        bytes: Vec<u8>,
    },
    CreateStream {
        id: AudioStreamId,
        desc: AudioStreamDesc,
    },
    SubmitI16 {
        id: AudioStreamId,
        samples: Vec<i16>,
    },
    SubmitF32 {
        id: AudioStreamId,
        samples: Vec<f32>,
    },
    Play {
        id: AudioStreamId,
        params: AudioParams,
        fade_in_ms: u32,
    },
    Stop {
        id: AudioStreamId,
        fade_ms: u32,
    },
    Pause {
        id: AudioStreamId,
    },
    Resume {
        id: AudioStreamId,
    },
    SetParams {
        id: AudioStreamId,
        params: AudioParams,
    },
    DestroyStream {
        id: AudioStreamId,
    },
    MasterVolume {
        volume: f32,
    },
}

#[derive(Debug, Default)]
pub struct AudioManager {
    commands: spin::Mutex<Vec<AudioCommand>>,
}

impl AudioManager {
    pub fn new() -> Self {
        Self {
            commands: spin::Mutex::new(Vec::new()),
        }
    }

    pub fn push_command(&self, command: AudioCommand) {
        self.commands.lock().push(command);
    }

    pub fn drain_commands(&self, out: &mut Vec<AudioCommand>) {
        let mut commands = self.commands.lock();
        out.extend(commands.drain(..));
    }

    pub fn master_vol(&self, vol: f32) {
        self.push_command(AudioCommand::MasterVolume { volume: vol });
    }

    pub fn tick(&self, _delta_ms: u32) {}
}

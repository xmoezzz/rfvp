use std::sync::Arc;

use anyhow::anyhow;
use ringbuf::HeapProducer;
use rfvp_core::{
    time::{Ticks, Tween},
    types::{AudioWaitStatus, Pan, Volume},
};

use crate::sound::{Command, Shared};

pub struct AudioHandle {
    pub(super) command_producer: HeapProducer<Command>,
    pub(super) shared: Arc<Shared>,
}

impl AudioHandle {
    pub fn get_wait_status(&self) -> AudioWaitStatus {
        AudioWaitStatus::from_bits_truncate(
            self.shared
                .wait_status
                .load(std::sync::atomic::Ordering::SeqCst),
        )
    }

    #[allow(unused)] // TODO: use it for lip-sync
    pub fn get_amplitude(&self) -> f32 {
        f32::from_bits(
            self.shared
                .amplitude
                .load(std::sync::atomic::Ordering::SeqCst),
        )
    }

    /// Sets the volume of the sound.
    /// The volume is a value between 0.0 and 1.0, on the linear scale.
    pub fn set_volume(&mut self, volume: Volume, tween: Tween) -> anyhow::Result<()> {
        self.command_producer
            .push(Command::SetVolume(volume, tween))
            .map_err(|_| anyhow!("Command queue full"))
    }

    /// Sets the panning of the sound
    pub fn set_panning(&mut self, panning: Pan, tween: Tween) -> anyhow::Result<()> {
        self.command_producer
            .push(Command::SetPanning(panning, tween))
            .map_err(|_| anyhow!("Command queue full"))
    }

    /// Fades out the sound to silence with the given tween and then
    /// stops playback.
    ///
    /// Once the sound is stopped, it cannot be restarted.
    pub fn stop(&mut self, tween: Tween) -> anyhow::Result<()> {
        self.command_producer
            .push(Command::Stop(tween))
            .map_err(|_| anyhow!("Command queue full"))
    }

    /// Returns the current playback position of the sound.
    pub fn position(&self) -> Ticks {
        Ticks::from_millis(
            self.shared
                .position
                .load(std::sync::atomic::Ordering::SeqCst) as f32,
        )
    }
}

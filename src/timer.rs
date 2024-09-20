use rfvp_audio::AudioHandle;
use rfvp_core::time::Ticks;
use tracing::warn;

pub struct IndependentTimer {
    /// How many time units are there in one second
    time_base: u32,
    /// How many time units have passed since the start of the timer
    time: u64,
}

impl IndependentTimer {
    pub fn new(time_base: u32) -> IndependentTimer {
        IndependentTimer { time_base, time: 0 }
    }

    pub fn update(&mut self, delta_time: Ticks) {
        self.time += (delta_time.as_seconds() as f64 * self.time_base as f64) as u64;
    }

    pub fn time(&self) -> u64 {
        self.time
    }
}

pub struct AudioTiedTimer {
    timer: IndependentTimer,
    audio_handle: AudioHandle,
}

impl AudioTiedTimer {
    pub const MAX_DRIFT: f64 = 0.3;

    pub fn new(time_base: u32, audio_handle: AudioHandle) -> AudioTiedTimer {
        AudioTiedTimer {
            timer: IndependentTimer::new(time_base),
            audio_handle,
        }
    }

    pub fn update(&mut self, delta_time: Ticks) {
        self.timer.update(delta_time);

        let audio_secs = self.audio_handle.position().as_seconds() as f64;
        let timer_secs = self.timer.time() as f64 / self.timer.time_base as f64;

        if (audio_secs - timer_secs).abs() > Self::MAX_DRIFT {
            warn!(
                "Audio and timer are out of sync by {} seconds, resetting timer",
                audio_secs - timer_secs
            );
            self.timer.time = (audio_secs * self.timer.time_base as f64) as u64;
        }
    }

    pub fn time(&self) -> u64 {
        self.timer.time()
    }
}

pub enum Timer {
    Independent(IndependentTimer),
    AudioTiedTimer(AudioTiedTimer),
}

impl Timer {
    pub fn new_independent(time_base: u32) -> Timer {
        Timer::Independent(IndependentTimer::new(time_base))
    }

    pub fn new_audio_tied(time_base: u32, audio_handle: AudioHandle) -> Timer {
        Timer::AudioTiedTimer(AudioTiedTimer::new(time_base, audio_handle))
    }

    pub fn update(&mut self, delta_time: Ticks) {
        match self {
            Timer::Independent(timer) => timer.update(delta_time),
            Timer::AudioTiedTimer(timer) => timer.update(delta_time),
        }
    }

    pub fn time(&self) -> u64 {
        match self {
            Timer::Independent(timer) => timer.time(),
            Timer::AudioTiedTimer(timer) => timer.time(),
        }
    }
}

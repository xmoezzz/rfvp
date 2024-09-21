use std::{
    f32::consts::SQRT_2,
    sync::{
        atomic::{AtomicI32, AtomicU32},
        Arc,
    },
};

use kira::{
    clock::clock_info::ClockInfoProvider, modulator::value_provider::ModulatorValueProvider,
    sound::Sound, track::TrackId, Frame, OutputDestination,
};
use ringbuf::{traits::Consumer as _, HeapCons};
use rfvp_core::{
    format::audio::{AudioFrameSource, AudioSource},
    time::{Ticks, Tween, Tweener},
    vm::command::types::{AudioWaitStatus, Pan, Volume},
};
use tracing::{debug, warn};

use crate::{resampler::Resampler, AudioData};

pub const COMMAND_BUFFER_CAPACITY: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    SetVolume(Volume, Tween),
    SetPanning(Pan, Tween),
    Stop(Tween),
}

pub(crate) struct Shared {
    pub wait_status: AtomicI32,
    // TODO: use it to implement BGMSYNC (I don't know which unit it uses)
    // in ms, relative to the start of the sound
    pub position: AtomicU32,
    // used for lip sync
    pub amplitude: AtomicU32,
}

impl Shared {
    fn new() -> Self {
        Self {
            wait_status: AtomicI32::new(0),
            position: AtomicU32::new(0),
            amplitude: AtomicU32::new(0),
        }
    }
}

/// The playback state of a sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaybackState {
    /// The sound is playing normally.
    Playing,
    /// The sound is fading out, and when the fade-out
    /// is finished, playback will stop.
    Stopping,
    /// The sound has stopped and can no longer be resumed.
    Stopped,
}

pub struct SampleProvider<S: AudioFrameSource + Send> {
    source: AudioSource<S>,
    loop_start: Option<u32>,
    resampler: Resampler,
    fractional_position: f64,
    reached_eof: bool,
}

impl<S: AudioFrameSource + Send> SampleProvider<S> {
    fn new(audio: S, loop_start: Option<u32>) -> Self {
        Self {
            source: AudioSource::new(audio),
            loop_start,
            resampler: Resampler::new(0),
            fractional_position: 0.0,
            reached_eof: false,
        }
    }

    fn push_frame_to_resampler(&mut self) {
        let frame = match self.source.read_sample() {
            Some((left, right)) => Frame { left, right },
            None => {
                if let Some(loop_start) = self.loop_start {
                    self.source
                        .samples_seek(loop_start)
                        .expect("Could not seek to loop start");

                    return self.push_frame_to_resampler();
                } else {
                    self.reached_eof = true;
                    Frame::ZERO
                }
            }
        };

        let next_sample_index = self.source.current_samples_position();
        self.resampler.push_frame(frame, next_sample_index - 1);
    }

    fn next(&mut self, dt: f64) -> Frame {
        let out = self.resampler.get(self.fractional_position as f32);
        self.fractional_position += dt * self.source.sample_rate() as f64;
        while self.fractional_position >= 1.0 {
            self.fractional_position -= 1.0;
            self.push_frame_to_resampler();
        }

        out
    }
}

pub struct AudioSound<S: AudioFrameSource + Send> {
    track_id: TrackId,
    command_consumer: HeapCons<Command>,
    shared: Arc<Shared>,
    state: PlaybackState,
    volume: Tweener,
    panning: Tweener,
    volume_fade: Tweener,
    sample_provider: SampleProvider<S>,
}

impl<S: AudioFrameSource + Send> AudioSound<S> {
    pub fn new(data: AudioData<S>, command_consumer: HeapCons<Command>) -> Self {
        debug!("Creating audio sound for track {:?}", data.settings.track);

        let mut volume_fade = Tweener::new(0.0);
        volume_fade.enqueue_now(1.0, data.settings.fade_in);

        let shared = Arc::new(Shared::new());

        AudioSound {
            track_id: data.settings.track,
            command_consumer,
            shared,
            state: PlaybackState::Playing,
            volume: Tweener::new(data.settings.volume.0),
            panning: Tweener::new(data.settings.pan.0),
            volume_fade,
            sample_provider: SampleProvider::new(data.source, data.settings.loop_start),
        }
    }

    fn stop(&mut self, fade_out_tween: Tween) {
        self.state = PlaybackState::Stopping;
        self.volume_fade.enqueue_now(0.0, fade_out_tween);
    }

    fn wait_status(&self) -> AudioWaitStatus {
        let mut result = AudioWaitStatus::empty();

        if self.state == PlaybackState::Stopped {
            result |= AudioWaitStatus::STOPPED;
        }
        if self.state == PlaybackState::Playing {
            result |= AudioWaitStatus::PLAYING;
        }
        if self.volume.is_idle() {
            result |= AudioWaitStatus::VOLUME_TWEENER_IDLE;
        }
        if self.panning.is_idle() {
            result |= AudioWaitStatus::PANNING_TWEENER_IDLE;
        }
        result |= AudioWaitStatus::PLAY_SPEED_TWEENER_IDLE;

        result
    }

    pub(crate) fn shared(&self) -> Arc<Shared> {
        self.shared.clone()
    }
}

impl<S: AudioFrameSource + Send> Sound for AudioSound<S> {
    fn output_destination(&mut self) -> OutputDestination {
        OutputDestination::Track(self.track_id)
    }

    fn on_start_processing(&mut self) {
        while let Some(command) = self.command_consumer.try_pop() {
            match command {
                // note: unlike in the layer props, we do the "enqueue_now" thing here
                // bacause we don't want to wait for previous audio changes to be applied
                // ideally, this should never allocate the tweener queue
                Command::SetVolume(volume, tween) => self.volume.enqueue_now(volume.0, tween),
                Command::SetPanning(panning, tween) => self.panning.enqueue_now(panning.0, tween),
                Command::Stop(tween) => self.stop(tween),
            }
        }

        self.shared.wait_status.store(
            self.wait_status().bits(),
            std::sync::atomic::Ordering::SeqCst,
        );
        // TODO: compute the amplitude
        let position = self.sample_provider.source.current_samples_position() as u64 * 1000
            / self.sample_provider.source.sample_rate() as u64;
        self.shared.position.store(
            position.try_into().unwrap(),
            std::sync::atomic::Ordering::SeqCst,
        );
    }

    fn process(
        &mut self,
        dt: f64,
        _clock_info_provider: &ClockInfoProvider,
        _modulator_value_provider: &ModulatorValueProvider,
    ) -> Frame {
        let dt_ticks = Ticks::from_seconds(dt as f32);

        // update tweeners
        self.volume.update(dt_ticks);
        self.panning.update(dt_ticks);
        self.volume_fade.update(dt_ticks);

        if self.state == PlaybackState::Stopping && self.volume_fade.is_idle() {
            self.state = PlaybackState::Stopped
        }

        let mut f = self.sample_provider.next(dt);

        if self.sample_provider.reached_eof && self.sample_provider.resampler.outputting_silence() {
            self.state = PlaybackState::Stopped;
        }

        let pan = self.panning.value();
        let volume = self.volume_fade.value() * self.volume.value();

        f *= volume;
        if pan != 0.0 {
            f = Frame::new(f.left * (1.0 - pan).sqrt(), f.right * pan.sqrt()) * SQRT_2
        }

        f
    }

    fn finished(&self) -> bool {
        let result = self.state == PlaybackState::Stopped;
        if result {
            debug!(
                "Track {:?} is finished, we are gonna get dropped soon",
                &self.track_id
            );
            // make sure that before we get dropped, the wait status is updated
            // otherwise SEWAIT can hand forever
            self.shared.wait_status.store(
                self.wait_status().bits(),
                std::sync::atomic::Ordering::SeqCst,
            );
        }

        result
    }
}

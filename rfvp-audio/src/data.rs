//! Implements the SoundData trait for the Kira audio library.

use std::sync::Arc;

use anyhow::Result;
use kira::sound::{Sound, SoundData};
use ringbuf::{traits::Split as _, HeapRb};
use rfvp_core::format::audio::{AudioDecoder, AudioFile, AudioFrameSource};

use super::AudioSettings;
use crate::{
    handle::AudioHandle,
    sound::{AudioSound, COMMAND_BUFFER_CAPACITY},
};

pub struct AudioData<S: AudioFrameSource> {
    pub source: S,
    pub settings: AudioSettings,
}

impl AudioData<AudioDecoder<Arc<AudioFile>>> {
    pub fn from_audio_file(audio: Arc<AudioFile>, settings: AudioSettings) -> Self {
        Self {
            source: AudioDecoder::new(audio).expect("Failed to create audio decoder"),
            settings,
        }
    }
}

impl<S: AudioFrameSource + Send + 'static> SoundData for AudioData<S> {
    type Error = anyhow::Error;
    type Handle = AudioHandle;

    fn into_sound(self) -> Result<(Box<dyn Sound>, Self::Handle), Self::Error> {
        let (sound, handle) = self.split();
        Ok((Box::new(sound), handle))
    }
}

impl<S: AudioFrameSource + Send> AudioData<S> {
    fn split(self) -> (AudioSound<S>, AudioHandle) {
        let (command_producer, command_consumer) = HeapRb::new(COMMAND_BUFFER_CAPACITY).split();

        let sound = AudioSound::new(self, command_consumer);
        let shared = sound.shared();

        (
            sound,
            AudioHandle {
                command_producer,
                shared,
            },
        )
    }
}

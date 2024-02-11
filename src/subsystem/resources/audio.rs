// use crate::subsystem::audio_controller::AudioController;
// use crate::subsystem::audio_controller;
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
// use std::sync::mpsc;
use std::collections::HashMap;
use std::io::BufReader;


/// `AudioPlayer` is the resource responsible to handle musics, sound effects, and action on them
pub struct Audio {
    // event_sender: mpsc::Sender<AudioEvent>,

    stream_handle: OutputStreamHandle,
    sinks: HashMap<usize, Sink>,
    audios: HashMap<usize, SoundChannel>,
    sounds: HashMap<usize, SoundChannel>,
    sound_type_volumes: HashMap<i32, f32>,
    
    master_volume: f32,
}


struct SoundChannel {
    id: usize,
    buffer: Vec<u8>,
    path: String,
    crossfade: u32,
    volume: f32,
    looped: bool,
    sound_type: i32,
}

impl Audio {
    pub(crate) fn default() -> Self {
        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
        // let _sink = Sink::try_new(&stream_handle).unwrap();
        // let (event_sender, receiver) = mpsc::channel();

        // std::thread::spawn(move || audio_controller::audio_thread(AudioController::new(receiver)));

        // Audio { event_sender }
        Audio { 
            stream_handle,
            sinks: HashMap::new(),
            audios: HashMap::new(),
            sounds: HashMap::new(),
            sound_type_volumes: HashMap::new(),
            master_volume: 1.0,
        }
    }

    // pub fn load_audio(
    //     &mut self,
    //     buffer: Vec<u8>,
    //     id: usize,
    //     config: PlayConfig,
    // ) -> Result<usize, Error> {
    //     if let Ok(()) = self.event_sender.send(AudioEvent::LoadAudio {
    //         buffer,
    //         config,
    //         sound_id: id,
    //     }) {
    //         return Ok(id);
    //     }
    //     return Err(Error::ImpossibleToLoadSound);
    // }

    /// load an audio into memory
    pub fn load_audio(
        &mut self,
        buffer: Vec<u8>,
        id: usize,
        config: PlayConfig,
    ) -> anyhow::Result<()> {
        log::debug!("load sound {}: {}", id, &config.path);

        // if this channel is already occupied, stop it
        if let Some(sink) = self.sinks.remove(&id) {
            sink.stop();
            drop(sink);
        }

        self.audios.remove(&id);
        
        let channel = SoundChannel {
            id,
            buffer,
            path: config.path,
            crossfade: config.crossfade,
            volume: config.volume,
            looped: config.looped,
            sound_type: 0,
        };
        self.audios.insert(id, channel);

        Ok(())
    } 
    

    /// Play the audio identified with `name`
    // pub fn play_audio(
    //     &mut self,
    //     id: usize,
    //     looped: bool,
    // ) -> Result<usize, Error> {
    //     if let Ok(()) = self.event_sender.send(AudioEvent::PlayAudio {
    //         sound_id: id,
    //         looped,
    //     }) {
    //         return Ok(id);
    //     }
    //     return Err(Error::ImpossibleToLoadSound);
    // }

    pub fn play_audio(
        &mut self,
        id: usize,
        looped: bool,
    ) -> anyhow::Result<()> {
        if let Some(sink) = self.sinks.get_mut(&id) {
            if sink.is_paused() {
                sink.play();
            }
            return Ok(());
        }

        if let Some(channel) = self.audios.get(&id) {
            let cur = std::io::Cursor::new(channel.buffer.clone());
            let source = rodio::Decoder::new(BufReader::new(cur)).unwrap();
            let sink = Sink::try_new(&self.stream_handle).unwrap();
            if looped {
                sink.append(source.repeat_infinite());
            }
            else {
                sink.append(source);
            }
            sink.set_volume(channel.volume);
            sink.play();
            self.sinks.insert(id, sink);
        }

        Ok(())
    }

    // pub fn stop_audio(&mut self, id: usize) -> Result<usize, Error> {
    //     if let Ok(()) = self.event_sender.send(AudioEvent::StopAudio { sound_id: id }) {
    //         return Ok(id);
    //     }
    //     return Err(Error::ImpossibleToLoadSound);
    // }

    /// Stop the audio identified with `name`
    pub fn stop_audio(&mut self, id: usize) -> anyhow::Result<()> {
        if let Some(sink) = self.sinks.remove(&id) {
                        
            sink.stop();
            drop(sink);
        }

        self.audios.remove(&id);
        Ok(())
    }

    // pub fn pause_audio(&mut self, id: usize) -> Result<usize, Error> {
    //     if let Ok(()) = self.event_sender.send(AudioEvent::PauseAudio { sound_id: id }) {
    //         return Ok(id);
    //     }
    //     return Err(Error::ImpossibleToLoadSound);
    // }


    /// Pause the audio identified with `name`
    pub fn pause_audio(&mut self, id: usize) -> anyhow::Result<()> {
        if let Some(sink) = self.sinks.get_mut(&id) {
            sink.pause();
        }
        Ok(())
    }

    /// Return the state of the audio identified with `name`
    pub fn audio_is_playing(&mut self, id: usize) -> bool {
        if let Some(sink) = self.sinks.get_mut(&id) {
            return sink.is_paused() || sink.empty();
        }
        false
    }

    /// Set the type of the audio identified with `name`
    pub fn audio_set_type(&mut self, id: usize, sound_type: i32) {
        if let Some(channel) = self.audios.get_mut(&id) {
            channel.sound_type = sound_type;
        }
    }

    /// Set the volume of the audio identified with `name`
    pub fn audio_set_volume(&mut self, id: usize, volume: f32) {
        if let Some(sink) = self.sinks.get_mut(&id) {
            sink.set_volume(volume * self.master_volume);
        }

        if let Some(channel) = self.audios.get_mut(&id) {
            channel.volume = volume;
        }
    }

    pub fn sound_load(&mut self, buffer: Vec<u8>, id: usize, config: PlayConfig) -> anyhow::Result<()> {
        // sound id starts from 4 (after audio channels)
        let sid = id + 4;

        // if this channel is already occupied, stop it
        if let Some(sink) = self.sinks.remove(&sid) {
            sink.stop();
            drop(sink);
        }

        self.audios.remove(&id);
        
        let channel = SoundChannel {
            id,
            buffer,
            path: config.path,
            crossfade: config.crossfade,
            volume: config.volume,
            looped: config.looped,
            sound_type: 0,
        };
        self.audios.insert(id, channel);

        Ok(())
    }

    /// Stop the sound identified with `name`
    pub fn stop_sound(&mut self, id: usize) -> anyhow::Result<()> {
        let sid = id + 4;
        if let Some(sink) = self.sinks.remove(&sid) {
                        
            sink.stop();
            drop(sink);
        }

        self.audios.remove(&id);
        Ok(())
    }

    pub fn play_sound(
        &mut self,
        id: usize,
        looped: bool,
        volume: f32,
    ) -> anyhow::Result<()> {
        // sound id starts from 4 (after audio channels)
        let sid = id + 4;

        if let Some(sink) = self.sinks.get_mut(&sid) {
            if sink.is_paused() {
                sink.play();
            }
            return Ok(());
        }

        if let Some(channel) = self.sounds.get(&id) {
            let cur = std::io::Cursor::new(channel.buffer.clone());
            let source = rodio::Decoder::new(BufReader::new(cur)).unwrap();
            let sink = Sink::try_new(&self.stream_handle).unwrap();
            if looped {
                sink.append(source.repeat_infinite());
            }
            else {
                sink.append(source);
            }
            sink.set_volume(volume);
            sink.play();
            self.sinks.insert(sid, sink);
        }

        Ok(())
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume;
        for (id, sink) in self.sinks.iter_mut() {
            if *id < 4 {
                continue;
            }

            let sid = (id - 4) as i32;
            let sound_type_vol = self.sound_type_volumes
                .get(&sid)
                .unwrap_or(&1.0)
                .to_owned();
            
            sink.set_volume(self.master_volume * sound_type_vol * sink.volume());
        }
    }

    pub fn sound_set_type(&mut self, id: usize, sound_type: i32) {
        if let Some(channel) = self.sounds.get_mut(&id) {
            channel.sound_type = sound_type;
        }
    }

    pub fn sound_set_type_volume(&mut self, sound_type: i32, volume: f32) {
        self.sound_type_volumes.insert(sound_type, volume);
        for (sid, channel) in self.sounds.iter_mut() {
            if channel.sound_type == sound_type {
                if let Some(sink) = self.sinks.get_mut(&(sid + 4)) {
                    sink.set_volume(volume * self.master_volume * sink.volume());
                }
            }
        }
    }

    pub fn sound_set_volume(&mut self, id: usize, volume: f32) {
        if let Some(channel) = self.sounds.get_mut(&id) {
            channel.volume = volume;
            if let Some(sink) = self.sinks.get_mut(&(id + 4)) {
                let sound_type_vol = self.sound_type_volumes
                    .get(&channel.sound_type)
                    .unwrap_or(&1.0)
                    .to_owned();

                sink.set_volume(sink.volume() * self.master_volume * sound_type_vol);
            }
        }
    }
}

/// Error that can be thrown by the AudioPlayer
#[derive(Debug)]
pub enum Error {
    SoundNotRegistered,
    SoundAlreadyExists,
    ImpossibleToLoadSound,
}

#[derive(Debug, Default)]
pub enum Sound {
    #[default]
    Music,
    SoundEffect,
    Video,
}

/// `PlayConfig` describe how sound must be played
pub struct PlayConfig {
    /// Volume of the sound (should be between 0 and 1)
    pub volume: f32,
    /// Should this sound be looped
    pub looped: bool,
    /// Category of the sound. Usefull when you want to be able to change volume of a given category of sounds
    pub category: Sound,
    /// Path of the sound, for debug purpose
    pub path: String,
    /// crossfade duration in milliseconds
    pub crossfade: u32,
}

impl Default for PlayConfig {
    fn default() -> Self {
        Self {
            volume: 0.2,
            looped: false,
            category: Default::default(),
            path: Default::default(),
            crossfade: 0,
        }
    }
}

/// `AudioEvent` represents events send from the audio controller to the Audio Thread
#[allow(dead_code)]
pub(crate) enum AudioEvent {
    LoadAudio {
        buffer: Vec<u8>,
        config: PlayConfig,
        sound_id: usize,
    },
    PlayAudio {
        sound_id: usize,
        looped: bool,
    },
    StopAudio {
        sound_id: usize,
    },
    PauseAudio {
        sound_id: usize,
    },
}

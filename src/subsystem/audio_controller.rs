use crate::subsystem::resources::audio::AudioEvent;
use rodio::{OutputStream, Sink, Source};
use std::collections::HashMap;
use std::io::BufReader;
use std::sync::mpsc::Receiver;
use log::debug;

struct SoundChannel {
    id: usize,
    buffer: Vec<u8>,
    path: String,
    crossfade: u32,
    volume: f32,
    looped: bool,
}

pub(crate) struct AudioController {
    receiver: Receiver<AudioEvent>,
}

impl AudioController {
    pub(crate) fn new(receiver: Receiver<AudioEvent>) -> Self {
        Self { 
            receiver,
        }
    }
}

pub(crate) fn audio_thread(controller: AudioController) {
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let mut sinks: HashMap<usize, Sink> = HashMap::new();
    let mut audios: HashMap<usize, SoundChannel> = HashMap::new();

    loop {
        if let Ok(message) = controller.receiver.try_recv() {
            match message {
                AudioEvent::LoadAudio { buffer, config, sound_id } => {
                    debug!("load sound {}: {}", sound_id, &config.path);

                    // if this channel is already occupied, stop it
                    if let Some(sink) = sinks.remove(&sound_id) {
                        sink.stop();
                        drop(sink);
                    }

                    audios.remove(&sound_id);
                    
                    let channel = SoundChannel {
                        id: sound_id,
                        buffer,
                        path: config.path,
                        crossfade: config.crossfade,
                        volume: config.volume,
                        looped: config.looped,
                    };
                    audios.insert(sound_id, channel);
                    
                }
                AudioEvent::PlayAudio { sound_id, looped } => {

                    if let Some(channel) = audios.get(&sound_id) {
                        let cur = std::io::Cursor::new(channel.buffer.clone());
                        let source = rodio::Decoder::new(BufReader::new(cur)).unwrap();
                        let sink = Sink::try_new(&stream_handle).unwrap();
                        if looped {
                            sink.append(source.repeat_infinite());
                        }
                        else {
                            sink.append(source);
                        }
                        sink.set_volume(channel.volume);
                        sink.play();
                        sinks.insert(sound_id, sink);
                    }
                }
                AudioEvent::StopAudio { sound_id } => {
                    if let Some(sink) = sinks.remove(&sound_id) {
                        
                        sink.stop();
                        drop(sink);
                    }

                    audios.remove(&sound_id);
                }
                AudioEvent::PauseAudio { sound_id } => {
                    if let Some(sink) = sinks.get_mut(&sound_id) {
                        sink.pause();
                    }
                }
            }
        }
    }
}

use anyhow::Result;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::{spawn, JoinHandle};
use std::io::{BufReader, Read, Seek};
use std::ops::Range;
use std::time::Duration;
use fdk_aac::dec::{Decoder, DecoderError, Transport};
use rodio::{OutputStream, Sink, Source};

pub struct MpegAacDecoder<R>
where
    R: Read + Seek,
{
    mp4_reader: mp4::Mp4Reader<R>,
    decoder: Decoder,
    current_pcm_index: usize,
    current_pcm: Vec<i16>,
    track_id: u32,
    position: u32,
}

impl<R> MpegAacDecoder<R>
where
    R: Read + Seek,
{
    pub fn new(reader: R, size: u64) -> Result<MpegAacDecoder<R>, &'static str> {
        let decoder = Decoder::new(Transport::Adts);
        let mp4 = mp4::Mp4Reader::read_header(reader, size).or(Err("Error reading MPEG header"))?;
        let mut track_id: Option<u32> = None;
        let mut video_track_id: Option<u32> = None;
        {
            for (i, track) in mp4.tracks().iter() {
                let media_type = match track.media_type() {
                    Ok(media_type) => media_type,
                    Err(_) => continue,
                };
                if media_type == mp4::MediaType::AAC {
                    if track_id.is_none() {
                        track_id = Some(track.track_id());
                        continue;
                    }
                }
                // else if media_type == mp4::MediaType::
            }
        }
        match track_id {
            Some(track_id) => {
                return Ok(MpegAacDecoder {
                    mp4_reader: mp4,
                    decoder,
                    current_pcm_index: 0,
                    current_pcm: Vec::new(),
                    track_id,
                    position: 1,
                });
            }
            None => {
                return Err("No aac track found");
            }
        }
    }
}

impl<R> Iterator for MpegAacDecoder<R>
where
    R: Read + Seek,
{
    type Item = i16;
    fn next(&mut self) -> Option<i16> {
        if self.current_pcm_index == self.current_pcm.len() {
            let mut pcm = vec![0; 8192];
            let result = match self.decoder.decode_frame(&mut self.current_pcm) {
                Err(DecoderError::NOT_ENOUGH_BITS) => {
                    let sample_result = self.mp4_reader.read_sample(self.track_id, self.position);
                    let sample = sample_result.expect("Error reading sample")?;
                    let tracks = self.mp4_reader.tracks();
                    let track = tracks.get(&self.track_id).expect("No track ID");
                    let adts_header = construct_adts_header(track, &sample).expect("ADTS bytes");
                    let adts_bytes = mp4::Bytes::copy_from_slice(&adts_header);
                    let bytes = [adts_bytes, sample.bytes].concat();
                    self.position += 1;
                    let _bytes_read = match self.decoder.fill(&bytes) {
                        Ok(bytes_read) => bytes_read,
                        Err(_) => return None,
                    };
                    self.decoder.decode_frame(&mut pcm)
                }
                val => val,
            };
            if let Err(err) = result {
                println!("DecoderError: {}", err);
                return None;
            }
            let decoded_fram_size = self.decoder.decoded_frame_size();
            if decoded_fram_size < pcm.len() {
                let _ = pcm.split_off(decoded_fram_size);
            }
            self.current_pcm = pcm;
            self.current_pcm_index = 0;
        }
        let value = self.current_pcm[self.current_pcm_index];
        self.current_pcm_index += 1;
        return Some(value);
    }
}

impl<R> rodio::Source for MpegAacDecoder<R>
where
    R: Read + Seek,
{
    fn current_frame_len(&self) -> Option<usize> {
        let frame_size: usize = self.decoder.decoded_frame_size();
        Some(frame_size)
    }
    fn channels(&self) -> u16 {
        let num_channels: i32 = self.decoder.stream_info().numChannels;
        num_channels as _
    }
    fn sample_rate(&self) -> u32 {
        let sample_rate: i32 = self.decoder.stream_info().sampleRate;
        sample_rate as _
    }
    fn total_duration(&self) -> Option<Duration> {
        return None;
    }
}

fn get_bits(byte: u16, range: Range<u16>) -> u16 {
    let shaved_left = byte << (range.start - 1);
    let moved_back = shaved_left >> (range.start - 1);
    let shave_right = moved_back >> (16 - range.end);
    return shave_right;
}

fn get_bits_u8(byte: u8, range: Range<u8>) -> u8 {
    let shaved_left = byte << (range.start - 1);
    let moved_back = shaved_left >> (range.start - 1);
    let shave_right = moved_back >> (8 - range.end);
    return shave_right;
}

pub fn construct_adts_header(track: &mp4::Mp4Track, sample: &mp4::Mp4Sample) -> Option<Vec<u8>> {
    // B: Only support 0 (MPEG-4)
    // D: Only support 1 (without CRC)
    // byte7 and byte9 not included without CRC
    let adts_header_length = 7;

    //            AAAA_AAAA
    let byte0 = 0b1111_1111;

    //            AAAA_BCCD
    let byte1 = 0b1111_0001;

    //                EEFF_FFGH
    let mut byte2 = 0b0000_0000;
    let object_type = match track.audio_profile() {
        Ok(mp4::AudioObjectType::AacMain) => 1,
        Ok(mp4::AudioObjectType::AacLowComplexity) => 2,
        Ok(mp4::AudioObjectType::AacScalableSampleRate) => 3,
        Ok(mp4::AudioObjectType::AacLongTermPrediction) => 4,
        Ok(_) => {
            log::error!("Unsupported audio object type: {:?}", track.audio_profile());
            return None
        },
        Err(e) => {
            log::error!("Error getting audio object type: {}", e);
            return None
        },
    };
    let adts_object_type = object_type - 1;
    byte2 = (byte2 << 2) | adts_object_type; // EE

    let sample_freq_index = match track.sample_freq_index() {
        Ok(mp4::SampleFreqIndex::Freq96000) => 0,
        Ok(mp4::SampleFreqIndex::Freq88200) => 1,
        Ok(mp4::SampleFreqIndex::Freq64000) => 2,
        Ok(mp4::SampleFreqIndex::Freq48000) => 3,
        Ok(mp4::SampleFreqIndex::Freq44100) => 4,
        Ok(mp4::SampleFreqIndex::Freq32000) => 5,
        Ok(mp4::SampleFreqIndex::Freq24000) => 6,
        Ok(mp4::SampleFreqIndex::Freq22050) => 7,
        Ok(mp4::SampleFreqIndex::Freq16000) => 8,
        Ok(mp4::SampleFreqIndex::Freq12000) => 9,
        Ok(mp4::SampleFreqIndex::Freq11025) => 10,
        Ok(mp4::SampleFreqIndex::Freq8000) => 11,
        Ok(mp4::SampleFreqIndex::Freq7350) => 12,
        // 13-14 = reserved
        // 15 = explicit frequency (forbidden in adts)
        Err(e) => {
            log::error!("Error getting sample frequency index: {}", e);
            return None
        },
    };
    byte2 = (byte2 << 4) | sample_freq_index; // FFFF
    byte2 = (byte2 << 1) | 0b1; // G

    let channel_config = match track.channel_config() {
        // 0 = for when channel config is sent via an inband PCE
        Ok(mp4::ChannelConfig::Mono) => 1,
        Ok(mp4::ChannelConfig::Stereo) => 2,
        Ok(mp4::ChannelConfig::Three) => 3,
        Ok(mp4::ChannelConfig::Four) => 4,
        Ok(mp4::ChannelConfig::Five) => 5,
        Ok(mp4::ChannelConfig::FiveOne) => 6,
        Ok(mp4::ChannelConfig::SevenOne) => 7,
        // 8-15 = reserved
        Err(_) => return None,
    };
    byte2 = (byte2 << 1) | get_bits_u8(channel_config, 6..6); // H

    // HHIJ_KLMM
    let mut byte3 = 0b0000_0000;
    byte3 = (byte3 << 2) | get_bits_u8(channel_config, 7..8); // HH
    byte3 = (byte3 << 4) | 0b1111; // IJKL

    let frame_length = adts_header_length + sample.bytes.len() as u16;
    byte3 = (byte3 << 2) | get_bits(frame_length, 3..5) as u8; // MM

    // MMMM_MMMM
    let byte4 = get_bits(frame_length, 6..13) as u8;

    // MMMO_OOOO
    let mut byte5 = 0b0000_0000;
    byte5 = (byte5 << 3) | get_bits(frame_length, 14..16) as u8;
    byte5 = (byte5 << 5) | 0b11111; // OOOOO

    // OOOO_OOPP
    let mut byte6 = 0b0000_0000;
    byte6 = (byte6 << 6) | 0b111111; // OOOOOO
    byte6 = (byte6 << 2) | 0b00; // PP

    return Some(vec![byte0, byte1, byte2, byte3, byte4, byte5, byte6]);
}

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
        width: u32,
        height: u32,
        should_play: Arc<AtomicBool>,
    ) -> Result<()> {
        let file = std::fs::File::open(&path)?;
        let file_size = file.metadata()?.len();
        let reader = BufReader::new(file);

        let mp4 = mp4::Mp4Reader::read_header(reader, file_size)?;
        

        // self.width = decoder.width();
        // self.height = decoder.height();

        // let mut frame_index = 0;
        // self.playing = true;
        // let mut receive_and_process_decoded_frames =
        //     |decoder: &mut ffmpeg_next::decoder::Video| -> Result<(), ffmpeg_next::Error> {
        //         let mut decoded = Video::empty();
        //         while decoder.receive_frame(&mut decoded).is_ok() {
        //             let mut rgb_frame = Video::empty();
        //             scaler.run(&decoded, &mut rgb_frame)?;
        //             self.render_video_to_texture(&rgb_frame, frame_index);

        //             frame_index += 1;
        //         }
        //         Ok(())
        //     };

        // for (stream, packet) in ctx.packets() {
        //     if !should_play.load(std::sync::atomic::Ordering::Relaxed) {
        //         break;
        //     }

        //     if stream.index() == video_stream_index {
        //         if let Err(e) = decoder.send_packet(&packet) {
        //             log::error!("Error sending packet to decoder: {}", e);
        //         }
        //         if let Err(e) = receive_and_process_decoded_frames(&mut decoder) {
        //             log::error!("Error receiving and processing decoded frames: {}", e);
        //         }
        //     }
        // }

        // if let Err(e) = decoder.send_eof() {
        //     log::error!("Error sending EOF to decoder: {}", e);
        // }
        // if let Err(e) = receive_and_process_decoded_frames(&mut decoder) {
        //     log::error!("Error receiving and processing decoded frames: {}", e);
        // }

        // self.playing = false;
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
            player_thread.join();
        }
    }
}

impl Default for VideoPlayerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use futures::executor::block_on;
    use winit::event::{Event, WindowEvent};

    use super::*;

    #[test]
    fn test_play_audio() {
        env_logger::init();
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/01.mp4"
        ));
        let file = std::fs::File::open(path).expect("Error opening file");
    
        let metadata = file.metadata().unwrap();
        let size = metadata.len();
        let buf = BufReader::new(file);
    
        let decoder = MpegAacDecoder::new(buf, size).unwrap();
    
        let output_stream = OutputStream::try_default();
        let (_stream, handle) = output_stream.expect("Error creating output stream");
        let sink = Sink::try_new(&handle).expect("Error creating sink");
    
        sink.append(decoder);
        sink.play();
        sink.set_volume(0.5);
        sink.sleep_until_end();
    }

    // #[test]
    // fn test_video_player() {
    //     let event_loop = winit::event_loop::EventLoop::new().expect("Event loop could not be created");
    //     event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    //     // 1024 × 640
    //     let window_builder: winit::window::WindowBuilder = winit::window::WindowBuilder::new()
    //         .with_title("app".to_string())
    //         .with_inner_size(winit::dpi::LogicalSize::new(1024, 640));

    //     let window = window_builder
    //         .build(&event_loop)
    //         .expect("An error occured while building the main game window");
        
    //     let vm = VideoPlayerManager::new();

    //     // init wgpu
    //     let backend = wgpu::util::backend_bits_from_env().unwrap_or_else(wgpu::Backends::all);
    //     let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
    //         backends: backend,
    //         dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
    //         flags: wgpu::InstanceFlags::default(),
    //         gles_minor_version: wgpu::Gles3MinorVersion::Automatic
    //     });

    //     let (_size, surface) = unsafe {
    //         let size = window.inner_size();
    //         let surface = instance.create_surface(window).expect("Surface unsupported by adapter");
    //         (size, surface)
    //     };

    //     //render video
    //     event_loop
    //         .run(move |event, target| {
    //             // Have the closure take ownership of the resources.
    //             // `event_loop.run` never returns, therefore we must do this to ensure
    //             // the resources are properly cleaned up.

    //             if let Event::WindowEvent { window_id, event } = event {
    //                 match event {
    //                     WindowEvent::Resized(new_size) => {
    //                     }
    //                     WindowEvent::RedrawRequested => {


    //                     }
    //                     WindowEvent::CloseRequested => {
    //                     }
    //                     _ => {}
    //                 }
    //             }
    //         })
    //         .unwrap();

    // }
}

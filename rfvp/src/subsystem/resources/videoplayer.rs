use anyhow::Result;
use byteorder::{BigEndian, ByteOrder};
use bytes::{Buf, BytesMut};
use fdk_aac::dec::{Decoder, DecoderError, Transport};
use mp4::{Mp4Track, TrackType};
use rodio::{OutputStream, Sink, Source};
use std::io::{BufReader, Read, Seek};
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{spawn, JoinHandle};
use std::time::Duration;

const BUF_SIZE: usize = 3;

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

        {
            for (_, track) in mp4.tracks().iter() {
                let media_type = match track.media_type() {
                    Ok(media_type) => media_type,
                    Err(_) => continue,
                };
                if media_type == mp4::MediaType::AAC && track_id.is_none() {
                    track_id = Some(track.track_id());
                    continue;
                }
            }
        }
        match track_id {
            Some(track_id) => Ok(MpegAacDecoder {
                mp4_reader: mp4,
                decoder,
                current_pcm_index: 0,
                current_pcm: Vec::new(),
                track_id,
                position: 1,
            }),
            _ => {
                let msg = format!("No AAC track found, AAC: {:?}", track_id);
                log::error!("{}", msg);
                Err("No AAC track found")
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
        Some(value)
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
        None
    }
}

fn get_bits(byte: u16, range: Range<u16>) -> u16 {
    let shaved_left = byte << (range.start - 1);
    let moved_back = shaved_left >> (range.start - 1);
    let shave_right = moved_back >> (16 - range.end);
    shave_right
}

fn get_bits_u8(byte: u8, range: Range<u8>) -> u8 {
    let shaved_left = byte << (range.start - 1);
    let moved_back = shaved_left >> (range.start - 1);
    let shave_right = moved_back >> (8 - range.end);
    shave_right
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
            return None;
        }
        Err(e) => {
            log::error!("Error getting audio object type: {}", e);
            return None;
        }
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
            return None;
        }
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

    Some(vec![byte0, byte1, byte2, byte3, byte4, byte5, byte6])
}

enum DecoderMessage {
    Frame(Vec<u8>),
    Stop,
}

struct VideoFrame {
    buffer: Vec<u8>,
    width: usize,
    height: usize,
}

/// Network abstraction layer type for H264 pocket we might find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NalType {
    Unspecified = 0,
    Slice = 1,
    Dpa = 2,
    Dpb = 3,
    Dpc = 4,
    IdrSlice = 5,
    Sei = 6,
    Sps = 7,
    Pps = 8,
    Aud = 9,
    EndSequence = 10,
    EndStream = 11,
    FillerData = 12,
    SpsExt = 13,
    Prefix = 14,
    SubSps = 15,
    DPS = 16,
    Reserved17 = 17,
    Reserved18 = 18,
    AuxiliarySlice = 19,
    ExtenSlice = 20,
    DepthExtenSlice = 21,
    Reserved22 = 22,
    Reserved23 = 23,
    Unspecified24 = 24,
    Unspecified25 = 25,
    Unspecified26 = 26,
    Unspecified27 = 27,
    Unspecified28 = 28,
    Unspecified29 = 29,
    Unspecified30 = 30,
    Unspecified31 = 31,
}

impl From<u8> for NalType {
    /// Reads NAL from header byte.
    fn from(value: u8) -> Self {
        use NalType::{
            Aud, AuxiliarySlice, DepthExtenSlice, Dpa, Dpb, Dpc, EndSequence, EndStream,
            ExtenSlice, FillerData, IdrSlice, Pps, Prefix, Reserved17, Reserved18, Reserved22,
            Reserved23, Sei, Slice, Sps, SpsExt, SubSps, Unspecified, Unspecified24, Unspecified25,
            Unspecified26, Unspecified27, Unspecified28, Unspecified29, Unspecified30,
            Unspecified31, DPS,
        };

        match value {
            0 => Unspecified,
            1 => Slice,
            2 => Dpa,
            3 => Dpb,
            4 => Dpc,
            5 => IdrSlice,
            6 => Sei,
            7 => Sps,
            8 => Pps,
            9 => Aud,
            10 => EndSequence,
            11 => EndStream,
            12 => FillerData,
            13 => SpsExt,
            14 => Prefix,
            15 => SubSps,
            16 => DPS,
            17 => Reserved17,
            18 => Reserved18,
            19 => AuxiliarySlice,
            20 => ExtenSlice,
            21 => DepthExtenSlice,
            22 => Reserved22,
            23 => Reserved23,
            24 => Unspecified24,
            25 => Unspecified25,
            26 => Unspecified26,
            27 => Unspecified27,
            28 => Unspecified28,
            29 => Unspecified29,
            30 => Unspecified30,
            31 => Unspecified31,
            _ => panic!("Invalid NAL type"),
        }
    }
}

/// A NAL unit in a bitstream.
struct NalUnit<'a> {
    nal_type: NalType,
    bytes: &'a [u8],
}

impl<'a> NalUnit<'a> {
    /// Reads a NAL unit from a slice of bytes in MP4, returning the unit, and the remaining stream after that slice.
    fn from_stream(mut stream: &'a [u8], length_size: u8) -> Option<(Self, &[u8])> {
        let mut nal_size = 0;

        // Construct nal_size from first bytes in MP4 stream.
        for _ in 0..length_size {
            nal_size = (nal_size << 8) | u32::from(stream[0]);
            stream = &stream[1..];
        }

        if nal_size == 0 {
            return None;
        }

        let packet = &stream[..nal_size as usize];
        let nal_type = NalType::from(packet[0] & 0x1F);
        let unit = NalUnit {
            nal_type,
            bytes: packet,
        };

        stream = &stream[nal_size as usize..];

        Some((unit, stream))
    }

    #[allow(unused)]
    fn nal_type(&self) -> NalType {
        self.nal_type
    }

    #[allow(unused)]
    fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// Converter from NAL units from the MP4 to the Annex B format expected by openh264.
///
/// It also inserts SPS and PPS units from the MP4 header into the stream.
/// They are also required for Annex B format to be decodable, but are not present in the MP4 bitstream,
/// as they are stored in the headers.
pub struct Mp4BitstreamConverter {
    length_size: u8,
    sps: Vec<Vec<u8>>,
    pps: Vec<Vec<u8>>,
    new_idr: bool,
    sps_seen: bool,
    pps_seen: bool,
}

impl Mp4BitstreamConverter {
    /// Create a new converter for the given track.
    ///
    /// The track must contain an AVC1 configuration.
    /// The track must contain an AVC1 configuration.
    pub fn for_mp4_track(track: &Mp4Track) -> Result<Self, anyhow::Error> {
        let avcc_config = &track
            .trak
            .mdia
            .minf
            .stbl
            .stsd
            .avc1
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Track does not contain AVC1 config"))?
            .avcc;

        Ok(Self {
            length_size: avcc_config.length_size_minus_one + 1,
            sps: avcc_config
                .sequence_parameter_sets
                .iter()
                .cloned()
                .map(|v| v.bytes)
                .collect(),
            pps: avcc_config
                .picture_parameter_sets
                .iter()
                .cloned()
                .map(|v| v.bytes)
                .collect(),
            new_idr: true,
            sps_seen: false,
            pps_seen: false,
        })
    }

    /// Convert a single packet from the MP4 format to the Annex B format.
    ///
    /// It clears the `out` vector and appends the converted packet to it.
    pub fn convert_packet(&mut self, packet: &[u8], out: &mut Vec<u8>) {
        let mut stream = packet;
        out.clear();

        while !stream.is_empty() {
            let Some((unit, remaining_stream)) = NalUnit::from_stream(stream, self.length_size)
            else {
                continue;
            };

            stream = remaining_stream;

            match unit.nal_type {
                NalType::Sps => self.sps_seen = true,
                NalType::Pps => self.pps_seen = true,
                NalType::IdrSlice => {
                    // If this is a new IDR picture following an IDR picture, reset the idr flag.
                    // Just check first_mb_in_slice to be 1
                    if !self.new_idr && unit.bytes[1] & 0x80 != 0 {
                        self.new_idr = true;
                    }
                    // insert SPS & PPS NAL units if they were not seen
                    if self.new_idr && !self.sps_seen && !self.pps_seen {
                        self.new_idr = false;
                        for sps in self.sps.iter() {
                            out.extend([0, 0, 1]);
                            out.extend(sps);
                        }
                        for pps in self.pps.iter() {
                            out.extend([0, 0, 1]);
                            out.extend(pps);
                        }
                    }
                    // insert only PPS if SPS was seen
                    if self.new_idr && self.sps_seen && !self.pps_seen {
                        for pps in self.pps.iter() {
                            out.extend([0, 0, 1]);
                            out.extend(pps);
                        }
                    }
                }
                _ => {}
            }

            out.extend([0, 0, 1]);
            out.extend(unit.bytes);

            if !self.new_idr && unit.nal_type == NalType::Slice {
                self.new_idr = true;
                self.sps_seen = false;
                self.pps_seen = false;
            }
        }
    }
}

pub struct MpegVideoDecoder {
    audio_decoder: MpegAacDecoder<BufReader<std::fs::File>>,
    video_decoder: openh264::decoder::Decoder,
    bitstream_converter: Mp4BitstreamConverter,
    mp4: mp4::Mp4Reader<BufReader<std::fs::File>>,
    width: u16,
    height: u16,
    track_id: u32,
    frame_rate: f64,
}

impl MpegVideoDecoder {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::File::open(&path)?;
        let metadata = file.metadata()?;
        let size = metadata.len();
        let reader = BufReader::new(file);
        let audio_decoder = match MpegAacDecoder::new(reader, size) {
            Ok(decoder) => decoder,
            Err(e) => {
                log::error!("Error creating audio decoder: {:?}", e);
                return Err(anyhow::anyhow!("Error creating audio decoder"));
            }
        };

        let (mp4, track_id) = Self::read_mp4_video(&path)?;
        let tracks = mp4.tracks();
        let track = tracks.get(&track_id).expect("No video track ID");
        let bitstream_converter = Mp4BitstreamConverter::for_mp4_track(track)?;
        let decoder = openh264::decoder::Decoder::new()?;
        let width = track.width();
        let height = track.height();
        let track_id = track.track_id();
        let frame_rate = track.frame_rate();

        Ok(Self {
            audio_decoder,
            video_decoder: decoder,
            bitstream_converter,
            mp4,
            width,
            height,
            track_id,
            frame_rate,
        })
    }

    fn read_mp4_video(
        path: impl AsRef<Path>,
    ) -> Result<(mp4::Mp4Reader<BufReader<std::fs::File>>, u32)> {
        let file = std::fs::File::open(&path)?;
        let metadata = file.metadata()?;
        let size = metadata.len();
        let reader = BufReader::new(file);
        let mp4 = mp4::Mp4Reader::read_header(reader, size)?;

        let track = mp4
            .tracks()
            .iter()
            .find(|(_, t)| t.media_type().unwrap() == mp4::MediaType::H264)
            .ok_or_else(|| anyhow::anyhow!("No avc1 track"))?
            .1;

        let track_id = track.track_id();

        Ok((mp4, track_id))
    }

    pub fn take_frame(
        &mut self,
        elapsed: u64,
    ) -> anyhow::Result<Option<image::ImageBuffer<image::Rgba<u8>, std::vec::Vec<u8>>>> {
        // calculate the frame index based on the elapsed time
        let frame_index = (elapsed as f64 * self.frame_rate) as u32 + 1;
        let frame_index = 1;

        let sample = match self.mp4.read_sample(self.track_id, frame_index) {
            Ok(Some(sample)) => sample,
            Ok(None) => {
                println!("No sample found");
                return Ok(None);
            }
            Err(e) => {
                log::error!("Error reading sample: {}", e);
                return Err(anyhow::anyhow!("Error reading sample"));
            }
        };

        let mut buffer = Vec::new();
        // convert the packet from mp4 representation to one that openh264 can decode
        self.bitstream_converter
            .convert_packet(&sample.bytes, &mut buffer);

        match self.video_decoder.decode(&buffer) {
            Ok(Some(mut image)) => {
                let mut rgb = vec![0; self.width as usize * self.height as usize * 3];
                image.write_rgb8(&mut rgb);
                let mut image = image::RgbImage::new(self.width as u32, self.height as u32);
                image.copy_from_slice(&rgb);
                let image = image::DynamicImage::ImageRgb8(image);
                let rbga8 = image.to_rgba8();
                return Ok(Some(rbga8));
            },
            Ok(None) => {
                println!("No frame found");
                return Ok(None);
            },
            Err(e) => {
                log::error!("Error decoding frame: {}", e);
                return Err(anyhow::anyhow!("Error decoding frame"));
            }
        }

        Ok(None)
    }
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
    use std::{env, time::Instant};

    use futures::executor::block_on;
    use wgpu::{CompositeAlphaMode, SamplerBindingType, SurfaceConfiguration, TextureFormat};
    use winit::event::{Event, WindowEvent};

    use super::*;

    #[test]
    fn test_play_audio() {
        env_logger::init();
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../video_player/testcase/01.mp4"
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
}

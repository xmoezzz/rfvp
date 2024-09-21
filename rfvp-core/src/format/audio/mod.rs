//! Support for NXA format, storing opus audio. (The format might be specific for Nintendo Switch?)
//!
//! It is a simple container storing opus frames mostly as-is. The only addition compared to usual opus formats are loop points.
//!
//! The header specifies loop start and loop end points in samples. When looping is enabled and loop end is reached, the decoder seeks to the loop start.

mod audio_source;

use std::io::Read;

use anyhow::{bail, Result};
pub use audio_source::{AudioBuffer, AudioFrameSource, AudioSource};
use binrw::{BinRead, BinWrite};
use opus::Channels;

#[derive(BinRead, BinWrite, Debug)]
#[brw(little, magic = b"NXA1")]
#[br(assert(version == 2))]
struct NxaHeader {
    version: u32,
    file_size: u32,
    #[brw(align_after = 0x10)]
    info: AudioInfo,
}

/// Provides some metadata about the audio
///
/// Stuff that would usually go into opus headers
#[derive(BinRead, BinWrite, Debug, Clone)]
pub struct AudioInfo {
    /// Sample rate, in Hz.
    pub sample_rate: u32,
    /// Number of channels (usually 1 or 2).
    pub channel_count: u16,
    /// Size of frame in bytes
    pub frame_size: u16,
    /// Amount of samples in one frame
    pub frame_samples: u16,
    /// Amount of samples to skip, needed because of the way opus works
    pub pre_skip: u16,
    /// Amount of samples in the whole file
    pub num_samples: u32,
    /// Where to start playing after looping in samples.
    pub loop_start: u32,
    /// Where to loop from in samples (usually happens at the end of the file).
    pub loop_end: u32,
}

// A fully in-memory, but not yet decoded audio file
pub struct AudioFile {
    info: AudioInfo,
    data: Vec<u8>,
}

impl AudioFile {
    pub fn info(&self) -> &AudioInfo {
        &self.info
    }

    pub fn decode(self) -> Result<AudioDecoder<Self>> {
        AudioDecoder::new(self)
    }

    pub fn read_frames(self) -> AudioFileFrameReader<Self> {
        AudioFileFrameReader::new(self)
    }
}

impl AsRef<AudioFile> for AudioFile {
    fn as_ref(&self) -> &AudioFile {
        self
    }
}

pub struct AudioFileFrameReader<F: AsRef<AudioFile>> {
    file: F,
    bytes_position: usize,
}

impl<F: AsRef<AudioFile>> AudioFileFrameReader<F> {
    pub fn new(file: F) -> Self {
        Self {
            file,
            bytes_position: 0,
        }
    }

    pub fn audio_info(&self) -> &AudioInfo {
        &self.file.as_ref().info
    }

    pub fn frame_size(&self) -> usize {
        self.audio_info().frame_size as usize
    }

    fn frame_samples(&self) -> usize {
        self.audio_info().frame_samples as usize
    }

    pub fn frames_position(&self) -> usize {
        self.bytes_position / self.frame_size()
    }

    pub fn seek_to_frames(&mut self, new_frames_position: usize) {
        self.bytes_position = self.frame_size() * new_frames_position;
    }

    pub fn get_next_frame(&mut self) -> Option<&[u8]> {
        let data = &self.file.as_ref().data;
        if self.bytes_position >= data.len() {
            return None;
        }

        let data = &data[self.bytes_position..][..self.frame_size()];

        self.bytes_position += self.frame_size();

        Some(data)
    }

    pub fn has_next_frame(&self) -> bool {
        self.bytes_position < self.file.as_ref().data.len()
    }
}

pub struct AudioDecoder<F: AsRef<AudioFile>> {
    frame_iter: AudioFileFrameReader<F>,
    buffer: Box<[f32]>,
    decoder: opus::Decoder,
}

impl<F: AsRef<AudioFile>> AudioDecoder<F> {
    pub fn new(file: F) -> Result<Self> {
        let info = &file.as_ref().info;
        let decoder = opus::Decoder::new(
            info.sample_rate,
            match info.channel_count {
                1 => Channels::Mono,
                2 => Channels::Stereo,
                _ => panic!("Unsupported channel count"),
            },
        )?;
        let buffer =
            vec![0.0; info.frame_samples as usize * info.channel_count as usize].into_boxed_slice();
        Ok(Self {
            frame_iter: AudioFileFrameReader::new(file),
            buffer,
            decoder,
        })
    }

    pub fn audio_info(&self) -> &AudioInfo {
        self.frame_iter.audio_info()
    }

    fn frame_samples(&self) -> usize {
        self.frame_iter.frame_samples()
    }
}

impl<F: AsRef<AudioFile>> AudioFrameSource for AudioDecoder<F> {
    fn max_frame_size(&self) -> usize {
        self.audio_info().frame_samples as usize
    }

    fn sample_rate(&self) -> u32 {
        self.audio_info().sample_rate
    }

    fn pre_skip(&self) -> u32 {
        self.audio_info().pre_skip as u32
    }

    fn pre_roll(&self) -> u32 {
        // the decoder needs some time to converge, we probably should seek a little bit before and skip some samples
        // this is called "pre-roll" by the RFC7845 and recommends to use 3840 samples / 80 ms
        const PRE_ROLL: u32 = 3840;

        PRE_ROLL
    }

    fn read_frame(&mut self, destination: &mut AudioBuffer) -> bool {
        // copy the important info to not annoy the borrow checker below
        let &AudioInfo {
            frame_samples,
            channel_count: channels,
            ..
        } = self.audio_info();

        let Some(data) = self.frame_iter.get_next_frame() else {
            return false;
        };

        assert_eq!(
            self.decoder.get_nb_samples(data).unwrap(),
            frame_samples as usize
        );

        let decoded = self
            .decoder
            .decode_float(data, &mut self.buffer, false)
            .unwrap();

        assert_eq!(decoded, frame_samples as usize);

        match channels {
            1 => {
                for &sample in self.buffer.iter() {
                    destination.push((sample, sample));
                }
            }
            2 => {
                for sample in self.buffer.chunks_exact(2) {
                    destination.push((sample[0], sample[1]));
                }
            }
            _ => panic!("Unsupported channel count: {}", channels),
        }

        true
    }

    fn samples_seek(&mut self, samples_position: u32) -> Result<u32> {
        if samples_position > self.audio_info().num_samples {
            bail!(
                "Seek position {} is out of bounds (the file is {} samples)",
                samples_position,
                self.audio_info().num_samples
            );
        }

        let samples_position = samples_position as usize;

        let frames_position = samples_position / self.frame_samples();
        let in_frame_position = samples_position % self.frame_samples();

        self.frame_iter.seek_to_frames(frames_position);
        self.decoder.reset_state().unwrap();

        Ok(in_frame_position.try_into().unwrap())
    }

    fn current_sample_position(&self) -> u32 {
        (self.frame_iter.frames_position() * self.frame_samples()) as u32
    }
}

pub fn read_audio(data: &[u8]) -> Result<AudioFile> {
    let mut cur = std::io::Cursor::new(data);
    let header = NxaHeader::read_le(&mut cur)?;

    assert_eq!(header.file_size, data.len() as u32);
    // how are we supposed to loop when the loop end is not in the end of the file?
    assert_eq!(header.info.loop_end, header.info.num_samples);

    let mut data = Vec::new();
    cur.read_to_end(&mut data)?;

    Ok(AudioFile {
        info: header.info,
        data,
    })
}

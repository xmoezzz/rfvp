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
    pub fn new(info: AudioInfo, data: Vec<u8>) -> Self {
        Self { info, data }
    }
    
    pub fn info(&self) -> &AudioInfo {
        &self.info
    }

    pub fn decode(self) -> Result<AudioDecoder<Self>> {
        AudioDecoder::new(self)
    }
}

impl AsRef<AudioFile> for AudioFile {
    fn as_ref(&self) -> &AudioFile {
        self
    }
}

pub struct AudioDecoder<F: AsRef<AudioFile>> {
    file: F,
    bytes_position: usize,
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
            file,
            bytes_position: 0,
            buffer,
            decoder,
        })
    }

    pub fn audio_info(&self) -> &AudioInfo {
        &self.file.as_ref().info
    }

    fn frame_size(&self) -> u32 {
        self.audio_info().frame_size as u32
    }

    fn frame_samples(&self) -> u32 {
        self.audio_info().frame_samples as u32
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
        let data = &self.file.as_ref().data;
        if self.bytes_position >= data.len() {
            return false;
        }

        let data = &data[self.bytes_position..][..self.frame_size() as usize];

        assert_eq!(
            self.decoder.get_nb_samples(data).unwrap() as u32,
            self.frame_samples()
        );

        let decoded = self
            .decoder
            .decode_float(data, &mut self.buffer, false)
            .unwrap() as u32;

        assert_eq!(decoded, self.frame_samples());

        self.bytes_position += self.frame_size() as usize;

        let channels = self.audio_info().channel_count;

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

        let frames_position = samples_position / self.frame_samples();
        let bytes_position = frames_position * self.frame_size();
        let in_frame_position = samples_position % self.frame_samples();

        self.bytes_position = bytes_position.try_into().unwrap();
        self.decoder.reset_state().unwrap();

        Ok(in_frame_position)
    }

    fn current_sample_position(&self) -> u32 {
        (self.bytes_position / self.frame_size() as usize) as u32 * self.frame_samples()
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

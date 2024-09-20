use anyhow::Result;

type Sample = (f32, f32);

/// Represents a storage for audio frames
pub struct AudioBuffer {
    // only stereo is supported
    data: Vec<Sample>,
}

impl AudioBuffer {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn push(&mut self, frame: Sample) {
        self.data.push(frame);
    }
}

/// Stores an [`AudioBuffer`] and a position in it
struct AudioBufferReader {
    buffer: AudioBuffer,
    position: u32,
}

impl AudioBufferReader {
    pub fn new(buffer: AudioBuffer) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.position = 0;
    }

    #[allow(unused)]
    pub fn inner(&self) -> &AudioBuffer {
        &self.buffer
    }

    pub fn inner_mut(&mut self) -> &mut AudioBuffer {
        &mut self.buffer
    }

    pub fn remaining(&self) -> u32 {
        self.buffer.len() as u32 - self.position
    }

    /// Skip `count` samples, return the remaining number of samples to skip (the buffer may not be long enough)
    pub fn skip_samples(&mut self, count: u32) -> u32 {
        // take the buffer length into account!
        let remaining = self.buffer.len() as u32 - self.position;
        if remaining >= count {
            self.position += count;
            0
        } else {
            self.position = self.buffer.len().try_into().unwrap();
            count - remaining
        }
    }

    fn read_sample(&mut self) -> Option<Sample> {
        let sample = self.buffer.data.get(self.position as usize)?;
        self.position += 1;
        Some(*sample)
    }
}

/// An audio source providing audio in frames
pub trait AudioFrameSource {
    /// Maximum size of the frame
    ///
    /// Used to allocate the audio buffer
    fn max_frame_size(&self) -> usize;
    /// Sample rate, in Hz
    fn sample_rate(&self) -> u32;
    /// Number of samples to skip at the beginning of the file
    fn pre_skip(&self) -> u32;
    /// Number of samples to pre-roll after seeking
    fn pre_roll(&self) -> u32;

    /// Read & decode one frame into the buffer
    ///
    /// Returns `true` if the frame was read successfully, `false` if the end of the file was reached
    fn read_frame(&mut self, destination: &mut AudioBuffer) -> bool;

    /// Seeks to the frame corresponding to the sample position
    ///
    /// Returns the sample offset in the frame (this will have to be accounted for by the caller)
    ///
    /// The sample position does not take the pre-skip into account, meaning to seek to the first sample of the file, the caller should pass the `pre_skip`
    fn samples_seek(&mut self, sample_position: u32) -> Result<u32>;
    /// Returns the number of the first sample in the next frame
    fn current_sample_position(&self) -> u32;
}

/// A wrapper around an [`AudioFrameSource`] that provides a sample-based interface
pub struct AudioSource<S: AudioFrameSource> {
    source: S,
    reader: AudioBufferReader,
    skip_left: u32,
}

impl<S: AudioFrameSource> AudioSource<S> {
    pub fn new(source: S) -> Self {
        let buffer_capacity = source.max_frame_size();
        let pre_skip = source.pre_skip();

        Self {
            source,
            reader: AudioBufferReader::new(AudioBuffer::with_capacity(buffer_capacity)),
            skip_left: pre_skip,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.source.sample_rate()
    }

    /// Seek to the sample position, taking the pre-skip into account (to seek to the first sample of the file, pass 0)
    pub fn samples_seek(&mut self, sample_position: u32) -> Result<()> {
        self.reader.clear();
        let pre_skip = self.source.pre_skip();
        let pre_roll = self.source.pre_roll();

        // compute the "raw" position (without pre-skip removed)
        let raw_position = sample_position + pre_skip;

        // compute the pre-roll (the number of samples to skip after seeking, to make sure the decoder converges)
        let pre_roll = std::cmp::min(pre_roll, raw_position);

        // seek to the raw position, minus the pre-roll, but include the pre-roll in the skip
        let skip = self.source.samples_seek(raw_position - pre_roll)? + pre_roll;
        self.skip_left = self.reader.skip_samples(skip);
        Ok(())
    }

    pub fn read_sample(&mut self) -> Option<Sample> {
        if self.skip_left > 0 {
            self.skip_left = self.reader.skip_samples(self.skip_left);
        }

        match self.reader.read_sample() {
            Some(sample) => Some(sample),
            None => {
                self.reader.clear();
                if !self.source.read_frame(self.reader.inner_mut()) {
                    return None;
                }
                self.read_sample()
            }
        }
    }

    /// Return the position of the next sample to be read
    pub fn current_samples_position(&self) -> u32 {
        self.source.current_sample_position()
            // these samples will be skipped
            + self.skip_left
            // take into account the samples not yet read from the buffer (but read from the source)
            - self.reader.remaining()
            // remove the pre-skip
            - self.source.pre_skip()
    }

    pub fn inner(&self) -> &S {
        &self.source
    }
}

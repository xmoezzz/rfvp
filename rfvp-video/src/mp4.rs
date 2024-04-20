use std::{
    io::{Read, Seek, SeekFrom},
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use mp4::{Mp4Sample, Mp4Track};

pub type Mp4ReadStream = std::fs::File;
pub type Mp4Reader<S> = Arc<Mutex<mp4::Mp4Reader<S>>>;

pub struct Mp4TrackReader<S: Read + Seek> {
    mp4: Mp4Reader<S>,
    track_id: u32,
    samples_position: u32,
    samples_count: u32,
}

impl<S: Read + Seek> Mp4TrackReader<S> {
    pub fn new(mp4: Mp4Reader<S>, track_id: u32) -> Result<Self> {
        let mp4_guard = mp4.lock().unwrap();

        let track = mp4_guard
            .tracks()
            .get(&track_id)
            .context("Could not find specified track")?;

        let samples_count = track.sample_count();

        drop(mp4_guard);

        Ok(Self {
            mp4,
            track_id,
            samples_position: 1,
            samples_count,
        })
    }

    pub fn get_mp4_track_info<R>(&self, f: impl FnOnce(&Mp4Track) -> R) -> R {
        let mp4 = self.mp4.lock().unwrap();
        let track = mp4.tracks().get(&self.track_id).unwrap();
        f(track)
    }

    pub fn next_sample(&mut self) -> Result<Option<Mp4Sample>> {
        if self.samples_position > self.samples_count {
            return Ok(None);
        }

        let mut mp4 = self.mp4.lock().unwrap();
        let sample = mp4
            .read_sample(self.track_id, self.samples_position)
            .with_context(|| {
                format!(
                    "Reading sample {} of track {}",
                    self.samples_position, self.track_id
                )
            })?
            .ok_or_else(|| anyhow!("mp4 crate indicated end-of-stream, while we expected to be able to read this sample"))?;

        self.samples_position += 1;

        Ok(Some(sample))
    }
}

impl<S: Read + Seek> Clone for Mp4TrackReader<S> {
    fn clone(&self) -> Self {
        Self {
            mp4: self.mp4.clone(),
            track_id: self.track_id,
            samples_position: self.samples_position,
            samples_count: self.samples_count,
        }
    }
}

fn stream_len(stream: &mut impl Seek) -> Result<u64> {
    let old_pos = stream.stream_position()?;
    let len = stream.seek(SeekFrom::End(0))?;

    // Avoid seeking a third time when we were already at the end of the
    // stream. The branch is usually way cheaper than a seek operation.
    if old_pos != len {
        stream.seek(SeekFrom::Start(old_pos))?;
    }

    Ok(len)
}

pub struct Mp4<S: Read + Seek> {
    pub reader: Mp4Reader<S>,
    pub video_track: Mp4TrackReader<S>,
    pub audio_track: Option<Mp4TrackReader<S>>,
}

impl<S: Read + Seek> Mp4<S> {
    pub fn new(mut reader: S) -> Result<Self> {
        let size = stream_len(&mut reader).context("Getting the length of a stream")?;
        let mp4 =
            mp4::Mp4Reader::read_header(reader, size).context("Reading the MP4 file headers")?;

        let tracks = mp4
            .tracks()
            .iter()
            .map(|(_, track)| -> Result<_> {
                let ty = track.track_type()?;
                Ok((track.track_id(), ty))
            })
            .collect::<Result<Vec<_>>>()?;

        let video_track_id = tracks
            .iter()
            .find(|(_, ty)| *ty == mp4::TrackType::Video)
            .map(|(id, _)| *id)
            .ok_or_else(|| anyhow::anyhow!("No video track found"))?;

        let audio_track_id = tracks
            .iter()
            .find(|(_, ty)| *ty == mp4::TrackType::Audio)
            .map(|(id, _)| *id);

        let reader = Arc::new(Mutex::new(mp4));

        let video_track = Mp4TrackReader::new(reader.clone(), video_track_id)
            .context("Opening mp4 video track")?;
        let audio_track = audio_track_id
            .map(|audio_track_id| {
                Mp4TrackReader::new(reader.clone(), audio_track_id)
                    .context("Opening mp4 video track")
            })
            .transpose()?;

        Ok(Self {
            reader,
            video_track,
            audio_track,
        })
    }
}

impl<S: Read + Seek> Clone for Mp4<S> {
    fn clone(&self) -> Self {
        Self {
            reader: self.reader.clone(),
            video_track: self.video_track.clone(),
            audio_track: self.audio_track.clone(),
        }
    }
}

use std::io::{Read, Seek};

use anyhow::Context;
use rfvp_core::format::audio::{AudioBuffer, AudioFrameSource};
use symphonia::core::{
    audio::{AudioBufferRef, Signal},
    codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_AAC},
    formats::Packet,
};

use crate::mp4::Mp4TrackReader;

pub struct AacFrameSource<S: Read + Seek> {
    track: Mp4TrackReader<S>,
    decoder: symphonia::default::codecs::AacDecoder,
    samples_position: u32,
}

impl<S: Read + Seek> AacFrameSource<S> {
    pub fn new(track: Mp4TrackReader<S>) -> anyhow::Result<Self> {
        let mp4a = track
            .get_mp4_track_info(|t| t.trak.mdia.minf.stbl.stsd.mp4a.clone())
            .context("Could not find mp4a atom")?;

        let mut codec_params = CodecParameters::new();
        codec_params
            .for_codec(CODEC_TYPE_AAC)
            .with_sample_rate(mp4a.samplerate.value() as u32);

        if let Some(esds) = mp4a.esds {
            let dec_specific = esds.es_desc.dec_config.dec_specific;

            let dec_specific_data = vec![
                // manually serialize the decoder specific info
                (dec_specific.profile << 3) + (dec_specific.freq_index >> 1),
                (dec_specific.freq_index << 7) + (dec_specific.chan_conf << 3),
            ];

            codec_params.extra_data = Some(dec_specific_data.into_boxed_slice());
        };

        let decoder = symphonia::default::codecs::AacDecoder::try_new(
            &codec_params,
            &DecoderOptions::default(),
        )
        .context("Creating AAC decoder")?;

        Ok(Self {
            track,
            decoder,
            samples_position: 0,
        })
    }
}

impl<S: Read + Seek> AudioFrameSource for AacFrameSource<S> {
    fn max_frame_size(&self) -> usize {
        self.decoder.last_decoded().capacity()
    }

    fn sample_rate(&self) -> u32 {
        self.decoder
            .codec_params()
            .sample_rate
            .expect("AAC sample rate")
    }

    fn pre_skip(&self) -> u32 {
        2112
    }

    fn pre_roll(&self) -> u32 {
        2112
    }

    fn read_frame(&mut self, destination: &mut AudioBuffer) -> bool {
        if let Some(frame) = self.track.next_sample().expect("Reading next sample") {
            let packet =
                Packet::new_from_slice(1, frame.start_time, frame.duration as u64, &frame.bytes);
            let buffer = self.decoder.decode(&packet).expect("Decoding AAC frame");

            let AudioBufferRef::F32(buffer) = buffer else {
                unreachable!()
            };

            let &[left, right] = buffer.planes().planes() else {
                panic!("Expected stereo audio")
            };

            for (&l, &r) in left.iter().zip(right.iter()) {
                destination.push((l, r));
            }

            self.samples_position += buffer.frames() as u32;

            true
        } else {
            false
        }
    }

    fn samples_seek(&mut self, _sample_position: u32) -> anyhow::Result<u32> {
        todo!("Seeking AAC stream")
    }

    fn current_sample_position(&self) -> u32 {
        self.samples_position
    }
}

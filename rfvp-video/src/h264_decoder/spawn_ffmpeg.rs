//! This module implements an H264 decoder that spawns an ffmpeg process to delegate the decoding.
//!
//! It pipes the H264 Annex B bitstream to ffmpeg, and reads the YUV420P frames wrapped in y4m format from ffmpeg's stdout.

use std::{
    io::{Read, Seek},
    iter::Peekable,
    process::Stdio,
};

use anyhow::{bail, Context, Result};
use futures_lite::{io::BufReader, AsyncBufReadExt, AsyncWriteExt};
use rfvp_tasks::{IoTaskPool, Task};
use tracing::{debug, error, trace, warn};

use crate::{
    h264_decoder::{y4m, Frame, FrameSize, FrameTiming},
    mp4::Mp4TrackReader,
    mp4_bitstream_converter::Mp4BitstreamConverter,
};

/// Decodes h264 Annex B format to YUV420P (or, possibly, some other frame format).
///
/// Currently implemented as a pipe to the ffmpeg binary, other options are possible in the future.
pub struct SpawnFfmpegH264Decoder {
    process: async_process::Child,
    frame_receiver: Peekable<std::sync::mpsc::IntoIter<Frame>>,
    frame_timing_receiver: std::sync::mpsc::Receiver<FrameTiming>,
    #[allow(unused)]
    frame_sender_task: Task<()>,
    #[allow(unused)]
    packet_sender_task: Task<()>,
    #[allow(unused)]
    stderr_task: Task<()>,

    frame_size: Option<FrameSize>,
}

const FFMPEG_LOG_LEVEL: &str = "info";

impl super::H264DecoderTrait for SpawnFfmpegH264Decoder {
    fn new<S: Read + Seek + Send + 'static>(track: Mp4TrackReader<S>) -> Result<Self> {
        // TODO: use a more robust way to find the ffmpeg binary
        let ffmpeg = which::which("ffmpeg").context("Could not locate ffmpeg binary")?;

        // let timescale = track.get_mp4_track_info(|t| t.timescale());

        let mut process = async_process::Command::new(ffmpeg)
            .arg("-loglevel")
            .arg(FFMPEG_LOG_LEVEL)
            .arg("-f")
            .arg("h264")
            .arg("-flags")
            .arg("low_delay")
            .arg("-analyzeduration")
            .arg("0")
            .arg("-probesize")
            .arg("32")
            .arg("-i")
            .arg("pipe:0")
            .arg("-f")
            .arg("yuv4mpegpipe")
            .arg("pipe:1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not spawn ffmpeg process")?;

        let stdin = process.stdin.take().unwrap();
        let stdout = process.stdout.take().unwrap();
        let stderr = process.stderr.take().unwrap();

        // send the decoded frames from ffmpeg to the game
        let (frame_sender, frame_receiver) = std::sync::mpsc::sync_channel(60);
        // send the frame timings from the mp4 stream to the game (without passing through ffmpeg)
        // this has a bit more delay than other chans because it goes around ffmpeg and ffmpeg has its own delay of several frames
        // hence the larger capacity (otherwise we might deadlock)
        let (frame_timing_sender, frame_timing_receiver) = std::sync::mpsc::sync_channel(10);

        let frame_sender_task = IoTaskPool::get().spawn(async move {
            let mut decoder = match y4m::Decoder::new(stdout).await {
                Ok(r) => r,
                Err(e) => {
                    error!(
                        "Error creating y4m decoder: {}. The stream will be closed",
                        e
                    );
                    return;
                }
            };
            loop {
                match decoder.read_frame().await {
                    Ok(frame) => {
                        trace!("Sending frame to game");
                        if frame_sender.send(frame).is_err() {
                            debug!("Game closed the channel, stopping sending frames");
                            break;
                        }
                    }
                    Err(y4m::Error::EndOfFile) => {
                        debug!("EOF from ffmpeg, stopping sending to game");
                        break;
                    }
                    Err(e) => {
                        error!("Error reading frame from ffmpeg: {}", e);
                        break;
                    }
                }
            }
        });

        let packet_sender_task = IoTaskPool::get().spawn(async move {
            let mut track = track;
            let mut buffer = Vec::new();
            let mut stdin = stdin;

            let mut bitstream_converter: Mp4BitstreamConverter =
                track.get_mp4_track_info(Mp4BitstreamConverter::for_mp4_track);

            let mut frame_number = 0;

            loop {
                match track.next_sample() {
                    Ok(Some(sample)) => {
                        // MP4 can do frame reordering if B-frames are used
                        // this seems to be indicated by the sample.rendering_offset field
                        // it also seems that this info can be duplicated in the h264 bistream in a form of picture_timing SEI NALUs
                        // it also seems that ffmpeg handles the picture_timing SEI NALUs correctly, so we don't need to do anything with the rendering offset if they are present
                        // NOTE: maybe what I said above is not correct, as, after stripping the SEI NALUs, the video still looks correct...
                        // I'll ignore the rendering offset for now, but this should be accounted for if other decoders are implemented

                        let frame_timing = FrameTiming {
                            frame_number,
                            start_time: sample.start_time,
                            duration: sample.duration,
                        };

                        frame_number += 1;

                        if frame_timing_sender.send(frame_timing).is_err() {
                            debug!("Game closed the channel, stopping sending frame timings");
                            break;
                        }

                        bitstream_converter.convert_packet(&sample.bytes, &mut buffer);
                        trace!("Sending sample to ffmpeg ({} bytes)", buffer.len());
                        match stdin.write_all(&buffer).await {
                            Ok(_) => {}
                            Err(e) => {
                                error!("Error writing sample to ffmpeg: {}", e);
                                break;
                            }
                        }
                    }
                    Ok(None) => {
                        debug!("EOF from mp4, stopping sending to ffmpeg");
                        break;
                    }
                    Err(e) => {
                        error!("Error reading sample from mp4: {}", e);
                        break;
                    }
                }
            }
        });

        // pump ffmpeg's stderr to the logs
        // TODO: parse the ffmpeg's log leve?
        let stderr_task = IoTaskPool::get().spawn(async move {
            let mut stderr = BufReader::new(stderr);
            let mut buffer = String::new();

            loop {
                match stderr.read_line(&mut buffer).await {
                    Ok(0) => {
                        debug!("EOF from ffmpeg stderr, stopping reading");
                        break;
                    }
                    Ok(_) => {
                        buffer.pop(); // remove newline
                        debug!("ffmpeg: {}", buffer);
                        buffer.clear();
                    }
                    Err(e) => {
                        error!("Error reading ffmpeg stderr: {}", e);
                        break;
                    }
                }
            }
        });

        let frame_receiver = frame_receiver.into_iter().peekable();

        Ok(Self {
            process,
            frame_receiver,
            frame_timing_receiver,
            frame_sender_task,
            packet_sender_task,
            stderr_task,
            frame_size: None,
        })
    }

    fn read_frame(&mut self) -> Result<Option<(FrameTiming, Frame)>> {
        trace!("Reading frame from ffmpeg...");
        match self.frame_receiver.next() {
            Some(frame) => {
                let timing = self.frame_timing_receiver.recv().unwrap();

                self.frame_size = Some(*frame.size());
                Ok(Some((timing, frame)))
            }
            None => Ok(None),
        }
    }

    fn frame_size(&mut self) -> Result<FrameSize> {
        debug!("Reading frame info from ffmpeg");
        match self.frame_size {
            Some(info) => Ok(info),
            None => match self.frame_receiver.peek() {
                Some(frame) => {
                    self.frame_size = Some(*frame.size());
                    Ok(*frame.size())
                }
                None => {
                    bail!("No frames available, don't know the format")
                }
            },
        }
    }
}

impl Drop for SpawnFfmpegH264Decoder {
    fn drop(&mut self) {
        if let Err(e) = self.process.kill() {
            warn!("Error killing ffmpeg: {:?}", e);
        }
    }
}

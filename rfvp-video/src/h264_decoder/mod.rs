mod y4m;

use std::io::{Read, Seek};
use cfg_if::cfg_if;
use anyhow::Result;
pub use y4m::{BitsPerSample, Colorspace, Frame, FrameSize, PlaneSize};

use crate::mp4::Mp4TrackReader;

pub trait H264DecoderTrait: Sized {
    fn new<S: Read + Seek + Send + 'static>(track: Mp4TrackReader<S>) -> Result<Self>;

    fn read_frame(&mut self) -> Result<Option<(FrameTiming, Frame)>>;

    fn frame_size(&mut self) -> Result<FrameSize>;
}

cfg_if! {
    if #[cfg(feature = "gstreamer")] {
        mod gstreamer;
        pub use self::gstreamer::GStreamerH264Decoder as H264Decoder;
    } else {
        mod spawn_ffmpeg;
        pub use spawn_ffmpeg::SpawnFfmpegH264Decoder as H264Decoder;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrameTiming {
    pub frame_number: u32,
    pub start_time: u64,
    pub duration: u32,
}
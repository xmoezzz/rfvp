//! na_mpeg2_decoder: MPEG-1/2 video + MPEG audio (MP1/2/3) decode helpers.

pub mod demux;
pub mod convert;
pub mod pipeline;
pub mod video;

pub mod error;
pub mod audio;
pub mod av;

pub use demux::{Demuxer, Packet, StreamType};
pub use convert::{frame_to_gray_rgba, frame_to_rgba_bt601_limited};
pub use pipeline::MpegVideoPipeline;
pub use video::{Decoder, Frame, PixelFormat};

pub use error::{AvError, Result as AvResult};
pub use audio::{MpaAudioChunk, MpaAudioDecoder};
pub use av::{MpegAvEvent, MpegAvPipeline, MpegAudioF32, MpegRgbaFrame};

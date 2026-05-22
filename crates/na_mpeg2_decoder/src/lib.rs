//! na_mpeg2_decoder: MPEG-1/2 video + MPEG audio (MP1/2/3) decode helpers.

pub mod convert;
pub mod demux;
pub mod pipeline;
pub mod video;

pub mod audio;
pub mod av;
pub mod error;

pub use convert::{frame_to_gray_rgba, frame_to_rgba_bt601_limited};
pub use demux::{Demuxer, Packet, StreamType};
pub use pipeline::MpegVideoPipeline;
pub use video::{Decoder, Frame, PixelFormat};

pub use audio::{MpaAudioChunk, MpaAudioDecoder};
pub use av::{MpegAudioF32, MpegAvEvent, MpegAvPipeline, MpegRgbaFrame};
pub use error::{AvError, Result as AvResult};

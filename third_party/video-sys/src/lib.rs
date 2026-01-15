#![deny(unsafe_op_in_unsafe_fn)]

pub mod backend;
pub mod core;
pub mod h264;
pub mod mp4;
pub mod pixel;

pub use core::{VideoCore, VideoFrame, VideoStream};

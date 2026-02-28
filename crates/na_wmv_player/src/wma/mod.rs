//! Windows Media Audio (WMA v1/v2) decoder.
//!
//!
//! Scope:
//! - WMAv1 (format tag 0x0160)
//! - WMAv2 (format tag 0x0161)
//!


pub mod bitstream;
pub mod common;
pub mod mdct;
pub mod tables;
pub mod vlc;

mod decoder;

pub use decoder::{PcmFrameF32, WmaDecoder};

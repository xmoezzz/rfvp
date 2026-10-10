//! Windows Media Audio decoders.
//!
//! Scope:
//! - WMAv1 (format tag 0x0160)
//! - WMAv2 (format tag 0x0161)
//! - WMA Professional (format tag 0x0162)
//!

pub mod bitstream;
pub mod common;
pub mod mdct;
mod pro;
mod pro_tables;
pub mod tables;
pub mod vlc;

mod decoder;

pub use decoder::{PcmFrameF32, WmaDecoder};
pub(crate) use pro::WmaProDecoder;

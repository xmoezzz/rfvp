//! Native ASF/WMV container parsing and WMV/VC-1 decoding library.
//!

pub mod asf;
pub mod bitreader;
pub mod color;
pub mod decoder;
pub mod error;
pub mod na_msmpeg4_mv_tables;
pub mod na_msmpeg4_tables;
pub mod na_rl_tables;
pub mod na_simple_idct;
pub mod na_wmv2_tables;
pub mod na_wmv2dsp;
pub mod vc1;
pub mod vc1_tables;
pub mod vlc;
pub mod vlc_tree;
pub mod wmv2;

#[cfg(feature = "audio")]
pub mod wma;

pub mod api;

#[cfg(feature = "audio")]
pub use api::{AsfWmaDecoder, DecodedAudioFrame};
pub use api::{AsfWmv2Decoder, DecodedFrame, Wmv2Decoder, Wmv3Decoder, Wvc1Decoder};
pub use color::{
    VideoTransferMatrix, yuv_limited_to_rgb, yuv420p_to_rgb, yuv420p_to_rgba,
    yuv420p_to_rgba_scaled,
};
pub use decoder::YuvFrame;
pub use error::{DecoderError, Result};

pub mod ffi;

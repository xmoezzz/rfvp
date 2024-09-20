//! Glue together mp4 demuxing, h264 and aac decoding and `shin-render` APIs to implement video playback in `shin`.

mod audio;
mod h264_decoder;
pub mod mp4;
mod mp4_bitstream_converter;
mod timer;
mod video_player;
mod yuv_texture;

pub use video_player::VideoPlayer;
pub use yuv_texture::YuvTexture;

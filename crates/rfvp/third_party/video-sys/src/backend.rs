use anyhow::{anyhow, Result};

use crate::h264::H264Config;
use crate::mp4::EncodedSample;

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub pts_us: i64,
    pub rgba: Vec<u8>,
}

pub trait H264Decoder {
    fn push(&mut self, sample: EncodedSample) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
    fn try_receive(&mut self) -> Result<Option<VideoFrame>>;
}

pub fn create_default_h264_decoder(cfg: &H264Config) -> Result<Box<dyn H264Decoder>> {
    #[cfg(target_os = "windows")]
    {
        return Ok(Box::new(mf::MfH264Decoder::new(cfg.clone())?));
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        return Ok(Box::new(vt::VtH264Decoder::new(cfg.clone())?));
    }

    #[cfg(target_os = "android")]
    {
        return Ok(Box::new(mediacodec::AndroidH264Decoder::new(cfg.clone())?));
    }

    #[cfg(target_os = "linux")]
    {
        return Ok(Box::new(gst::GstH264Decoder::new(cfg.clone())?));
    }

    #[allow(unreachable_code)]
    Err(anyhow!("no supported H.264 system decoder backend for this target"))
}

#[cfg(target_os = "linux")]
mod gst;

#[cfg(target_os = "windows")]
mod mf;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod vt;

#[cfg(target_os = "android")]
mod mediacodec;

use std::collections::VecDeque;
use std::ffi::CString;
use std::ptr;

use anyhow::{anyhow, bail, Result};

use crate::backend::{H264Decoder, VideoFrame};
use crate::h264::H264Config;
use crate::mp4::EncodedSample;
use crate::pixel::yuv420_888_to_rgba;

type MediaStatus = i32;

#[repr(C)]
struct AMediaCodec;
#[repr(C)]
struct AMediaFormat;
#[repr(C)]
struct AMediaCrypto;

#[repr(C)]
struct AMediaImage;

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
struct AMediaCodecBufferInfo {
    offset: i32,
    size: i32,
    presentationTimeUs: i64,
    flags: u32,
}

const AMEDIACODEC_INFO_TRY_AGAIN_LATER: i32 = -1;
const AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED: i32 = -2;
const AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED: i32 = -3;

#[link(name = "mediandk")]
unsafe extern "C" {
    fn AMediaCodec_createDecoderByType(mime_type: *const i8) -> *mut AMediaCodec;
    fn AMediaCodec_delete(codec: *mut AMediaCodec) -> MediaStatus;

    fn AMediaCodec_configure(
        codec: *mut AMediaCodec,
        format: *mut AMediaFormat,
        surface: *mut libc::c_void,
        crypto: *mut AMediaCrypto,
        flags: u32,
    ) -> MediaStatus;

    fn AMediaCodec_start(codec: *mut AMediaCodec) -> MediaStatus;
    fn AMediaCodec_stop(codec: *mut AMediaCodec) -> MediaStatus;
    fn AMediaCodec_flush(codec: *mut AMediaCodec) -> MediaStatus;

    fn AMediaCodec_dequeueInputBuffer(codec: *mut AMediaCodec, timeoutUs: i64) -> i32;
    fn AMediaCodec_getInputBuffer(codec: *mut AMediaCodec, idx: usize, out_size: *mut usize) -> *mut u8;
    fn AMediaCodec_queueInputBuffer(
        codec: *mut AMediaCodec,
        idx: usize,
        offset: usize,
        size: usize,
        timeUs: i64,
        flags: u32,
    ) -> MediaStatus;

    fn AMediaCodec_dequeueOutputBuffer(
        codec: *mut AMediaCodec,
        info: *mut AMediaCodecBufferInfo,
        timeoutUs: i64,
    ) -> i32;

    fn AMediaCodec_releaseOutputBuffer(codec: *mut AMediaCodec, idx: usize, render: bool) -> MediaStatus;

    fn AMediaCodec_getOutputImage(codec: *mut AMediaCodec, idx: usize) -> *mut AMediaImage;

    fn AMediaCodec_getOutputFormat(codec: *mut AMediaCodec) -> *mut AMediaFormat;

    fn AMediaFormat_new() -> *mut AMediaFormat;
    fn AMediaFormat_delete(format: *mut AMediaFormat);

    fn AMediaFormat_setString(format: *mut AMediaFormat, name: *const i8, value: *const i8);
    fn AMediaFormat_setInt32(format: *mut AMediaFormat, name: *const i8, value: i32);
    fn AMediaFormat_setBuffer(format: *mut AMediaFormat, name: *const i8, data: *const libc::c_void, size: usize);
    fn AMediaFormat_getInt32(format: *mut AMediaFormat, name: *const i8, out: *mut i32) -> bool;

    fn AMediaImage_delete(image: *mut AMediaImage);

    fn AMediaImage_getWidth(image: *const AMediaImage) -> i32;
    fn AMediaImage_getHeight(image: *const AMediaImage) -> i32;
    fn AMediaImage_getPlaneData(image: *const AMediaImage, planeIdx: i32, data: *mut *mut u8, out_len: *mut usize) -> MediaStatus;
    fn AMediaImage_getPlaneRowStride(image: *const AMediaImage, planeIdx: i32) -> i32;
    fn AMediaImage_getPlanePixelStride(image: *const AMediaImage, planeIdx: i32) -> i32;
}

pub struct AndroidH264Decoder {
    cfg: H264Config,
    codec: *mut AMediaCodec,
    format: *mut AMediaFormat,

    in_queue: VecDeque<EncodedSample>,
    eos_sent: bool,
}

unsafe impl Send for AndroidH264Decoder {}
unsafe impl Sync for AndroidH264Decoder {}

impl AndroidH264Decoder {
    pub fn new(cfg: H264Config) -> Result<Self> {
        let mime = CString::new("video/avc")?;
        let codec = unsafe { AMediaCodec_createDecoderByType(mime.as_ptr()) };
        if codec.is_null() {
            bail!("AMediaCodec_createDecoderByType(video/avc) returned null");
        }

        let format = unsafe { AMediaFormat_new() };
        if format.is_null() {
            unsafe { AMediaCodec_delete(codec) };
            bail!("AMediaFormat_new returned null");
        }

        unsafe {
            let key_mime = CString::new("mime")?;
            let key_w = CString::new("width")?;
            let key_h = CString::new("height")?;
            let key_csd0 = CString::new("csd-0")?;
            let key_csd1 = CString::new("csd-1")?;

            AMediaFormat_setString(format, key_mime.as_ptr(), mime.as_ptr());
            AMediaFormat_setInt32(format, key_w.as_ptr(), cfg.width as i32);
            AMediaFormat_setInt32(format, key_h.as_ptr(), cfg.height as i32);

            // For H.264, `csd-0` is SPS and `csd-1` is PPS. Both are typically provided in Annex B form.
            let mut csd0 = Vec::new();
            csd0.extend_from_slice(&[0, 0, 0, 1]);
            csd0.extend_from_slice(&cfg.sps[0]);

            let mut csd1 = Vec::new();
            csd1.extend_from_slice(&[0, 0, 0, 1]);
            csd1.extend_from_slice(&cfg.pps[0]);

            AMediaFormat_setBuffer(format, key_csd0.as_ptr(), csd0.as_ptr() as *const libc::c_void, csd0.len());
            AMediaFormat_setBuffer(format, key_csd1.as_ptr(), csd1.as_ptr() as *const libc::c_void, csd1.len());

            let st = AMediaCodec_configure(codec, format, ptr::null_mut(), ptr::null_mut(), 0);
            if st != 0 {
                AMediaFormat_delete(format);
                AMediaCodec_delete(codec);
                bail!("AMediaCodec_configure failed: status={}", st);
            }

            let st = AMediaCodec_start(codec);
            if st != 0 {
                AMediaFormat_delete(format);
                AMediaCodec_delete(codec);
                bail!("AMediaCodec_start failed: status={}", st);
            }
        }

        Ok(Self {
            cfg,
            codec,
            format,
            in_queue: VecDeque::new(),
            eos_sent: false,
        })
    }

    fn try_feed(&mut self) -> Result<()> {
        if self.codec.is_null() || self.eos_sent {
            return Ok(());
        }

        while let Some(s) = self.in_queue.front() {
            // Convert to Annex B for MediaCodec input.
            let annexb = self.cfg.avcc_sample_to_annexb(&s.data_avcc)?;
            let idx = unsafe { AMediaCodec_dequeueInputBuffer(self.codec, 0) };
            if idx < 0 {
                break;
            }

            let mut in_size: usize = 0;
            let in_ptr = unsafe { AMediaCodec_getInputBuffer(self.codec, idx as usize, &mut in_size as *mut usize) };
            if in_ptr.is_null() {
                bail!("AMediaCodec_getInputBuffer returned null");
            }
            if annexb.len() > in_size {
                bail!("annexb frame too large: {} > {}", annexb.len(), in_size);
            }

            unsafe {
                ptr::copy_nonoverlapping(annexb.as_ptr(), in_ptr, annexb.len());
                let st = AMediaCodec_queueInputBuffer(
                    self.codec,
                    idx as usize,
                    0,
                    annexb.len(),
                    s.pts_us,
                    0,
                );
                if st != 0 {
                    bail!("AMediaCodec_queueInputBuffer failed: status={}", st);
                }
            }

            self.in_queue.pop_front();
        }

        Ok(())
    }

    fn get_output_dims(&mut self) -> Option<(u32, u32)> {
        let fmt = unsafe { AMediaCodec_getOutputFormat(self.codec) };
        if fmt.is_null() {
            return None;
        }

        unsafe {
            let mut w: i32 = 0;
            let mut h: i32 = 0;
            let key_w = CString::new("width").ok()?;
            let key_h = CString::new("height").ok()?;
            let ok_w = AMediaFormat_getInt32(fmt, key_w.as_ptr(), &mut w as *mut i32);
            let ok_h = AMediaFormat_getInt32(fmt, key_h.as_ptr(), &mut h as *mut i32);
            AMediaFormat_delete(fmt);
            if ok_w && ok_h && w > 0 && h > 0 {
                Some((w as u32, h as u32))
            } else {
                None
            }
        }
    }
}

impl Drop for AndroidH264Decoder {
    fn drop(&mut self) {
        unsafe {
            if !self.codec.is_null() {
                let _ = AMediaCodec_stop(self.codec);
                let _ = AMediaCodec_delete(self.codec);
                self.codec = ptr::null_mut();
            }
            if !self.format.is_null() {
                AMediaFormat_delete(self.format);
                self.format = ptr::null_mut();
            }
        }
    }
}

impl H264Decoder for AndroidH264Decoder {
    fn push(&mut self, sample: EncodedSample) -> Result<()> {
        self.in_queue.push_back(sample);
        self.try_feed()
    }

    fn flush(&mut self) -> Result<()> {
        if self.codec.is_null() || self.eos_sent {
            return Ok(());
        }

        // Send EOS when we can get an input buffer.
        loop {
            let idx = unsafe { AMediaCodec_dequeueInputBuffer(self.codec, 0) };
            if idx < 0 {
                break;
            }
            unsafe {
                let st = AMediaCodec_queueInputBuffer(self.codec, idx as usize, 0, 0, 0, 4 /* AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM */);
                if st != 0 {
                    bail!("AMediaCodec_queueInputBuffer(EOS) failed: status={}", st);
                }
            }
            self.eos_sent = true;
            break;
        }
        Ok(())
    }

    fn try_receive(&mut self) -> Result<Option<VideoFrame>> {
        self.try_feed()?;

        let mut info = AMediaCodecBufferInfo::default();

        let idx = unsafe { AMediaCodec_dequeueOutputBuffer(self.codec, &mut info as *mut _, 0) };
        if idx == AMEDIACODEC_INFO_TRY_AGAIN_LATER {
            return Ok(None);
        }
        if idx == AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED || idx == AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED {
            // No frame produced; try again next tick.
            return Ok(None);
        }
        if idx < 0 {
            return Err(anyhow!("AMediaCodec_dequeueOutputBuffer returned {}", idx));
        }

        let image = unsafe { AMediaCodec_getOutputImage(self.codec, idx as usize) };
        if image.is_null() {
            unsafe {
                let _ = AMediaCodec_releaseOutputBuffer(self.codec, idx as usize, false);
            }
            bail!("AMediaCodec_getOutputImage returned null");
        }

        let width = unsafe { AMediaImage_getWidth(image) };
        let height = unsafe { AMediaImage_getHeight(image) };

        let (width, height) = if width > 0 && height > 0 {
            (width as u32, height as u32)
        } else {
            self.get_output_dims().unwrap_or((self.cfg.width, self.cfg.height))
        };

        // Planes: 0=Y, 1=U, 2=V for YUV_420_888.
        let mut y_ptr: *mut u8 = ptr::null_mut();
        let mut u_ptr: *mut u8 = ptr::null_mut();
        let mut v_ptr: *mut u8 = ptr::null_mut();
        let mut y_len: usize = 0;
        let mut u_len: usize = 0;
        let mut v_len: usize = 0;

        let st0 = unsafe { AMediaImage_getPlaneData(image, 0, &mut y_ptr as *mut _, &mut y_len as *mut _) };
        let st1 = unsafe { AMediaImage_getPlaneData(image, 1, &mut u_ptr as *mut _, &mut u_len as *mut _) };
        let st2 = unsafe { AMediaImage_getPlaneData(image, 2, &mut v_ptr as *mut _, &mut v_len as *mut _) };
        if st0 != 0 || st1 != 0 || st2 != 0 || y_ptr.is_null() || u_ptr.is_null() || v_ptr.is_null() {
            unsafe {
                AMediaImage_delete(image);
                let _ = AMediaCodec_releaseOutputBuffer(self.codec, idx as usize, false);
            }
            bail!("AMediaImage_getPlaneData failed");
        }

        let y_rs = unsafe { AMediaImage_getPlaneRowStride(image, 0) } as usize;
        let u_rs = unsafe { AMediaImage_getPlaneRowStride(image, 1) } as usize;
        let v_rs = unsafe { AMediaImage_getPlaneRowStride(image, 2) } as usize;
        let u_ps = unsafe { AMediaImage_getPlanePixelStride(image, 1) } as usize;
        let v_ps = unsafe { AMediaImage_getPlanePixelStride(image, 2) } as usize;

        let y = unsafe { std::slice::from_raw_parts(y_ptr as *const u8, y_len) };
        let u = unsafe { std::slice::from_raw_parts(u_ptr as *const u8, u_len) };
        let v = unsafe { std::slice::from_raw_parts(v_ptr as *const u8, v_len) };

        let rgba = yuv420_888_to_rgba(width, height, y_rs, u_rs, v_rs, u_ps, v_ps, y, u, v);

        unsafe {
            AMediaImage_delete(image);
            let _ = AMediaCodec_releaseOutputBuffer(self.codec, idx as usize, false);
        }

        Ok(Some(VideoFrame {
            width,
            height,
            pts_us: info.presentationTimeUs,
            rgba,
        }))
    }
}

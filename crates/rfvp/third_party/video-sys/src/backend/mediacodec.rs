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
struct AImageReader;
#[repr(C)]
struct AImage;
#[repr(C)]
struct ANativeWindow;

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

// Java ImageFormat.YUV_420_888
const AIMAGE_FORMAT_YUV_420_888: i32 = 35;

#[link(name = "mediandk")]
unsafe extern "C" {
    fn AMediaCodec_createDecoderByType(mime_type: *const i8) -> *mut AMediaCodec;
    fn AMediaCodec_delete(codec: *mut AMediaCodec) -> MediaStatus;

    fn AMediaCodec_configure(
        codec: *mut AMediaCodec,
        format: *mut AMediaFormat,
        surface: *mut libc::c_void, // actually ANativeWindow*
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

    fn AMediaCodec_getOutputFormat(codec: *mut AMediaCodec) -> *mut AMediaFormat;

    fn AMediaFormat_new() -> *mut AMediaFormat;
    fn AMediaFormat_delete(format: *mut AMediaFormat);

    fn AMediaFormat_setString(format: *mut AMediaFormat, name: *const i8, value: *const i8);
    fn AMediaFormat_setInt32(format: *mut AMediaFormat, name: *const i8, value: i32);
    fn AMediaFormat_setBuffer(format: *mut AMediaFormat, name: *const i8, data: *const libc::c_void, size: usize);
    fn AMediaFormat_getInt32(format: *mut AMediaFormat, name: *const i8, out: *mut i32) -> bool;

    // AImageReader / AImage APIs (stable NDK)
    fn AImageReader_new(
        width: i32,
        height: i32,
        format: i32,
        maxImages: i32,
        reader: *mut *mut AImageReader,
    ) -> MediaStatus;
    fn AImageReader_delete(reader: *mut AImageReader) -> MediaStatus;
    fn AImageReader_getWindow(reader: *mut AImageReader, window: *mut *mut ANativeWindow) -> MediaStatus;

    fn AImageReader_acquireLatestImage(reader: *mut AImageReader, image: *mut *mut AImage) -> MediaStatus;

    fn AImage_delete(image: *mut AImage) -> MediaStatus;

    fn AImage_getWidth(image: *const AImage, width: *mut i32) -> MediaStatus;
    fn AImage_getHeight(image: *const AImage, height: *mut i32) -> MediaStatus;

    fn AImage_getPlaneData(image: *const AImage, planeIdx: i32, data: *mut *mut u8, dataLength: *mut i32) -> MediaStatus;
    fn AImage_getPlaneRowStride(image: *const AImage, planeIdx: i32, rowStride: *mut i32) -> MediaStatus;
    fn AImage_getPlanePixelStride(image: *const AImage, planeIdx: i32, pixelStride: *mut i32) -> MediaStatus;
}

pub struct AndroidH264Decoder {
    cfg: H264Config,
    codec: *mut AMediaCodec,
    format: *mut AMediaFormat,

    reader: *mut AImageReader,
    window: *mut ANativeWindow,

    in_queue: VecDeque<EncodedSample>,
    eos_sent: bool,
}

unsafe impl Send for AndroidH264Decoder {}
unsafe impl Sync for AndroidH264Decoder {}

impl AndroidH264Decoder {
    pub fn new(cfg: H264Config) -> Result<Self> {
        // Create AImageReader + ANativeWindow surface for decoder output.
        let mut reader: *mut AImageReader = ptr::null_mut();
        let st = unsafe {
            AImageReader_new(cfg.width as i32, cfg.height as i32, AIMAGE_FORMAT_YUV_420_888, 4, &mut reader)
        };
        if st != 0 || reader.is_null() {
            bail!("AImageReader_new failed: status={}", st);
        }
        let mut window: *mut ANativeWindow = ptr::null_mut();
        let st = unsafe { AImageReader_getWindow(reader, &mut window) };
        if st != 0 || window.is_null() {
            unsafe { let _ = AImageReader_delete(reader); }
            bail!("AImageReader_getWindow failed: status={}", st);
        }

        let mime = CString::new("video/avc")?;
        let codec = unsafe { AMediaCodec_createDecoderByType(mime.as_ptr() as *const i8) };
        if codec.is_null() {
            unsafe { let _ = AImageReader_delete(reader); }
            bail!("AMediaCodec_createDecoderByType(video/avc) returned null");
        }

        let format = unsafe { AMediaFormat_new() };
        if format.is_null() {
            unsafe {
                AMediaCodec_delete(codec);
                let _ = AImageReader_delete(reader);
            }
            bail!("AMediaFormat_new returned null");
        }

        unsafe {
            let key_mime = CString::new("mime")?;
            let key_w = CString::new("width")?;
            let key_h = CString::new("height")?;
            let key_csd0 = CString::new("csd-0")?;
            let key_csd1 = CString::new("csd-1")?;

            AMediaFormat_setString(format, key_mime.as_ptr() as *const i8, mime.as_ptr() as *const i8);
            AMediaFormat_setInt32(format, key_w.as_ptr() as *const i8, cfg.width as i32);
            AMediaFormat_setInt32(format, key_h.as_ptr() as *const i8, cfg.height as i32);

            let mut csd0 = Vec::new();
            csd0.extend_from_slice(&[0, 0, 0, 1]);
            csd0.extend_from_slice(&cfg.sps[0]);

            let mut csd1 = Vec::new();
            csd1.extend_from_slice(&[0, 0, 0, 1]);
            csd1.extend_from_slice(&cfg.pps[0]);

            AMediaFormat_setBuffer(format, key_csd0.as_ptr() as *const i8, csd0.as_ptr() as *const libc::c_void, csd0.len());
            AMediaFormat_setBuffer(format, key_csd1.as_ptr() as *const i8, csd1.as_ptr() as *const libc::c_void, csd1.len());

            // IMPORTANT: pass ANativeWindow* as the output surface.
            let st = AMediaCodec_configure(codec, format, window as *mut libc::c_void, ptr::null_mut(), 0);
            if st != 0 {
                AMediaFormat_delete(format);
                AMediaCodec_delete(codec);
                let _ = AImageReader_delete(reader);
                bail!("AMediaCodec_configure failed: status={}", st);
            }

            let st = AMediaCodec_start(codec);
            if st != 0 {
                AMediaFormat_delete(format);
                AMediaCodec_delete(codec);
                let _ = AImageReader_delete(reader);
                bail!("AMediaCodec_start failed: status={}", st);
            }
        }

        Ok(Self {
            cfg,
            codec,
            format,
            reader,
            window,
            in_queue: VecDeque::new(),
            eos_sent: false,
        })
    }

    fn try_feed(&mut self) -> Result<()> {
        if self.codec.is_null() || self.eos_sent {
            return Ok(());
        }

        while let Some(s) = self.in_queue.front() {
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
                let st = AMediaCodec_queueInputBuffer(self.codec, idx as usize, 0, annexb.len(), s.pts_us, 0);
                if st != 0 {
                    bail!("AMediaCodec_queueInputBuffer failed: status={}", st);
                }
            }

            self.in_queue.pop_front();
        }

        Ok(())
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
            if !self.reader.is_null() {
                let _ = AImageReader_delete(self.reader);
                self.reader = ptr::null_mut();
                self.window = ptr::null_mut();
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
        loop {
            let idx = unsafe { AMediaCodec_dequeueInputBuffer(self.codec, 0) };
            if idx < 0 {
                break;
            }
            unsafe {
                let st = AMediaCodec_queueInputBuffer(self.codec, idx as usize, 0, 0, 0, 4);
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
            return Ok(None);
        }
        if idx < 0 {
            return Err(anyhow!("AMediaCodec_dequeueOutputBuffer returned {}", idx));
        }

        // Render this output buffer into the AImageReader surface.
        unsafe {
            let st = AMediaCodec_releaseOutputBuffer(self.codec, idx as usize, true);
            if st != 0 {
                bail!("AMediaCodec_releaseOutputBuffer(render=true) failed: status={}", st);
            }
        }

        // Acquire the latest image (drops older ones).
        let mut image: *mut AImage = ptr::null_mut();
        let st = unsafe { AImageReader_acquireLatestImage(self.reader, &mut image) };
        if st != 0 || image.is_null() {
            return Ok(None);
        }

        let mut w: i32 = 0;
        let mut h: i32 = 0;
        unsafe {
            let _ = AImage_getWidth(image, &mut w);
            let _ = AImage_getHeight(image, &mut h);
        }
        let width = if w > 0 { w as u32 } else { self.cfg.width };
        let height = if h > 0 { h as u32 } else { self.cfg.height };

        let mut y_ptr: *mut u8 = ptr::null_mut();
        let mut u_ptr: *mut u8 = ptr::null_mut();
        let mut v_ptr: *mut u8 = ptr::null_mut();
        let mut y_len: i32 = 0;
        let mut u_len: i32 = 0;
        let mut v_len: i32 = 0;

        let st0 = unsafe { AImage_getPlaneData(image, 0, &mut y_ptr, &mut y_len) };
        let st1 = unsafe { AImage_getPlaneData(image, 1, &mut u_ptr, &mut u_len) };
        let st2 = unsafe { AImage_getPlaneData(image, 2, &mut v_ptr, &mut v_len) };

        if st0 != 0 || st1 != 0 || st2 != 0 || y_ptr.is_null() || u_ptr.is_null() || v_ptr.is_null() {
            unsafe { let _ = AImage_delete(image); }
            bail!("AImage_getPlaneData failed");
        }

        let mut y_rs: i32 = 0;
        let mut u_rs: i32 = 0;
        let mut v_rs: i32 = 0;
        let mut u_ps: i32 = 0;
        let mut v_ps: i32 = 0;

        unsafe {
            let _ = AImage_getPlaneRowStride(image, 0, &mut y_rs);
            let _ = AImage_getPlaneRowStride(image, 1, &mut u_rs);
            let _ = AImage_getPlaneRowStride(image, 2, &mut v_rs);
            let _ = AImage_getPlanePixelStride(image, 1, &mut u_ps);
            let _ = AImage_getPlanePixelStride(image, 2, &mut v_ps);
        }

        let y = unsafe { std::slice::from_raw_parts(y_ptr as *const u8, y_len.max(0) as usize) };
        let u = unsafe { std::slice::from_raw_parts(u_ptr as *const u8, u_len.max(0) as usize) };
        let v = unsafe { std::slice::from_raw_parts(v_ptr as *const u8, v_len.max(0) as usize) };

        let rgba = yuv420_888_to_rgba(
            width,
            height,
            y_rs.max(0) as usize,
            u_rs.max(0) as usize,
            v_rs.max(0) as usize,
            u_ps.max(0) as usize,
            v_ps.max(0) as usize,
            y,
            u,
            v,
        );

        unsafe { let _ = AImage_delete(image); }

        Ok(Some(VideoFrame {
            width,
            height,
            pts_us: info.presentationTimeUs,
            rgba,
        }))
    }
}

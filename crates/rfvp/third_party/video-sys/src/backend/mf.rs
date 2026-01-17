use std::collections::VecDeque;
use std::sync::Once;

use anyhow::{anyhow, bail, Context, Result};
use windows::core::Interface;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED};

use crate::backend::{H264Decoder, VideoFrame};
use crate::h264::H264Config;
use crate::mp4::EncodedSample;
use crate::pixel::nv12_to_rgba_strided;

static MF_INIT: Once = Once::new();

pub struct MfH264Decoder {
    cfg: H264Config,
    dec: IMFTransform,
    pending_in: VecDeque<EncodedSample>,
    out_queue: VecDeque<VideoFrame>,
    stride: usize,
}

impl MfH264Decoder {
    pub fn new(cfg: H264Config) -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().context("CoInitializeEx")?;
        }

        MF_INIT.call_once(|| unsafe {
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        });

        let dec = unsafe { create_h264_decoder_mft().context("create decoder MFT")? };
        unsafe { configure_decoder(&dec, &cfg).context("configure decoder")? };

        let stride = unsafe { query_nv12_stride(&dec, cfg.width).unwrap_or(cfg.width as usize) };

        Ok(Self {
            cfg,
            dec,
            pending_in: VecDeque::new(),
            out_queue: VecDeque::new(),
            stride,
        })
    }

    fn try_feed(&mut self) -> Result<()> {
        // Always drain first (prevents MF_E_NOTACCEPTING churn).
        self.drain_output()?;

        while let Some(sample) = self.pending_in.pop_front() {
            let accepted = self.feed_one(sample)?;
            if !accepted {
                // Not accepting now; put it back and retry in a later tick.
                self.pending_in.push_front(sample);
                break;
            }
            self.drain_output()?;
        }
        Ok(())
    }

    fn feed_one(&mut self, sample: EncodedSample) -> Result<bool> {
        let annexb = self.cfg.avcc_sample_to_annexb(&sample.data_avcc)?;

        unsafe {
            let mut buf = None;
            MFCreateMemoryBuffer(annexb.len() as u32, &mut buf).ok().context("MFCreateMemoryBuffer")?;
            let buf = buf.ok_or_else(|| anyhow!("MFCreateMemoryBuffer returned null"))?;

            let mut ptr = std::ptr::null_mut();
            let mut max_len = 0u32;
            let mut cur_len = 0u32;
            buf.Lock(&mut ptr, Some(&mut max_len), Some(&mut cur_len))
                .ok()
                .context("IMFMediaBuffer::Lock")?;
            if max_len < annexb.len() as u32 {
                buf.Unlock().ok().ok();
                bail!("MF buffer too small: {} < {}", max_len, annexb.len());
            }
            std::ptr::copy_nonoverlapping(annexb.as_ptr(), ptr as *mut u8, annexb.len());
            buf.Unlock().ok().context("IMFMediaBuffer::Unlock")?;
            buf.SetCurrentLength(annexb.len() as u32).ok().context("SetCurrentLength")?;

            let mut s = None;
            MFCreateSample(&mut s).ok().context("MFCreateSample")?;
            let s = s.ok_or_else(|| anyhow!("MFCreateSample returned null"))?;
            s.AddBuffer(&buf).ok().context("IMFSample::AddBuffer")?;

            // Media Foundation timestamps are in 100-ns units.
            s.SetSampleTime(sample.pts_us * 10).ok().context("SetSampleTime")?;
            if sample.dur_us > 0 {
                s.SetSampleDuration(sample.dur_us * 10).ok().context("SetSampleDuration")?;
            }

            match self.dec.ProcessInput(0, &s, 0) {
                Ok(()) => return Ok(true),
                Err(e) if e.code() == MF_E_NOTACCEPTING => return Ok(false),
                Err(e) => return Err(anyhow!("ProcessInput failed: {e}")),
            }
        }
        Ok(true)
    }

    fn drain_output(&mut self) -> Result<()> {
        unsafe {
            loop {
                match process_output_once(&self.dec, self.cfg.width, self.cfg.height, self.stride) {
                    Ok(Some(frame)) => self.out_queue.push_back(frame),
                    Ok(None) => break,
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(true)
    }
}

impl Drop for MfH264Decoder {
    fn drop(&mut self) {
        unsafe {
            let _ = self.dec.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
            let _ = self.dec.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            let _ = self.dec.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            CoUninitialize();
        }
    }
}

impl H264Decoder for MfH264Decoder {
    fn push(&mut self, sample: EncodedSample) -> Result<()> {
        self.pending_in.push_back(sample);
        self.try_feed()
    }

    fn flush(&mut self) -> Result<()> {
        unsafe {
            let _ = self.dec.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.dec.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0);
        }
        self.drain_output()
    }

    fn try_receive(&mut self) -> Result<Option<VideoFrame>> {
        self.try_feed()?;
        Ok(self.out_queue.pop_front())
    }
}

unsafe fn create_h264_decoder_mft() -> Result<IMFTransform> {
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut act_count: u32 = 0;

    // Enumerate decoder MFTs.
    let input_type = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let output_type = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };

    MFTEnumEx(
        MFT_CATEGORY_VIDEO_DECODER,
        MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
        Some(&input_type),
        Some(&output_type),
        &mut activates,
        &mut act_count,
    )
    .ok()
    .context("MFTEnumEx")?;

    if act_count == 0 || activates.is_null() {
        bail!("no H.264 decoder MFT found");
    }

    let slice = std::slice::from_raw_parts_mut(activates, act_count as usize);
    let act = slice[0].take().ok_or_else(|| anyhow!("MFTEnumEx returned null activate"))?;

    let mut obj = None;
    act.ActivateObject(&IMFTransform::IID, &mut obj)
        .ok()
        .context("ActivateObject")?;

    // Free activation array.
    CoTaskMemFree(Some(activates as *const _));

    let unk = obj.ok_or_else(|| anyhow!("ActivateObject returned null"))?;
    let dec: IMFTransform = unk.cast().context("cast to IMFTransform")?;
    Ok(dec)
}

unsafe fn configure_decoder(dec: &IMFTransform, cfg: &H264Config) -> Result<()> {
    // Input type (H.264).
    let mut in_type = None;
    MFCreateMediaType(&mut in_type).ok().context("MFCreateMediaType(in)")?;
    let in_type = in_type.ok_or_else(|| anyhow!("MFCreateMediaType(in) returned null"))?;
    in_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
    in_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).ok()?;

    MFSetAttributeSize(&in_type, &MF_MT_FRAME_SIZE, cfg.width, cfg.height).ok()?;

    let seq = cfg.annexb_sequence_header();
    in_type
        .SetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, seq.as_ptr(), seq.len() as u32)
        .ok()
        .context("SetBlob(MF_MT_MPEG_SEQUENCE_HEADER)")?;

    in_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).ok()?;

    dec.SetInputType(0, &in_type, 0).ok().context("SetInputType")?;

    // Output type (NV12).
    let mut out_type = None;
    MFCreateMediaType(&mut out_type).ok().context("MFCreateMediaType(out)")?;
    let out_type = out_type.ok_or_else(|| anyhow!("MFCreateMediaType(out) returned null"))?;
    out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
    out_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).ok()?;
    MFSetAttributeSize(&out_type, &MF_MT_FRAME_SIZE, cfg.width, cfg.height).ok()?;
    out_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).ok()?;

    dec.SetOutputType(0, &out_type, 0).ok().context("SetOutputType")?;

    dec.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0).ok()?;
    dec.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0).ok()?;
    dec.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0).ok()?;

    Ok(())
}

unsafe fn query_nv12_stride(dec: &IMFTransform, width: u32) -> Option<usize> {
    let mut mt = None;
    dec.GetOutputCurrentType(0, &mut mt).ok()?;
    let mt = mt?;
    if let Ok(s) = mt.GetUINT32(&MF_MT_DEFAULT_STRIDE) {
        return Some(s as usize);
    }
    let aligned = ((width as usize + 15) / 16) * 16;
    Some(aligned.max(width as usize))
}

unsafe fn process_output_once(
    dec: &IMFTransform,
    width: u32,
    height: u32,
    stride: usize,
) -> Result<Option<VideoFrame>> {
    let mut info = MFT_OUTPUT_STREAM_INFO::default();
    dec.GetOutputStreamInfo(0, &mut info).ok().context("GetOutputStreamInfo")?;

    let cb = if info.cbSize != 0 {
        info.cbSize
    } else {
        (stride * height as usize * 3 / 2) as u32
    };

    let mut buf = None;
    MFCreateMemoryBuffer(cb, &mut buf).ok().context("MFCreateMemoryBuffer(out)")?;
    let buf = buf.ok_or_else(|| anyhow!("MFCreateMemoryBuffer(out) returned null"))?;

    let mut sample = None;
    MFCreateSample(&mut sample).ok().context("MFCreateSample(out)")?;
    let sample = sample.ok_or_else(|| anyhow!("MFCreateSample(out) returned null"))?;
    sample.AddBuffer(&buf).ok().context("AddBuffer(out)")?;

    let mut out = MFT_OUTPUT_DATA_BUFFER {
        dwStreamID: 0,
        pSample: Some(sample.clone()),
        dwStatus: 0,
        pEvents: None,
    };
    let mut status: u32 = 0;

    let hr = dec.ProcessOutput(0, std::slice::from_mut(&mut out), &mut status);

    if hr == MF_E_TRANSFORM_NEED_MORE_INPUT {
        return Ok(None);
    }
    hr.ok().context("ProcessOutput")?;

    // Extract NV12 bytes.
    let mut out_buf = None;
    sample
        .ConvertToContiguousBuffer(&mut out_buf)
        .ok()
        .context("ConvertToContiguousBuffer")?;
    let out_buf = out_buf.ok_or_else(|| anyhow!("ConvertToContiguousBuffer returned null"))?;

    let mut ptr = std::ptr::null_mut();
    let mut max_len = 0u32;
    let mut cur_len = 0u32;
    out_buf.Lock(&mut ptr, Some(&mut max_len), Some(&mut cur_len)).ok()?;
    let bytes = std::slice::from_raw_parts(ptr as *const u8, cur_len as usize);

    let y_size = stride * height as usize;
    let uv_size = stride * height as usize / 2;
    if bytes.len() < y_size + uv_size {
        out_buf.Unlock().ok().ok();
        return Err(anyhow!("NV12 buffer too small: {}", bytes.len()));
    }

    let y = &bytes[..y_size];
    let uv = &bytes[y_size..y_size + uv_size];
    let rgba = nv12_to_rgba_strided(width, height, stride, stride, y, uv);

    out_buf.Unlock().ok().ok();

    let pts_100ns = sample.GetSampleTime().unwrap_or(0);
    Ok(Some(VideoFrame {
        width,
        height,
        pts_us: pts_100ns / 10,
        rgba,
    }))
}

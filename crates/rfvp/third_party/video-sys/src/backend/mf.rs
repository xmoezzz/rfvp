use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::sync::Once;

use anyhow::{anyhow, bail, Context, Result};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
};

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
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .context("CoInitializeEx")?;
        }

        MF_INIT.call_once(|| unsafe {
            // MFStartup returns HRESULT; ignore failure here, later calls will surface issues anyway.
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

        loop {
            let accepted = match self.pending_in.front() {
                Some(s) => self.feed_one(s)?,
                None => break,
            };

            if !accepted {
                break;
            }

            // Only pop after we know it was accepted.
            let _ = self.pending_in.pop_front();
            self.drain_output()?;
        }

        Ok(())
    }

    fn feed_one(&self, sample: &EncodedSample) -> Result<bool> {
        let annexb = self.cfg.avcc_sample_to_annexb(&sample.data_avcc)?;

        unsafe {
            // windows-rs 0.60: MFCreateMemoryBuffer returns IMFMediaBuffer directly.
            let buf = MFCreateMemoryBuffer(annexb.len() as u32).context("MFCreateMemoryBuffer")?;

            let mut ptr = std::ptr::null_mut();
            let mut max_len = 0u32;
            let mut cur_len = 0u32;

            buf.Lock(&mut ptr, Some(&mut max_len), Some(&mut cur_len))
                .context("IMFMediaBuffer::Lock")?;

            if max_len < annexb.len() as u32 {
                let _ = buf.Unlock();
                bail!("MF buffer too small: {} < {}", max_len, annexb.len());
            }

            std::ptr::copy_nonoverlapping(annexb.as_ptr(), ptr as *mut u8, annexb.len());
            buf.Unlock().context("IMFMediaBuffer::Unlock")?;
            buf.SetCurrentLength(annexb.len() as u32)
                .context("IMFMediaBuffer::SetCurrentLength")?;

            // windows-rs 0.60: MFCreateSample returns IMFSample directly.
            let s = MFCreateSample().context("MFCreateSample")?;
            s.AddBuffer(&buf).context("IMFSample::AddBuffer")?;

            // Media Foundation timestamps are in 100-ns units.
            s.SetSampleTime(sample.pts_us * 10)
                .context("IMFSample::SetSampleTime")?;
            if sample.dur_us > 0 {
                s.SetSampleDuration(sample.dur_us * 10)
                    .context("IMFSample::SetSampleDuration")?;
            }

            match self.dec.ProcessInput(0, &s, 0) {
                Ok(()) => Ok(true),
                Err(e) if e.code() == MF_E_NOTACCEPTING => Ok(false),
                Err(e) => Err(anyhow!("ProcessInput failed: {e}")),
            }
        }
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
        Ok(())
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

    let input_type = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let output_type = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };

    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_DECODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            Some(&input_type),
            Some(&output_type),
            &mut activates,
            &mut act_count,
        )
        .context("MFTEnumEx")?;
    }

    if act_count == 0 || activates.is_null() {
        bail!("no H.264 decoder MFT found");
    }

    let slice = unsafe { std::slice::from_raw_parts_mut(activates, act_count as usize) };
    let act = slice[0]
        .take()
        .ok_or_else(|| anyhow!("MFTEnumEx returned null activate"))?;

    let dec: IMFTransform = unsafe { act.ActivateObject::<IMFTransform>() }
        .context("ActivateObject::<IMFTransform>")?;

    unsafe { CoTaskMemFree(Some(activates as _)) };

    Ok(dec)
}


unsafe fn configure_decoder(dec: &IMFTransform, cfg: &H264Config) -> Result<()> {
    // Input type (H.264).
    let in_type = unsafe { MFCreateMediaType().context("MFCreateMediaType(in)")? };
    unsafe {
        in_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .context("SetGUID(MF_MT_MAJOR_TYPE)")?;
        in_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
            .context("SetGUID(MF_MT_SUBTYPE)")?;
    }

    // MFSetAttributeSize may not be available depending on bindings; set as UINT64 explicitly.
    let frame_size = ((cfg.width as u64) << 32) | (cfg.height as u64);
    unsafe {
        in_type
            .SetUINT64(&MF_MT_FRAME_SIZE, frame_size)
            .context("SetUINT64(MF_MT_FRAME_SIZE)")?;
    }

    let seq = cfg.annexb_sequence_header();

    unsafe {
        in_type
            .SetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, seq.as_slice())
            .context("SetBlob(MF_MT_MPEG_SEQUENCE_HEADER)")?;

        in_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .context("SetUINT32(MF_MT_INTERLACE_MODE)")?;

        dec.SetInputType(0, &in_type, 0)
            .context("IMFTransform::SetInputType")?;
    }

    // Output type (NV12).
    let out_type = unsafe { MFCreateMediaType().context("MFCreateMediaType(out)")? };

    unsafe {
        out_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .context("SetGUID(out MF_MT_MAJOR_TYPE)")?;
        out_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)
            .context("SetGUID(out MF_MT_SUBTYPE)")?;
    }

    unsafe {
        out_type
            .SetUINT64(&MF_MT_FRAME_SIZE, frame_size)
            .context("SetUINT64(out MF_MT_FRAME_SIZE)")?;
    }

    unsafe {
        out_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .context("SetUINT32(out MF_MT_INTERLACE_MODE)")?;
    }

    unsafe {
        dec.SetOutputType(0, &out_type, 0)
            .context("IMFTransform::SetOutputType")?;

        dec.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)
            .context("ProcessMessage(FLUSH)")?;
        dec.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
            .context("ProcessMessage(BEGIN_STREAMING)")?;
        dec.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
            .context("ProcessMessage(START_OF_STREAM)")?;
    }

    Ok(())
}

unsafe fn query_nv12_stride(dec: &IMFTransform, width: u32) -> Option<usize> {
    let mt = unsafe { dec.GetOutputCurrentType(0).ok()? };

    if let Ok(s) = unsafe { mt.GetUINT32(&MF_MT_DEFAULT_STRIDE) } {
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
    let info = unsafe { dec.GetOutputStreamInfo(0) }
        .context("GetOutputStreamInfo")?;

    let cb = if info.cbSize != 0 {
        info.cbSize
    } else {
        (stride * height as usize * 3 / 2) as u32
    };

    let buf = unsafe { MFCreateMemoryBuffer(cb) }
        .context("MFCreateMemoryBuffer(out)")?;

    let sample = unsafe { MFCreateSample() }.context("MFCreateSample(out)")?;
    unsafe { sample.AddBuffer(&buf) }.context("AddBuffer(out)")?;

    let mut out = MFT_OUTPUT_DATA_BUFFER {
        dwStreamID: 0,
        pSample: ManuallyDrop::new(Some(sample.clone())),
        dwStatus: 0,
        pEvents: ManuallyDrop::new(None),
    };
    let mut status: u32 = 0;

    match unsafe { dec.ProcessOutput(0, std::slice::from_mut(&mut out), &mut status) } {
        Ok(()) => {}
        Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(None),
        Err(e) => return Err(anyhow!("ProcessOutput failed: {e}")),
    }

    let out_buf = unsafe { sample.ConvertToContiguousBuffer() }
        .context("ConvertToContiguousBuffer")?;

    let mut ptr = std::ptr::null_mut();
    let mut max_len = 0u32;
    let mut cur_len = 0u32;
    unsafe { out_buf.Lock(&mut ptr, Some(&mut max_len), Some(&mut cur_len)) }
        .context("IMFMediaBuffer::Lock(out)")?;

    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, cur_len as usize) };

    let y_size = stride * height as usize;
    let uv_size = stride * height as usize / 2;
    if bytes.len() < y_size + uv_size {
        let _ = unsafe { out_buf.Unlock() };
        return Err(anyhow!("NV12 buffer too small: {}", bytes.len()));
    }

    let y = &bytes[..y_size];
    let uv = &bytes[y_size..y_size + uv_size];
    let rgba = nv12_to_rgba_strided(width, height, stride, stride, y, uv);

    let _ = unsafe { out_buf.Unlock() };

    let pts_100ns = unsafe { sample.GetSampleTime() }.unwrap_or(0);

    Ok(Some(VideoFrame {
        width,
        height,
        pts_us: pts_100ns / 10,
        rgba,
    }))
}


use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

use crate::backend::{H264Decoder, VideoFrame};
use crate::h264::H264Config;
use crate::mp4::EncodedSample;

pub struct GstH264Decoder {
    cfg: H264Config,
    pipeline: gst::Pipeline,
    appsrc: gst_app::AppSrc,
    appsink: gst_app::AppSink,
}

impl GstH264Decoder {
    pub fn new(cfg: H264Config) -> Result<Self> {
        gst::init().context("gst::init")?;

        let pipeline = gst::Pipeline::new();

        let appsrc = gst::ElementFactory::make("appsrc")
            .build()
            .context("create appsrc")?
            .downcast::<gst_app::AppSrc>()
            .map_err(|_| anyhow!("appsrc downcast"))?;

        let h264parse = gst::ElementFactory::make("h264parse")
            .build()
            .context("create h264parse")?;

        let decodebin = gst::ElementFactory::make("decodebin")
            .build()
            .context("create decodebin")?;

        let videoconvert = gst::ElementFactory::make("videoconvert")
            .build()
            .context("create videoconvert")?;

        let capsfilter = gst::ElementFactory::make("capsfilter")
            .build()
            .context("create capsfilter")?;

        let appsink = gst::ElementFactory::make("appsink")
            .build()
            .context("create appsink")?
            .downcast::<gst_app::AppSink>()
            .map_err(|_| anyhow!("appsink downcast"))?;

        // AppSrc caps: H.264 in AVCC format + codec_data from mp4.
        let codec_buf = gst::Buffer::from_slice(cfg.avcc.clone());
        let caps = gst::Caps::builder("video/x-h264")
            .field("stream-format", "avc")
            .field("alignment", "au")
            .field("codec_data", codec_buf)
            .build();
        appsrc.set_caps(Some(&caps));
        appsrc.set_is_live(true);
        appsrc.set_format(gst::Format::Time);

        // Appsink properties: do not sync to pipeline clock; we drive timing.
        appsink.set_property("sync", false);
        appsink.set_property("max-buffers", 2u32);
        appsink.set_property("drop", true);

        // Force RGBA output.
        let out_caps = gst::Caps::builder("video/x-raw")
            .field("format", "RGBA")
            .build();
        capsfilter.set_property("caps", &out_caps);

        pipeline
            .add_many([
                appsrc.upcast_ref(),
                &h264parse,
                &decodebin,
                &videoconvert,
                &capsfilter,
                appsink.upcast_ref(),
            ])
            .context("pipeline add")?;

        gst::Element::link_many([appsrc.upcast_ref(), &h264parse, &decodebin])
            .context("link appsrc->h264parse->decodebin")?;

        gst::Element::link_many([&videoconvert, &capsfilter, appsink.upcast_ref()])
            .context("link videoconvert->capsfilter->appsink")?;

        // decodebin dynamic pad -> videoconvert sink pad.
        let vc_sink = videoconvert.static_pad("sink").ok_or_else(|| anyhow!("videoconvert sink pad"))?;
        decodebin.connect_pad_added(move |_dbin, src_pad| {
            let _ = src_pad.link(&vc_sink);
        });

        pipeline
            .set_state(gst::State::Playing)
            .context("pipeline set Playing")?;

        Ok(Self {
            cfg,
            pipeline,
            appsrc,
            appsink,
        })
    }
}

impl Drop for GstH264Decoder {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

impl H264Decoder for GstH264Decoder {
    fn push(&mut self, sample: EncodedSample) -> Result<()> {
        let mut buf = gst::Buffer::from_slice(sample.data_avcc);
        {
            let b = buf.make_mut();
            if sample.pts_us >= 0 {
                b.set_pts(gst::ClockTime::from_nseconds((sample.pts_us as u64) * 1000));
            }
            if sample.dur_us > 0 {
                b.set_duration(gst::ClockTime::from_nseconds((sample.dur_us as u64) * 1000));
            }
        }
        self.appsrc
            .push_buffer(buf)
            .map_err(|e| anyhow!("appsrc push_buffer: {e:?}"))?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.appsrc.end_of_stream().map_err(|e| anyhow!("appsrc eos: {e:?}"))?;
        Ok(())
    }

    fn try_receive(&mut self) -> Result<Option<VideoFrame>> {
        let sample = match self.appsink.try_pull_sample(gst::ClockTime::from_mseconds(0)) {
            None => return Ok(None),
            Some(s) => s,
        };

        let caps = sample.caps().ok_or_else(|| anyhow!("appsink sample missing caps"))?;
        let info = gst_video::VideoInfo::from_caps(caps).map_err(|_| anyhow!("VideoInfo::from_caps"))?;

        let width = info.width();
        let height = info.height();
        let stride = info.stride()[0] as usize;

        let buffer = sample.buffer().ok_or_else(|| anyhow!("appsink sample missing buffer"))?;
        let map = buffer.map_readable().map_err(|_| anyhow!("buffer map_readable"))?;
        let data = map.as_slice();

        let row_bytes = (width as usize) * 4;
        let mut rgba = vec![0u8; row_bytes * (height as usize)];
        if stride == row_bytes {
            rgba.copy_from_slice(&data[..rgba.len()]);
        } else {
            for y in 0..(height as usize) {
                let src_off = y * stride;
                let dst_off = y * row_bytes;
                rgba[dst_off..dst_off + row_bytes]
                    .copy_from_slice(&data[src_off..src_off + row_bytes]);
            }
        }

        let pts_us = buffer
            .pts()
            .map(|t| (t.nseconds().unwrap_or(0) / 1000) as i64)
            .unwrap_or(0);

        Ok(Some(VideoFrame {
            width,
            height,
            pts_us,
            rgba,
        }))
    }
}

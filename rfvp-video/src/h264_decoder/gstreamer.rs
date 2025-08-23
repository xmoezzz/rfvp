use std::io::{Read, Seek};

use anyhow::{bail, Context, Result};
use gst::prelude::*;
use once_cell::sync::Lazy;
use tracing::{debug, error, trace, warn};

use crate::{
    h264_decoder::{BitsPerSample, Colorspace, Frame, FrameSize, FrameTiming, PlaneSize},
    mp4::Mp4TrackReader,
    mp4_bitstream_converter::Mp4BitstreamConverter,
};

// NOTE: doing this one-time init in a library is bad practice
// but we want to abstract away the gstreamer dependency, so we do it in hopes no other dep uses GStreamer

static INIT_ONCE: Lazy<()> = Lazy::new(|| {
    tracing_gstreamer::integrate_events();
    // gst::debug_remove_default_log_function();
    // gst::debug_set_default_threshold(gst::DebugLevel::Memdump);
    gst::init().expect("Failed to initialize GStreamer");
});
fn init() {
    Lazy::force(&INIT_ONCE);
}

pub struct GStreamerH264Decoder {
    #[allow(dead_code)] // it's required to keep the pipeline alive
    pipeline: gst::Pipeline,
    app_sink: gst_app::AppSink,
    frame_size: FrameSize,
    saved_frame: Option<gst::Sample>,
    time_base: u32,
    frame_number: u32,
}

impl super::H264DecoderTrait for GStreamerH264Decoder {
    fn new<S: Read + Seek + Send + 'static>(mut track: Mp4TrackReader<S>) -> Result<Self> {
        init();

        let (major, minor, micro, nano) = gst::version();
        debug!(
            "Creating GStreamerH264Decoder with GStreamer version {}.{}.{}.{}",
            major, minor, micro, nano
        );

        let in_video_caps = gst::caps::Caps::builder("video/x-h264")
            .field("stream-format", "byte-stream")
            .field("alignment", "au")
            .build();

        let app_src = gst_app::AppSrc::builder()
            .caps(&in_video_caps)
            .format(gst::Format::Time)
            .build();

        let decodebin = gst::ElementFactory::make("decodebin")
            .build()
            .context("Failed to create decodebin")?;
        let queue = gst::ElementFactory::make("queue")
            .build()
            .context("Failed to create queue")?;
        let videoconvert = gst::ElementFactory::make("videoconvert")
            .build()
            .context("Failed to create videoconvert")?;

        let out_video_caps = gst::caps::Caps::builder("video/x-raw")
            .field("format", "I420")
            .field("interlace-mode", "progressive")
            .build();

        let app_sink = gst_app::AppSink::builder()
            .caps(&out_video_caps)
            .drop(false)
            .build();

        let pipeline = gst::Pipeline::default();
        pipeline
            .add_many([
                app_src.upcast_ref(),
                &decodebin,
                &queue,
                &videoconvert,
                app_sink.upcast_ref(),
            ])
            .context("Failed to add elements to pipeline")?;
        // NOTE: we cannot link the whole pipeline here, because decodebin doesn't create output pads before it receives data
        // we need to handle the "pad-added" signal from decodebin and link the elements then
        app_src
            .link(&decodebin)
            .context("Failed to link app_src to decodebin")?;
        gst::Element::link_many([&queue, &videoconvert, app_sink.upcast_ref()])
            .context("Failed to link queue, videoconvert and appsink")?;

        decodebin.connect_pad_added(move |_decodebin, src_pad| {
            let is_video = {
                let media_type = src_pad
                    .current_caps()
                    .and_then(|caps| caps.structure(0).map(|s| s.name().starts_with("video/")));

                match media_type {
                    None => {
                        warn!("Failed to get media type from pad {}", src_pad.name());

                        return;
                    }
                    Some(media_type) => media_type,
                }
            };

            let insert_sink = || -> Result<()> {
                let sink_pad = queue.static_pad("sink").expect("queue has no sinkpad");
                src_pad.link(&sink_pad)?;

                Ok(())
            };

            if is_video {
                if let Err(err) = insert_sink() {
                    // TODO: do we care to use the bus?
                    error!("Failed to insert sink: {:?}", err);
                }
            }
        });

        // how many time units are there in 1 second
        let time_base = track.get_mp4_track_info(|track| track.timescale());

        // define data callback for AppSrc
        {
            let mut bitstream_converter: Mp4BitstreamConverter =
                track.get_mp4_track_info(Mp4BitstreamConverter::for_mp4_track);
            let mut vec_buffer = Vec::new();

            app_src.set_callbacks(
                gst_app::AppSrcCallbacks::builder()
                    .need_data(move |app_src, _| match track.next_sample() {
                        Ok(Some(sample)) => {
                            let mp4_time_to_gst_time = |time| {
                                gst::ClockTime::from_useconds(time * 1_000_000 / time_base as u64)
                            };

                            let start_time = mp4_time_to_gst_time((sample.start_time as i64 + sample.rendering_offset as i64) as u64);
                            let duration = mp4_time_to_gst_time(sample.duration as u64);

                            bitstream_converter.convert_packet(&sample.bytes, &mut vec_buffer);
                            let mut buffer = gst::Buffer::with_size(vec_buffer.len())
                                .expect("Failed to create buffer");
                            {
                                let buffer = buffer.get_mut().unwrap();
                                buffer
                                    .copy_from_slice(0, &vec_buffer)
                                    .expect("Failed to copy data to buffer");
                                buffer.set_pts(start_time);
                                buffer.set_duration(duration);
                            }

                            let _ = app_src.push_buffer(buffer);
                        }
                        Ok(None) => {
                            let _ = app_src.end_of_stream();
                        }
                        Err(e) => {
                            error!("Error reading sample from mp4: {:?}; ending the stream", e);
                            let _ = app_src.end_of_stream();
                        }
                    })
                    .build(),
            );
        }

        pipeline
            .set_state(gst::State::Playing)
            .context("Failed to set pipeline state to playing")?;

        let sample = app_sink.pull_sample().unwrap();
        let frame_size = {
            let caps = sample.caps().context("Failed to get caps from sample")?;
            let video_info = gst_video::VideoInfo::from_caps(caps)
                .context("Failed to get video info from caps")?;
            if video_info.format() != gst_video::VideoFormat::I420 {
                bail!("Unsupported video format: {:?}", video_info.format())
            }
            if video_info.interlace_mode() != gst_video::VideoInterlaceMode::Progressive {
                bail!(
                    "Unsupported interlace mode: {:?}",
                    video_info.interlace_mode()
                )
            }
            // if video_info.chroma_site() != gst_video::VideoChromaSite::MPEG2 {
            //     bail!("Unsupported chroma site: {:?}", video_info.chroma_site())
            // }
            // ehh,, i dunno how to compare it
            // assert_eq!(video_info.colorimetry(), gst_video::VideoColorimetry::);
            // assert_eq!(video_info.pixel_aspect_ratio(), gst::Fraction::new(1, 1));

            let width = video_info.width();
            let height = video_info.height();

            FrameSize {
                colorspace: Colorspace::C420mpeg2,
                plane_sizes: [
                    PlaneSize::new(width, height, BitsPerSample::B8),
                    PlaneSize::new(width / 2, height / 2, BitsPerSample::B8),
                    PlaneSize::new(width / 2, height / 2, BitsPerSample::B8),
                ],
            }
        };

        Ok(Self {
            pipeline,
            app_sink,
            frame_size,
            saved_frame: Some(sample),
            time_base,
            frame_number: 0,
        })
    }

    fn read_frame(&mut self) -> Result<Option<(FrameTiming, Frame)>> {
        if self.app_sink.is_eos() {
            return Ok(None);
        }

        let sample = if let Some(sample) = self.saved_frame.take() {
            sample
        } else {
            self.app_sink
                .pull_sample()
                .context("Failed to pull sample")?
        };

        let buffer = sample.buffer().unwrap();
        // TODO: suboptimal: FrameTimings should already be in an absolute time unit
        let gst_time_to_mp4_time =
            |time: gst::ClockTime| (time.useconds() * self.time_base as u64 / 1_000_000);
        let start_time = gst_time_to_mp4_time(buffer.pts().unwrap());
        let duration = gst_time_to_mp4_time(buffer.duration().unwrap_or_default())
            .try_into()
            .unwrap();

        let frame_timing = FrameTiming {
            frame_number: self.frame_number,
            start_time,
            duration,
        };

        self.frame_number += 1;
        trace!(
            "Got a new frame! (frame #{}, start_time={})",
            self.frame_number,
            start_time
        );

        let [y_len, uv_len, _] = self.frame_size.plane_sizes.map(|v| v.get_bytes_len());

        let mut y_data = vec![0; y_len];
        let mut u_data = vec![0; uv_len];
        let mut v_data = vec![0; uv_len];

        buffer.copy_to_slice(0, &mut y_data).unwrap();
        buffer.copy_to_slice(y_len, &mut u_data).unwrap();
        buffer.copy_to_slice(y_len + uv_len, &mut v_data).unwrap();

        let frame = Frame::new([y_data, u_data, v_data], None, self.frame_size);

        Ok(Some((frame_timing, frame)))
    }

    fn frame_size(&mut self) -> Result<FrameSize> {
        Ok(self.frame_size)
    }
}

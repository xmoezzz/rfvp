use std::io::{Read, Seek};

use anyhow::{Context, Result};
use glam::Mat4;
use kira::track::TrackId;
use rfvp_audio::{AudioData, AudioManager, AudioSettings};
use rfvp_core::{
    time::{Ticks, Tween},
    types::{Pan, Volume},
};
use rfvp_render::{GpuCommonResources, Renderable, SpriteVertexBuffer};
use tracing::{error, info, trace, warn};

use crate::{
    audio::AacFrameSource,
    h264_decoder::{Frame, FrameTiming, H264Decoder, H264DecoderTrait},
    mp4::Mp4,
    timer::Timer,
    YuvTexture,
};

pub struct VideoPlayer {
    timer: Timer,
    video_decoder: H264Decoder,
    video_texture: YuvTexture,
    vertex_buffer: SpriteVertexBuffer,
    pending_frame: Option<(FrameTiming, Frame)>,
}

impl VideoPlayer {
    pub fn new<S: Read + Seek + Send + 'static>(
        resources: &GpuCommonResources,
        audio_manager: &AudioManager,
        mp4: Mp4<S>,
    ) -> Result<VideoPlayer> {
        let time_base = mp4
            .video_track
            .get_mp4_track_info(|track| track.timescale());

        let start = std::time::Instant::now();
        let mut video_decoder =
            H264Decoder::new(mp4.video_track).context("Initializing H264Decoder")?;
        let pending_frame = video_decoder.read_frame().context("Reading first frame")?;
        let duration = start.elapsed();

        info!("H264Decoder::new took {:?}", duration);
        let frame_size = video_decoder
            .frame_size()
            .context("Getting H264 frame size")?;

        let video_texture = YuvTexture::new(resources, frame_size.clone());

        // TODO: use the audio track
        // if we are using audio the timer should be tracking the audio playback
        let audio_handle = if let Some(track) = mp4.audio_track {
            let frame_source = AacFrameSource::new(track).context("Initializing AacFrameSource")?;
            Some(audio_manager.play(AudioData {
                source: frame_source,
                settings: AudioSettings {
                    fade_in: Tween::MS_15,
                    loop_start: None,
                    volume: Volume::default(),
                    pan: Pan::default(),
                },
            }))
        } else {
            None
        };

        let timer = match audio_handle {
            Some(handle) => Timer::new_audio_tied(time_base, handle),
            None => Timer::new_independent(time_base),
        };

        // TODO :
        let vertex_buffer = SpriteVertexBuffer::new_fullscreen(
            resources,
            frame_size.plane_sizes[0].width,
            frame_size.plane_sizes[0].height,
        );

        Ok(VideoPlayer {
            timer,
            video_decoder,
            video_texture,
            vertex_buffer,
            pending_frame,
        })
    }

    pub fn update(&mut self, delta_time: Ticks, queue: &wgpu::Queue) {
        self.timer.update(delta_time);
        let current_time = self.timer.time();

        let mut skipped_frames = 0;
        // find the latest frame that is ready for display
        // this might be the currently pending frame, or any of the frames after it (shouldn't happen often I think)
        while let Some((timing, ref frame)) = self.pending_frame {
            // if it's not time to display the frame yet - stop the loop
            if timing.start_time > current_time {
                // very noisy
                // trace!(
                //     "Not time to display frame #{}, time: {}",
                //     timing.frame_number,
                //     timing.start_time
                // );
                break;
            }

            // look at the frame after the pending one
            let next_frame = match self.video_decoder.read_frame() {
                Ok(frame) => frame,
                Err(err) => {
                    error!("Error reading frame: {}. Stopping playback", err);
                    None
                }
            };

            // if the next frame is not ready for display yet...
            if next_frame
                .as_ref()
                .map_or(true, |(timing, _)| timing.start_time > current_time)
            {
                if skipped_frames > 0 {
                    warn!("Skipped {} frames", skipped_frames);
                }

                // then update the texture with the pending frame
                trace!(
                    "Displaying frame #{}, time: {}",
                    timing.frame_number,
                    timing.start_time
                );
                self.video_texture.write_data(frame, queue);
                // the loop will not enter again, so the pending frame will now be displayed
            } else {
                skipped_frames += 1;
                // if the next frame is also ready for display, then we should skip the pending frame
            }

            if next_frame.is_none() {
                info!("No more frames, stopping playback");
            }

            self.pending_frame = next_frame;
        }
    }

    pub fn is_finished(&self) -> bool {
        self.pending_frame.is_none()
    }
}

// or should it just provide a renderable texture?
// depends on how will the generic layer rendering will be implemented...
impl Renderable for VideoPlayer {
    fn render<'enc>(
        &'enc self,
        resources: &'enc GpuCommonResources,
        render_pass: &mut wgpu::RenderPass<'enc>,
        transform: Mat4,
        projection: Mat4,
    ) {
        let total_transform = projection * transform;
        resources.draw_yuv_sprite(
            render_pass,
            self.vertex_buffer.vertex_source(),
            self.video_texture.bind_group(),
            total_transform,
        );
    }

    fn resize(&mut self, _resources: &GpuCommonResources) {}
}

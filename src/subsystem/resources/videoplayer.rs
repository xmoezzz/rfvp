use anyhow::Result;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::media::Type;
use ffmpeg_next::software::scaling::{context::Context, flag::Flags};
use ffmpeg_next::util::frame::video::Video;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::{spawn, JoinHandle};

pub struct VideoPlayer {
    pub width: u32,
    pub height: u32,
    pub playing: bool,
    pub pixel_data: Vec<u8>,
}

impl VideoPlayer {
    pub fn new() -> Result<Self> {
        ffmpeg_next::init()?;

        let player = Self {
            width: 0,
            height: 0,
            playing: false,
            pixel_data: Vec::new(),
        };

        Ok(player)
    }

    fn play_threaded(
        &mut self,
        path: impl AsRef<Path>,
        width: u32,
        height: u32,
        should_play: Arc<AtomicBool>,
    ) -> Result<()> {
        let mut ctx = ffmpeg_next::format::input(&path)?;
        let input = ctx
            .streams()
            .best(Type::Video)
            .ok_or(ffmpeg_next::Error::StreamNotFound)?;

        let video_stream_index = input.index();

        let context_decoder =
            ffmpeg_next::codec::context::Context::from_parameters(input.parameters())?;
        let mut decoder = context_decoder.decoder().video()?;

        let mut scaler = Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            Pixel::RGB24,
            width,
            height,
            Flags::BILINEAR,
        )?;

        self.width = decoder.width();
        self.height = decoder.height();

        let mut frame_index = 0;
        self.playing = true;
        let mut receive_and_process_decoded_frames =
            |decoder: &mut ffmpeg_next::decoder::Video| -> Result<(), ffmpeg_next::Error> {
                let mut decoded = Video::empty();
                while decoder.receive_frame(&mut decoded).is_ok() {
                    let mut rgb_frame = Video::empty();
                    scaler.run(&decoded, &mut rgb_frame)?;
                    self.render_video_to_texture(&rgb_frame, frame_index);

                    frame_index += 1;
                }
                Ok(())
            };

        for (stream, packet) in ctx.packets() {
            if !should_play.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            if stream.index() == video_stream_index {
                if let Err(e) = decoder.send_packet(&packet) {
                    log::error!("Error sending packet to decoder: {}", e);
                }
                if let Err(e) = receive_and_process_decoded_frames(&mut decoder) {
                    log::error!("Error receiving and processing decoded frames: {}", e);
                }
            }
        }

        if let Err(e) = decoder.send_eof() {
            log::error!("Error sending EOF to decoder: {}", e);
        }
        if let Err(e) = receive_and_process_decoded_frames(&mut decoder) {
            log::error!("Error receiving and processing decoded frames: {}", e);
        }

        self.playing = false;
        Ok(())
    }

    fn render_video_to_texture(&mut self, video: &Video, _index: i32) {
        let data = video.data(0);
        self.pixel_data = data.to_vec();
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }
}

pub struct VideoPlayerManager {
    player_thread: Option<JoinHandle<()>>,
    should_play: Arc<AtomicBool>,
}

impl VideoPlayerManager {
    pub fn new() -> Self {
        Self {
            player_thread: None,
            should_play: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn play(&mut self, path: impl AsRef<Path>, width: u32, height: u32) -> Result<()> {
        let mut player = VideoPlayer::new()?;
        let path = path.as_ref().to_path_buf();
        let flag = self.should_play.clone();
        let player_thread = spawn(move || {
            player.play_threaded(path, width, height, flag).unwrap();
        });
        self.player_thread = Some(player_thread);
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        if let Some(player_thread) = &self.player_thread {
            !player_thread.is_finished()
        } else {
            false
        }
    }

    pub fn stop(&mut self) {
        if let Some(player_thread) = self.player_thread.take() {
            self.should_play
                .store(false, std::sync::atomic::Ordering::Relaxed);
            player_thread.join();
        }
    }
}

impl Default for VideoPlayerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use winit::event::{Event, WindowEvent};

    use super::*;

    // #[test]
    // fn test_video_player() {
    //     let event_loop = winit::event_loop::EventLoop::new().expect("Event loop could not be created");
    //     event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    //     // 1024 × 640
    //     let window_builder: winit::window::WindowBuilder = winit::window::WindowBuilder::new()
    //         .with_title("app".to_string())
    //         .with_inner_size(winit::dpi::LogicalSize::new(1024, 640));

    //     let window = window_builder
    //         .build(&event_loop)
    //         .expect("An error occured while building the main game window");

    //     // init wgpu
    //     let backend = wgpu::util::backend_bits_from_env().unwrap_or_else(wgpu::Backends::all);
    //     let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
    //         backends: backend,
    //         dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
    //         flags: wgpu::InstanceFlags::default(),
    //         gles_minor_version: wgpu::Gles3MinorVersion::Automatic
    //     });

    //     let (_size, surface) = unsafe {
    //         let size = window.inner_size();
    //         let surface = instance.create_surface(window).expect("Surface unsupported by adapter");
    //         (size, surface)
    //     };

    //     //render video
    //     event_loop
    //         .run(move |event, target| {
    //             // Have the closure take ownership of the resources.
    //             // `event_loop.run` never returns, therefore we must do this to ensure
    //             // the resources are properly cleaned up.

    //             if let Event::WindowEvent { window_id, event } = event {
    //                 match event {
    //                     WindowEvent::Resized(new_size) => {
    //                     }
    //                     WindowEvent::RedrawRequested => {

    //                         // frame.present();

    //                     }
    //                     WindowEvent::CloseRequested => {
    //                     }
    //                     _ => {}
    //                 }
    //             }
    //         })
    //         .unwrap();

    // }
}

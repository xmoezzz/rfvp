use std::{collections::VecDeque, env};
use std::time::Instant;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use video_sys::VideoStream;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

static VERTEX_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRS,
        }
    }
}

struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,

    pipeline: wgpu::RenderPipeline,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    icount: u32,

    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,

    video: VideoStream,

    stash: std::collections::VecDeque<video_sys::VideoFrame>,
    started_at: Option<std::time::Instant>,
    base_pts_us: Option<i64>,
    last_presented_pts: i64,

    render_fps_t0: Instant,
    render_fps_n: u32,

    video_fps_t0: Instant,
    video_fps_n: u32,
    video_delta_sum_us: i64,
    video_delta_n: u32,

}

impl State {
    async fn new(window: Arc<winit::window::Window>, path: &str) -> Result<Self> {
        let size = window.inner_size();

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).context("create_surface")?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("request_adapter")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .context("request_device")?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Load video.
        let video = VideoStream::open(path).context("VideoStream::open")?;
        let tex_w = video.width();
        let tex_h = video.height();

        // Texture for RGBA frames.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("video_texture"),
            size: wgpu::Extent3d {
                width: tex_w,
                height: tex_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Shaders/pipeline.
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let vertices = [
            Vertex { pos: [-1.0, -1.0], uv: [0.0, 1.0] },
            Vertex { pos: [ 1.0, -1.0], uv: [1.0, 1.0] },
            Vertex { pos: [ 1.0,  1.0], uv: [1.0, 0.0] },
            Vertex { pos: [-1.0,  1.0], uv: [0.0, 0.0] },
        ];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vbuf"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ibuf"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            vbuf,
            ibuf,
            icount: indices.len() as u32,
            texture,
            texture_view,
            sampler,
            bind_group,
            video,
            stash: VecDeque::new(),
            started_at: None,
            base_pts_us: None,
            last_presented_pts: -1,
            render_fps_t0: Instant::now(),
            render_fps_n: 0,
            video_fps_t0: Instant::now(),
            video_fps_n: 0,
            video_delta_sum_us: 0,
            video_delta_n: 0,
        })
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn update(&mut self) -> Result<()> {
        // 1) Drain decoded frames from VideoStream into local stash.
        while let Some(f) = self.video.try_recv_one() {
            self.stash.push_back(f);
        }

        // 2) Initialize timing on first frame.
        if self.started_at.is_none() {
            if let Some(front) = self.stash.front() {
                self.started_at = Some(std::time::Instant::now());
                self.base_pts_us = Some(front.pts_us);
            } else {
                return Ok(());
            }
        }

        let started_at = self.started_at.unwrap();
        let elapsed_us = started_at.elapsed().as_micros() as i64;
        let base = self.base_pts_us.unwrap();
        let target_pts_us = base + elapsed_us;

        // 3) Pop all frames that are due; keep latest.
        let mut latest_due = None;
        while let Some(front) = self.stash.front() {
            if front.pts_us <= target_pts_us {
                latest_due = self.stash.pop_front();
            } else {
                break;
            }
        }

        if let Some(frame) = latest_due {
            if frame.pts_us != self.last_presented_pts {
                let dt = frame.pts_us - self.last_presented_pts;
                log::info!("present dt={}us", dt);
                self.last_presented_pts = frame.pts_us;
                self.upload_frame(&frame.data, frame.width, frame.height);
            }
        }

        Ok(())
    }


    fn upload_frame(&mut self, rgba: &[u8], width: u32, height: u32) {
        // wgpu requires bytes_per_row to be a multiple of 256.
        let bytes_per_pixel = 4usize;
        let row_bytes = width as usize * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padded_row_bytes = (row_bytes + align - 1) / align * align;

        if padded_row_bytes == row_bytes {
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                rgba,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(row_bytes as u32),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            return;
        }

        let mut padded = vec![0u8; padded_row_bytes * height as usize];
        for y in 0..height as usize {
            let src = &rgba[y * row_bytes..(y + 1) * row_bytes];
            let dst = &mut padded[y * padded_row_bytes..y * padded_row_bytes + row_bytes];
            dst.copy_from_slice(src);
        }

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &padded,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes as u32),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn render(&mut self) -> Result<()> {
        let output = self.surface.get_current_texture().context("get_current_texture")?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("encoder"),
        });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.bind_group, &[]);
            rpass.set_vertex_buffer(0, self.vbuf.slice(..));
            rpass.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint16);
            rpass.draw_indexed(0..self.icount, 0, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let mut args = env::args().skip(1);
    let path = args.next().context("Usage: video-player-wgpu <file.mp4>")?;

    let event_loop = EventLoop::new()?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("video-player-wgpu")
            .build(&event_loop)?,
    );

    let mut state = pollster::block_on(State::new(window.clone(), &path))?;

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(winit::event_loop::ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(s) => state.resize(s),
                // WindowEvent::ScaleFactorChanged { new_inner_size, .. } => state.resize(*new_inner_size),
                WindowEvent::RedrawRequested => {
                    if let Err(e) = state.update().and_then(|_| state.render()) {
                        log::error!("render error: {e:?}");
                    }
                }
                _ => {}
            },
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    })?;

    Ok(())
}

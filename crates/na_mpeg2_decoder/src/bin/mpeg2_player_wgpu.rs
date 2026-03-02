use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver};
use na_mpeg2_decoder::{MpegAudioF32, MpegAvEvent, MpegAvPipeline, MpegRgbaFrame};
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        // Helpful defaults to surface wgpu validation errors.
        std::env::set_var("RUST_LOG", "info,wgpu_core=warn,wgpu_hal=warn");
    }
    env_logger::init();

    let path = std::env::args().nth(1).map(PathBuf::from).expect("usage: mpeg2_player_wgpu <file.ts|ps|es>");

    let (tx_v, rx_v) = unbounded::<MpegRgbaFrame>();
    let (tx_a, rx_a) = unbounded::<MpegAudioF32>();

    std::thread::spawn(move || {
        let mut f = File::open(&path).expect("open input");
        let mut buf = vec![0u8; 1024 * 1024];
        let mut pipe = MpegAvPipeline::new();

        loop {
            let n = f.read(&mut buf).expect("read");
            if n == 0 {
                break;
            }
            let chunk = &buf[..n];
            if let Err(e) = pipe.push_with(chunk, None, |ev| match ev {
                MpegAvEvent::Video(v) => {
                    let _ = tx_v.send(v);
                }
                MpegAvEvent::Audio(a) => {
                    let _ = tx_a.send(a);
                }
            }) {
                log::error!("pipeline error: {e:?}");
                break;
            }
        }

        let _ = pipe.flush_with(|ev| match ev {
            MpegAvEvent::Video(v) => {
                let _ = tx_v.send(v);
            }
            MpegAvEvent::Audio(a) => {
                let _ = tx_a.send(a);
            }
        });
    });

    std::thread::spawn(move || audio_thread(rx_a));
    pollster::block_on(run(rx_v));
}

fn audio_thread(rx: Receiver<MpegAudioF32>) {
    let Ok((_stream, handle)) = rodio::OutputStream::try_default() else {
        log::warn!("no audio output device");
        return;
    };
    let Ok(sink) = rodio::Sink::try_new(&handle) else {
        log::warn!("failed to create audio sink");
        return;
    };

    let mut seen = 0usize;
    while let Ok(ch) = rx.recv() {
        seen += 1;
        if seen == 1 {
            log::info!(
                "audio: first chunk pts_ms={} rate={} ch={} samples={} (f32 interleaved)",
                ch.pts_ms,
                ch.sample_rate,
                ch.channels,
                ch.samples.len()
            );
        }
        if ch.channels == 0 || ch.sample_rate == 0 {
            continue;
        }

        // Demo path: queue decoded PCM as it arrives.
        let src = rodio::buffer::SamplesBuffer::new(ch.channels as u16, ch.sample_rate, ch.samples);
        sink.append(src);
    }

    // Let queued samples finish.
    sink.sleep_until_end();
}

async fn run(rx: Receiver<MpegRgbaFrame>) {
    let event_loop = EventLoop::new().unwrap();
    // wgpu 0.19 surface can borrow the window; we keep a 'static window to satisfy lifetimes.
    let window = WindowBuilder::new().with_title("mpeg2_player_wgpu").with_inner_size(PhysicalSize::new(1280, 720)).build(&event_loop).unwrap();
    let window: &'static winit::window::Window = Box::leak(Box::new(window));

    let instance = wgpu::Instance::default();
    let surface = unsafe { instance.create_surface(window) }.unwrap();

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .unwrap();

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
            },
            None,
        )
        .await
        .unwrap();

    let size = window.inner_size();
    let caps = surface.get_capabilities(&adapter);
    let format = caps.formats[0];

    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: caps.present_modes[0],
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    // Shader.
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blit"),
        source: wgpu::ShaderSource::Wgsl(include_str!("./shader_blit.wgsl").into()),
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("sampler"),
        ..Default::default()
    });

    // Placeholder texture (1x1).
    let mut tex_w: u32 = 1;
    let mut tex_h: u32 = 1;
    let mut texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("video_tex"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

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

    let mut bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bg"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&texture_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pl"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("rp"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let mut pending: VecDeque<MpegRgbaFrame> = VecDeque::new();
    let mut t0: Option<Instant> = None;
    let mut pts0: i64 = 0;

    // Staging buffer for row padding required by wgpu texture uploads.
    // bytes_per_row must be a multiple of 256.
    let mut upload_staging: Vec<u8> = Vec::new();
    let mut upload_bpr: u32 = 0;
    let mut seen_video = 0usize;

    event_loop
        .run(move |event, elwt| {
            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::Resized(new_size) => {
                        config.width = new_size.width.max(1);
                        config.height = new_size.height.max(1);
                        surface.configure(&device, &config);
                    }
                    WindowEvent::RedrawRequested => {
                    // Drain decoded frames.
                    while let Ok(f) = rx.try_recv() {
                        pending.push_back(f);
                    }

                    // Present due frame.
                    if let Some(front) = pending.front() {
                        if t0.is_none() {
                            t0 = Some(Instant::now());
                            pts0 = front.pts_ms;
                        }
                        let due = t0.unwrap() + Duration::from_millis((front.pts_ms - pts0).max(0) as u64);
                        if Instant::now() >= due {
                            let f = pending.pop_front().unwrap();
                            seen_video += 1;
                            if seen_video == 1 {
                                let sample0 = f.rgba.get(0).copied().unwrap_or(0);
                                log::info!("video: first frame {}x{} pts_ms={} rgba_len={} sample0={}", f.width, f.height, f.pts_ms, f.rgba.len(), sample0);
                            }

                            if f.width != tex_w || f.height != tex_h {
                                tex_w = f.width;
                                tex_h = f.height;
                                texture = device.create_texture(&wgpu::TextureDescriptor {
                                    label: Some("video_tex"),
                                    size: wgpu::Extent3d { width: tex_w, height: tex_h, depth_or_array_layers: 1 },
                                    mip_level_count: 1,
                                    sample_count: 1,
                                    dimension: wgpu::TextureDimension::D2,
                                    format: wgpu::TextureFormat::Rgba8Unorm,
                                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                                    view_formats: &[],
                                });
                                texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                                bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                                    label: Some("bg"),
                                    layout: &bind_group_layout,
                                    entries: &[
                                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&texture_view) },
                                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                                    ],
                                });
                                // Force staging recompute.
                                upload_bpr = 0;
                            }

                            let unpadded_bpr = 4u32.saturating_mul(tex_w);
                            let padded_bpr = ((unpadded_bpr + 255) / 256) * 256;
                            if padded_bpr != upload_bpr {
                                upload_bpr = padded_bpr;
                                upload_staging.resize((upload_bpr as usize) * (tex_h as usize), 0);
                            }

                            let upload_bytes: &[u8] = if padded_bpr == unpadded_bpr {
                                &f.rgba
                            } else {
                                // Row-pad into staging buffer.
                                let row_src = unpadded_bpr as usize;
                                let row_dst = padded_bpr as usize;
                                for y in 0..(tex_h as usize) {
                                    let src0 = y * row_src;
                                    let dst0 = y * row_dst;
                                    upload_staging[dst0..dst0 + row_src].copy_from_slice(&f.rgba[src0..src0 + row_src]);
                                }
                                &upload_staging
                            };

                            queue.write_texture(
                                wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                                upload_bytes,
                                wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(upload_bpr), rows_per_image: Some(tex_h) },
                                wgpu::Extent3d { width: tex_w, height: tex_h, depth_or_array_layers: 1 },
                            );
                        }
                    }

                    // Render.
                    let frame = match surface.get_current_texture() {
                        Ok(frame) => frame,
                        Err(_) => {
                            surface.configure(&device, &config);
                            return;
                        }
                    };
                    let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

                    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                    {
                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: None,
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        rpass.set_pipeline(&render_pipeline);
                        rpass.set_bind_group(0, &bind_group, &[]);
                        rpass.draw(0..3, 0..1);
                    }

                    queue.submit(Some(encoder.finish()));
                    frame.present();
                    }
                    _ => {}
                },
                Event::AboutToWait => {
                    window.request_redraw();
                }
                _ => {}
            }
        })
        .unwrap();
}

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::Context;
use ringbuf::traits::{Consumer, Producer, Split};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

use wmv_decoder::{
    asf::{AsfFile, AudioStreamInfo, VideoStreamInfo},
    AsfWmaDecoder, AsfWmv2Decoder,
};

fn main() {
    env_logger::init();

    let mut args = std::env::args();
    let _exe = args.next();
    let Some(path) = args.next() else {
        eprintln!("Usage: wmv-player-wgpu <input.wmv>");
        std::process::exit(2);
    };
    let input_path = PathBuf::from(path);

    let (video_info, audio_info) = match probe_streams(&input_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("stream probe failed: {e:?}");
            std::process::exit(1);
        }
    };

    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new()
        .with_title("wmv-player-wgpu")
        .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 640.0))
        .build(&event_loop)
        .expect("create window");

    let renderer = pollster::block_on(Renderer::new(window, video_info.width, video_info.height))
        .expect("renderer init");

    let stop = Arc::new(AtomicBool::new(false));

    // Shared A/V start clock. Video sets it on first decoded frame; audio waits for it.
    let clock = Arc::new(AvStartClock::new());

    // Video decode thread.
    let (video_tx, video_rx) = crossbeam_channel::bounded::<VideoFrame>(3);
    let _video_join = spawn_wmv2_decode_thread(input_path.clone(), video_tx, stop.clone(), clock.clone());

    // Audio output + decode thread (optional).
    let (_audio, _audio_join) = match audio_info {
        Some(ai) => match AudioOutput::new_best_effort(ai.sample_rate, ai.channels as usize) {
            Ok((ao, prod, out_channels, out_rate)) => {
                let j = spawn_wma_decode_thread(
                    input_path.clone(),
                    prod,
                    out_channels,
                    out_rate,
                    stop.clone(),
                    clock.clone(),
                );
                (Some(ao), Some(j))
            }
            Err(e) => {
                eprintln!("[audio] output init failed: {e:?}");
                (None, None)
            }
        },
        None => (None, None),
    };

    let mut state = PlayerState {
        renderer,
        video_rx,
        stop,
    };

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    state.stop.store(true, Ordering::Relaxed);
                    elwt.exit();
                }
                WindowEvent::Resized(size) => {
                    state.renderer.resize(size.width, size.height);
                }
                WindowEvent::RedrawRequested => {
                    while let Ok(vf) = state.video_rx.try_recv() {
                        state.renderer.upload_frame_mut(&vf);
                    }
                    let _ = state.renderer.render();
                }
                _ => {}
            },
            Event::AboutToWait => {
                state.renderer.request_redraw();
            }
            Event::LoopExiting => {
                state.stop.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    });
}

struct PlayerState {
    renderer: Renderer,
    video_rx: crossbeam_channel::Receiver<VideoFrame>,
    stop: Arc<AtomicBool>,
}

struct AvStartClock {
    inner: Mutex<Option<(Instant, u32)>>,
    cv: Condvar,
}

impl AvStartClock {
    fn new() -> Self {
        Self {
            inner: Mutex::new(None),
            cv: Condvar::new(),
        }
    }

    fn set_once(&self, t0: Instant, pts0: u32) {
        let mut g = self.inner.lock().unwrap();
        if g.is_none() {
            *g = Some((t0, pts0));
            self.cv.notify_all();
        }
    }

    fn wait_ready(&self, stop: &AtomicBool) -> Option<(Instant, u32)> {
        let mut g = self.inner.lock().unwrap();
        loop {
            if let Some(v) = *g {
                return Some(v);
            }
            if stop.load(Ordering::Relaxed) {
                return None;
            }
            let (ng, _timeout) = self
                .cv
                .wait_timeout(g, Duration::from_millis(100))
                .unwrap();
            g = ng;
        }
    }
}

fn probe_streams(path: &PathBuf) -> anyhow::Result<(VideoStreamInfo, Option<AudioStreamInfo>)> {
    let f = std::fs::File::open(path).with_context(|| format!("open {path:?}"))?;
    let mut r = std::io::BufReader::new(f);
    let asf = AsfFile::open(&mut r)?;

    let video = asf
        .video_streams
        .get(0)
        .cloned()
        .context("no video stream")?;

    let mut audio: Option<AudioStreamInfo> = None;
    for a in asf.audio_streams.iter() {
        if matches!(a.format_tag, 0x0160 | 0x0161) {
            audio = Some(a.clone());
            break;
        }
    }

    Ok((video, audio))
}

#[derive(Clone)]
struct VideoFrame {
    width: u32,
    height: u32,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

fn spawn_wmv2_decode_thread(
    path: PathBuf,
    tx: crossbeam_channel::Sender<VideoFrame>,
    stop: Arc<AtomicBool>,
    clock: Arc<AvStartClock>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let f = match std::fs::File::open(&path) {
            Ok(v) => v,
            Err(e) => {
                log::error!("open failed: {e:?}");
                return;
            }
        };
        let r = std::io::BufReader::new(f);
        let mut dec = match AsfWmv2Decoder::open(r) {
            Ok(v) => v,
            Err(e) => {
                log::error!("video decoder open failed: {e:?}");
                return;
            }
        };

        let mut t0: Option<Instant> = None;
        let mut pts0: u32 = 0;

        while !stop.load(Ordering::Relaxed) {
            let frame = match dec.next_frame() {
                Ok(Some(f)) => f,
                Ok(None) => break,
                Err(e) => {
                    log::error!("video decode error: {e:?}");
                    break;
                }
            };

            if t0.is_none() {
                t0 = Some(Instant::now());
                pts0 = frame.pts_ms;
                if let Some(t0v) = t0 {
                    clock.set_once(t0v, pts0);
                }
            }

            if let Some(t0) = t0 {
                let target = t0 + Duration::from_millis((frame.pts_ms - pts0) as u64);
                let now = Instant::now();
                if target > now {
                    std::thread::sleep(target - now);
                }
            }

            let vf = VideoFrame {
                width: frame.frame.width,
                height: frame.frame.height,
                y: frame.frame.y,
                u: frame.frame.cb,
                v: frame.frame.cr,
            };

            if tx.send(vf).is_err() {
                break;
            }
        }
    })
}

fn spawn_wma_decode_thread<P>(
    path: PathBuf,
    mut prod: P,
    out_channels: usize,
    out_rate: u32,
    stop: Arc<AtomicBool>,
    clock: Arc<AvStartClock>,
) -> std::thread::JoinHandle<()>
where
    P: Producer<Item = f32> + Send + 'static,
{
    std::thread::spawn(move || {
        let f = match std::fs::File::open(&path) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[audio] open failed: {e:?}");
                return;
            }
        };
        let r = std::io::BufReader::new(f);
        let mut dec = match AsfWmaDecoder::open(r) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[audio] decoder open failed: {e:?}");
                return;
            }
        };

        let in_rate = dec.sample_rate();
        let in_channels = dec.channels() as usize;
        eprintln!(
            "[audio] stream: {} Hz, {} ch (output {} Hz, {} ch)",
            in_rate,
            in_channels,
            out_rate,
            out_channels
        );

        let mut resampler = LinearResampler::new(in_rate, out_rate, out_channels);

        // Align audio timing to the video start clock.
        let Some((t0, pts0)) = clock.wait_ready(stop.as_ref()) else {
            return;
        };

        while !stop.load(Ordering::Relaxed) {
            let af = match dec.next_frame() {
                Ok(Some(f)) => f,
                Ok(None) => break,
                Err(e) => {
                    eprintln!("[audio] decode error: {e:?}");
                    break;
                }
            };

            // PTS-based pacing: prevents audio from running ahead ("fast forward")
            // if video startup is slower.
            let rel_ms = af.pts_ms.saturating_sub(pts0);
            let target = t0 + Duration::from_millis(rel_ms as u64);
            let now = Instant::now();
            if target > now {
                std::thread::sleep(target - now);
            }

            let samples_in = &af.frame.samples;
            let mut tmp: Vec<f32>;
            let samples_out_ch = if in_channels == out_channels {
                samples_in.as_slice()
            } else if in_channels == 1 && out_channels == 2 {
                tmp = Vec::with_capacity(samples_in.len() * 2);
                for &s in samples_in {
                    tmp.push(s);
                    tmp.push(s);
                }
                tmp.as_slice()
            } else if in_channels == 2 && out_channels == 1 {
                tmp = Vec::with_capacity(samples_in.len() / 2);
                let mut i = 0usize;
                while i + 1 < samples_in.len() {
                    tmp.push(0.5 * (samples_in[i] + samples_in[i + 1]));
                    i += 2;
                }
                tmp.as_slice()
            } else {
                // Unsupported channel layout.
                continue;
            };

            let out = resampler.process(samples_out_ch);
            for s in out {
                let _ = prod.try_push(s);
            }
        }
    })
}

struct LinearResampler {
    in_rate: u32,
    out_rate: u32,
    channels: usize,
    pos: f64,
    prev: Vec<f32>,
}

impl LinearResampler {
    fn new(in_rate: u32, out_rate: u32, channels: usize) -> Self {
        Self {
            in_rate,
            out_rate,
            channels,
            pos: 0.0,
            prev: vec![0.0; channels],
        }
    }

    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.in_rate == self.out_rate {
            // Keep continuity for next block.
            if input.len() >= self.channels {
                let tail = &input[input.len() - self.channels..];
                self.prev.copy_from_slice(tail);
            }
            return input.to_vec();
        }

        let ch = self.channels;
        let in_frames = input.len() / ch;
        if in_frames == 0 {
            return Vec::new();
        }

        // Buffer = [prev_frame] + [current input frames]
        let mut buf = Vec::with_capacity((in_frames + 1) * ch);
        buf.extend_from_slice(&self.prev);
        buf.extend_from_slice(&input[..in_frames * ch]);
        let buf_frames = in_frames + 1;

        // Advance by source frames per output frame.
        let step = (self.in_rate as f64) / (self.out_rate as f64);
        let mut out = Vec::with_capacity(((in_frames as u64 * self.out_rate as u64) / self.in_rate as u64) as usize * ch + ch);

        while self.pos < (buf_frames - 1) as f64 {
            let i0 = self.pos.floor() as usize;
            let frac = self.pos - (i0 as f64);
            let i1 = i0 + 1;
            if i1 >= buf_frames {
                break;
            }
            let base0 = i0 * ch;
            let base1 = i1 * ch;
            for c in 0..ch {
                let a = buf[base0 + c];
                let b = buf[base1 + c];
                out.push(a + (b - a) * (frac as f32));
            }
            self.pos += step;
        }

        // Carry fractional position into the next block: subtract the number of *new* frames consumed.
        self.pos -= in_frames as f64;
        if self.pos < 0.0 {
            self.pos = 0.0;
        }

        // Update prev frame.
        let tail = &input[(in_frames - 1) * ch..in_frames * ch];
        self.prev.copy_from_slice(tail);

        out
    }
}
struct AudioOutput {
    _stream: cpal::Stream,
}

impl AudioOutput {
    fn new_best_effort(
        sample_rate: u32,
        channels: usize,
    ) -> anyhow::Result<(Self, ringbuf::HeapProd<f32>, usize, u32)> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();
        let dev = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("no default output device"))?;

        let cfgs: Vec<cpal::SupportedStreamConfigRange> = dev.supported_output_configs()?.collect();

        let mut try_channels: Vec<usize> = vec![channels];
        if channels == 1 {
            try_channels.push(2);
        }

        let mut chosen: Option<(cpal::SupportedStreamConfig, usize, u32)> = None;
        for &ch in &try_channels {
            for r in &cfgs {
                if r.channels() as usize != ch {
                    continue;
                }
                let sr = sample_rate.clamp(r.min_sample_rate().0, r.max_sample_rate().0);
                chosen = Some((r.with_sample_rate(cpal::SampleRate(sr)), ch, sr));
                if sr == sample_rate {
                    break;
                }
            }
            if chosen.is_some() {
                break;
            }
        }

        let (cfg, out_channels, out_rate) = if let Some((cfg, ch, sr)) = chosen {
            (cfg, ch, sr)
        } else {
            let cfg = dev.default_output_config()?;
            let ch = cfg.channels().clone() as usize;
            let rate = cfg.sample_rate().0.clone();
            (cfg, ch, rate)
        };

        eprintln!(
            "[audio] output: {} Hz, {} ch, format {:?}",
            out_rate,
            out_channels,
            cfg.sample_format()
        );

        let cap = (out_rate as usize)
            .saturating_mul(out_channels)
            .saturating_mul(2)
            .max(4096);
        let rb = ringbuf::HeapRb::<f32>::new(cap);
        let (prod, mut cons) = rb.split();

        let stream_cfg: cpal::StreamConfig = cfg.clone().into();
        let err_fn = |e| eprintln!("[audio] stream error: {e:?}");

        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => dev.build_output_stream(
                &stream_cfg,
                move |data: &mut [f32], _| {
                    for s in data {
                        *s = cons.try_pop().unwrap_or(0.0f32);
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => dev.build_output_stream(
                &stream_cfg,
                move |data: &mut [i16], _| {
                    for s in data {
                        let v = cons.try_pop().unwrap_or(0.0);
                        let v = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        *s = v;
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::U16 => dev.build_output_stream(
                &stream_cfg,
                move |data: &mut [u16], _| {
                    for s in data {
                        let v = cons.try_pop().unwrap_or(0.0);
                        let v = ((v * 32767.0) + 32768.0).clamp(0.0, 65535.0) as u16;
                        *s = v;
                    }
                },
                err_fn,
                None,
            )?,
            f => {
                return Err(anyhow::anyhow!("unsupported sample format: {f:?}"));
            }
        };

        stream.play()?;
        Ok((Self { _stream: stream }, prod, out_channels, out_rate))
    }
}

struct Renderer {
    // IMPORTANT: surface must drop before window.
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,

    tex_y: wgpu::Texture,
    tex_u: wgpu::Texture,
    tex_v: wgpu::Texture,
    bind_group: wgpu::BindGroup,

    // Staging buffers for row-padding (wgpu requires bytes_per_row alignment).
    scratch_y: Vec<u8>,
    scratch_u: Vec<u8>,
    scratch_v: Vec<u8>,

    video_w: u32,
    video_h: u32,
    window: Window,
}

impl Renderer {
    async fn new(window: Window, video_w: u32, video_h: u32) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface_tmp = instance.create_surface(&window)?;
        let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface_tmp) };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("no suitable GPU adapter"))?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("yuv_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let (tex_y, view_y) = Self::make_plane_tex(&device, video_w, video_h, "Y");
        let (tex_u, view_u) = Self::make_plane_tex(&device, video_w / 2, video_h / 2, "U");
        let (tex_v, view_v) = Self::make_plane_tex(&device, video_w / 2, video_h / 2, "V");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("yuv_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuv_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("yuv_bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view_y),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view_u),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&view_v),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("yuv_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("yuv_pipe"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            sampler,
            tex_y,
            tex_u,
            tex_v,
            bind_group,

            scratch_y: Vec::new(),
            scratch_u: Vec::new(),
            scratch_v: Vec::new(),
            video_w,
            video_h,
            window,
        })
    }

    fn request_redraw(&self) {
        self.window.request_redraw();
    }

    fn resize(&mut self, w: u32, h: u32) {
        let w = w.max(1);
        let h = h.max(1);
        if self.config.width == w && self.config.height == h {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }

    fn upload_frame_mut(&mut self, vf: &VideoFrame) {
        if vf.width != self.video_w || vf.height != self.video_h {
            return;
        }
        let w = self.video_w;
        let h = self.video_h;

        Self::upload_plane_static(
            &self.queue,
            &self.tex_y,
            w, h,
            &vf.y,
            &mut self.scratch_y,
        );
        Self::upload_plane_static(
            &self.queue,
            &self.tex_u,
            w / 2, h / 2,
            &vf.u,
            &mut self.scratch_u,
        );
        Self::upload_plane_static(
            &self.queue,
            &self.tex_v,
            w / 2, h / 2,
            &vf.v,
            &mut self.scratch_v,
        );
    }

    fn upload_plane_static(
        queue: &wgpu::Queue,
        tex: &wgpu::Texture,
        w: u32,
        h: u32,
        data: &[u8],
        scratch: &mut Vec<u8>,
    ) {
        if w == 0 || h == 0 {
            return;
        }

        const ALIGN: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let stride = ((w + (ALIGN - 1)) / ALIGN) * ALIGN;

        let (src, bpr) = if stride == w {
            (data, w)
        } else {
            let needed = (stride as usize) * (h as usize);
            if scratch.len() < needed {
                scratch.resize(needed, 0);
            }
            for row in 0..(h as usize) {
                let dst0 = row * (stride as usize);
                let src0 = row * (w as usize);
                scratch[dst0..dst0 + (w as usize)].copy_from_slice(&data[src0..src0 + (w as usize)]);
            }
            (&scratch[..needed], stride)
        };

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            src,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    fn render(&mut self) -> anyhow::Result<()> {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(e) => return Err(anyhow::anyhow!("surface error: {e:?}")),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
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
            rpass.draw(0..4, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    fn make_plane_tex(
        device: &wgpu::Device,
        w: u32,
        h: u32,
        label: &str,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("tex_{label}")),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }
}

const SHADER: &str = r#"
struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VSOut {
    var positions = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var uvs = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
    );

    var out: VSOut;
    out.pos = vec4<f32>(positions[idx], 0.0, 1.0);
    out.uv = uvs[idx];
    return out;
}

@group(0) @binding(0) var tex_y: texture_2d<f32>;
@group(0) @binding(1) var tex_u: texture_2d<f32>;
@group(0) @binding(2) var tex_v: texture_2d<f32>;
@group(0) @binding(3) var samp: sampler;

fn yuv_to_rgb(y: f32, u: f32, v: f32) -> vec3<f32> {
    let uu = u - 0.5;
    let vv = v - 0.5;
    let r = y + 1.402 * vv;
    let g = y - 0.344136 * uu - 0.714136 * vv;
    let b = y + 1.772 * uu;
    return vec3<f32>(r, g, b);
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let y = textureSample(tex_y, samp, in.uv).r;
    let u = textureSample(tex_u, samp, in.uv).r;
    let v = textureSample(tex_v, samp, in.uv).r;
    let rgb = clamp(yuv_to_rgb(y, u, v), vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(rgb, 1.0);
}
"#;

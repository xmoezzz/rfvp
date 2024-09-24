use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use glam::Mat4;
use rfvp_audio::AudioManager;
use rfvp_render::{
    BindGroupLayouts, Camera, GpuCommonResources, Pillarbox, Pipelines, RenderTarget, Renderable,
};
use tracing::{debug, info, warn};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use winit::{
    dpi::{LogicalPosition, LogicalSize, PhysicalSize},
    event::*,
    event_loop::{ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen, Window, WindowBuilder},
};

use crate::{
    adv::{self, assets::AdvAssets, Adv},
    asset::{locate_assets, AnyAssetIo, AnyAssetServer, AssetServer},
    cli::Cli,
    fps_counter::FpsCounter,
    input::RawInputState,
    render::overlay::{OverlayManager, OverlayVisitable},
    time::Time,
    update::{Updatable, UpdateContext},
};

struct State<'window> {
    surface: wgpu::Surface<'window>,
    surface_config: wgpu::SurfaceConfiguration,
    window_size: (u32, u32),
    resources: Arc<GpuCommonResources>,
    camera: Camera,
    time: Time,
    render_target: RenderTarget,
    pillarbox: Pillarbox,
    asset_server: Arc<AnyAssetServer>,
    input: RawInputState,
    overlay_manager: OverlayManager,
    fps_counter: FpsCounter,
    adv: Adv,
}

impl<'state> State<'state> {
    async fn new(
        window: &'state Window,
        adv_assets: AdvAssets,
        asset_server: Arc<AssetServer<AnyAssetIo>>,
        _cli: &Cli,
    ) -> Result<Self> {
        let window_size = window.inner_size();
        let window_size = (window_size.width, window_size.height);

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let backends = wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::all());
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let surface = instance
            .create_surface(window)
            .context("Creating surface")?;
        let adapter = wgpu::util::initialize_adapter_from_env_or_default(
            &instance,
            // NOTE: this select the low-power GPU by default
            // it's fine, but if we want to use the high-perf one in the future we will have to ditch this function
            Some(&surface),
        )
        .await
        .context("Failed to find appropriate wgpu adapter")?;

        info!("Selected an adapter {:?}", adapter.get_info(),);
        debug!("Adapter limits: {:?}", adapter.limits());

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::PUSH_CONSTANTS,
                    // WebGL doesn't support all of wgpu's features, so if
                    // we're building for the web we'll have to disable some.
                    required_limits: wgpu::Limits {
                        max_texture_dimension_2d: 4096,
                        max_push_constant_size: 128,

                        ..wgpu::Limits::downlevel_webgl2_defaults()
                    },
                },
                // Some(&std::path::Path::new("trace")), // Trace path
                None,
            )
            .await
            .context("Failed to create wgpu device")?;

        // TODO: make a better selection?
        // TODO: rn we don't really support switching this
        // it may be worth to add one more pass to convert from internal (Rgba8) to the preferred output format
        // or support having everything in the preferred format? (sounds hard)
        let surface_texture_format = surface.get_capabilities(&adapter).formats[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_texture_format,
            width: window_size.0,
            height: window_size.1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let bind_group_layouts = BindGroupLayouts::new(&device);
        let pipelines = Pipelines::new(&device, &bind_group_layouts, surface_texture_format);

        let camera = Camera::new(window_size);

        let resources = Arc::new(GpuCommonResources {
            device,
            queue,
            render_buffer_size: RwLock::new(camera.render_buffer_size()),
            bind_group_layouts,
            pipelines,
        });

        let overlay = OverlayManager::new(&resources, surface_texture_format);

        let render_target = RenderTarget::new(
            &resources,
            camera.render_buffer_size(),
            Some("Window RenderTarget"),
        );

        let pillarbox = Pillarbox::new(&resources);

        let audio_manager = Arc::new(AudioManager::new());

        let mut adv = Adv::new(&resources, audio_manager, adv_assets, 0, 42);

        Ok(Self {
            surface,
            surface_config: config,
            window_size,
            resources,
            camera,
            time: Time::default(),
            render_target,
            pillarbox,
            asset_server,
            input: RawInputState::new(),
            overlay_manager: overlay,
            fps_counter: FpsCounter::new(),
            adv,
        })
    }

    fn reconfigure_surface(&mut self) {
        self.surface
            .configure(&self.resources.device, &self.surface_config);
    }

    pub fn resize(&mut self, new_size: (u32, u32)) {
        if new_size.0 > 0 && new_size.1 > 0 {
            self.window_size = new_size;
            self.surface_config.width = new_size.0;
            self.surface_config.height = new_size.1;
            self.surface
                .configure(&self.resources.device, &self.surface_config);

            self.camera.resize(new_size);
            self.render_target
                .resize(&self.resources, self.camera.render_buffer_size());

            debug!(
                "Window resized to {:?}, new render buffer size is {:?}",
                new_size,
                self.camera.render_buffer_size()
            );

            *self.resources.render_buffer_size.write().unwrap() = self.camera.render_buffer_size();

            self.pillarbox.resize(&self.resources);
            self.adv.resize(&self.resources);
        }
    }

    #[allow(unused_variables)]
    fn input(&mut self, event: &WindowEvent) -> bool {
        self.input.on_winit_event(event);
        false
    }

    fn update(&mut self) {
        self.time.update();

        let mut input = self.input.clone();

        self.overlay_manager
            .start_update(&self.time, &input, self.window_size);
        self.overlay_manager.visit_overlays(|collector| {
            self.fps_counter.visit_overlay(collector);
            input.visit_overlay(collector);
            self.adv.visit_overlay(collector);
        });
        self.overlay_manager
            .finish_update(&self.resources, &mut input);

        let update_context = UpdateContext {
            time: &self.time,
            gpu_resources: &self.resources,
            asset_server: &self.asset_server,
            raw_input_state: &input,
        };

        self.adv.update(&update_context);
        self.fps_counter.update(&update_context);

        // NOTE: it's important that the input is updated after everything else, as it clears some state after it should have been handled
        self.input.update();
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // render everything to the render target
        {
            let mut encoder = self.resources.start_encoder();
            let mut render_pass = self
                .render_target
                .begin_srgb_render_pass(&mut encoder, Some("Screen RenderPass"));

            self.adv.render(
                &self.resources,
                &mut render_pass,
                Mat4::IDENTITY,
                self.render_target.projection_matrix(),
            );
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        {
            let mut encoder = self.resources.start_encoder();
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Final RenderPass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.resources.pipelines.sprite_screen.draw(
                &mut render_pass,
                self.render_target.vertex_source(),
                self.render_target.bind_group(),
                self.camera.screen_projection_matrix(),
            );
            self.pillarbox.render(
                &self.resources,
                &mut render_pass,
                Mat4::IDENTITY,
                self.camera.screen_projection_matrix(),
            );

            self.overlay_manager
                .render(&self.resources, &mut render_pass);
        }

        output.present();

        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub async fn run(cli: Cli) {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init_with_level(log::Level::Warn).expect("Couldn't initialize logger");
        } else {
            tracing_subscriber::fmt::init();
        }
    }

    rfvp_tasks::create_task_pools();

    let asset_io = locate_assets(cli.assets_dir.as_deref())
        .context("Failed to locate assets. Consult the README for instructions on how to set up the game.")
        .unwrap();

    debug!("Asset IO: {:#?}", asset_io);

    let asset_server = Arc::new(AnyAssetServer::new(asset_io.into()));

    let adv_assets =
        pollster::block_on(AdvAssets::load(&asset_server)).expect("Loading assets failed");
    
    let (width, height) = adv_assets.scenario.get_screen_size();

    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(width, height))
        .with_maximized(false)
        .with_position(LogicalPosition::new(width, 0))
        .with_title(adv_assets.scenario.get_title())
        .build(&event_loop)
        .unwrap();

    #[cfg(target_arch = "wasm32")]
    {
        // Winit prevents sizing with CSS, so we have to set
        // the size manually when on web.
        use winit::dpi::PhysicalSize;
        window.set_inner_size(PhysicalSize::new(450, 400));

        use winit::platform::web::WindowExtWebSys;
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let dst = doc.get_element_by_id("wasm-example")?;
                let canvas = web_sys::Element::from(window.canvas());
                dst.append_child(&canvas).ok()?;
                Some(())
            })
            .expect("Couldn't append canvas to document body.");
    }

    // State::new uses async code, so we're going to wait for it to finish
    let mut state = State::new(&window, adv_assets, asset_server, &cli)
        .await
        .expect("Failed to initialize the game"); // TODO: report error in a better way

    // don't move it pls
    let window = &window;

    event_loop
        .run(move |event, target| {
            match event {
                Event::WindowEvent {
                    ref event,
                    window_id,
                } if window_id == window.id() => {
                    if !state.input(event) {
                        // UPDATED!
                        match event {
                            WindowEvent::CloseRequested
                            | WindowEvent::KeyboardInput {
                                event:
                                    KeyEvent {
                                        state: ElementState::Pressed,
                                        physical_key:
                                            PhysicalKey::Code(KeyCode::Escape | KeyCode::KeyQ),
                                        ..
                                    },
                                ..
                            } => target.exit(),
                            WindowEvent::KeyboardInput {
                                event:
                                    KeyEvent {
                                        state: ElementState::Pressed,
                                        physical_key: PhysicalKey::Code(KeyCode::F11),
                                        ..
                                    },
                                ..
                            } => {
                                window.set_fullscreen(
                                    window.fullscreen().map_or_else(
                                        || Some(Fullscreen::Borderless(None)),
                                        |_| None,
                                    ),
                                );
                            }
                            WindowEvent::KeyboardInput {
                                event:
                                    KeyEvent {
                                        state: ElementState::Pressed,
                                        physical_key: PhysicalKey::Code(KeyCode::F10),
                                        ..
                                    },
                                ..
                            } => {
                                if let Some(new_size) =
                                    window.request_inner_size(PhysicalSize::new(1920, 1080))
                                {
                                    state.resize(new_size.into());
                                }
                            }
                            WindowEvent::Resized(physical_size) => {
                                state.resize((*physical_size).into());
                            }
                            WindowEvent::RedrawRequested => {
                                state.update();
                                match state.render() {
                                    Ok(_) => {}
                                    // Reconfigure the surface if it's lost or outdated
                                    Err(
                                        wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated,
                                    ) => {
                                        state.reconfigure_surface();
                                    }
                                    // The system is out of memory, we should probably quit
                                    Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),

                                    Err(wgpu::SurfaceError::Timeout) => warn!("Surface timeout"),
                                }

                                window.request_redraw();
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        })
        .unwrap();
}

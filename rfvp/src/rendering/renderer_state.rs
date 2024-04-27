use rfvp_render::{BindGroupLayouts, Camera, GpuCommonResources, Pillarbox, Pipelines, RenderTarget};
use std::sync::{Arc, RwLock};
use wgpu::{CompositeAlphaMode, InstanceDescriptor, SurfaceConfiguration, TextureFormat};
use winit::{event::WindowEvent, window::Window};

use crate::subsystem::world::GameData;
use crate::config::app_config::AppConfig;

use super::overlay::OverlayManager;

pub(crate) struct RendererState {
    surface: wgpu::Surface<'static>,
    // device: wgpu::Device,
    // queue: wgpu::Queue,
    config: SurfaceConfiguration,
    render_target: RenderTarget,
    pillarbox: Pillarbox,
    overlay_manager: OverlayManager,
}

impl RendererState {
    pub(crate) async fn new(window: Arc<Window>) -> Self {
        let _size = window.inner_size();

        let backend = wgpu::util::backend_bits_from_env().unwrap_or_else(wgpu::Backends::all);
        let instance = wgpu::Instance::new(InstanceDescriptor {
            backends: backend,
            dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
            flags: wgpu::InstanceFlags::default(),
            gles_minor_version: wgpu::Gles3MinorVersion::Automatic,
        });

        let (size, surface) = {
            let size = window.inner_size();
            let surface = instance
                .create_surface(window.clone())
                .expect("Surface unsupported by adapter");
            (size, surface)
        };

        let adapter = wgpu::util::initialize_adapter_from_env_or_default(&instance, Some(&surface))
            .await
            .expect("No suitable GPU adapters found on the system!");

        let needed_limits =
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        let trace_dir = std::env::var("WGPU_TRACE");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: needed_limits,
                },
                trace_dir.ok().as_ref().map(std::path::Path::new),
            )
            .await
            .expect("Unable to find a suitable GPU adapter!");

        let w = window.inner_size();

        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_capabilities(&adapter).formats[0],
            width: w.width * window.scale_factor() as u32,
            height: w.height * window.scale_factor() as u32,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![TextureFormat::Bgra8UnormSrgb],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let surface_texture_format = surface.get_capabilities(&adapter).formats[0];
        let bind_group_layouts = BindGroupLayouts::new(&device);
        let pipelines = Pipelines::new(
            &device,
            &bind_group_layouts,
            surface_texture_format,
        );

        let window_size = (w.width, w.height);
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

        let pillarbox = Pillarbox::new(&resources, window_size.0, window_size.1);

        Self {
            surface,
            config,
            render_target,
            pillarbox,
            overlay_manager: overlay,
        }
    }

    pub(crate) fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>, scale_factor: f64) {
        self.config.width = new_size.width * scale_factor as u32;
        self.config.height = new_size.height * scale_factor as u32;
        self.surface.configure(&self.device, &self.config);
    }

    pub(crate) fn _input(&mut self, _event: &WindowEvent) -> bool {
        false
    }

    pub(crate) fn update(&mut self, data: &mut GameData) {
        self.renderer
            .update(data, &self.device, &self.config, &mut self.queue);
    }

    pub(crate) fn render(
        &mut self,
        data: &mut GameData,
        config: &AppConfig,
    ) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        self.renderer.render(data, config, &view, &mut encoder);

        self.queue.submit(Some(encoder.finish()));

        frame.present();
        Ok(())
    }
}

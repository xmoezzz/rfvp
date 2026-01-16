use anyhow::Result;
use std::{
    collections::HashMap, fs::File, path::{Path, PathBuf}, slice::Windows, sync::{Arc, RwLock}, time::Instant
};
use glam::{mat4, vec3, vec4, Mat4};
use wgpu::util::DeviceExt;
use regex::Regex;
use crate::{
    script::{
        global::GLOBAL,
        parser::{Nls, Parser}, Variant,
    },
    subsystem::resources::thread_manager::ThreadManager,
    utils::ani::{self, icondir_to_custom_cursor, CursorBundle},
};

use winit::{dpi::{PhysicalSize, Size}, window::CustomCursor};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

use crate::rendering::render_tree::RenderTree;
use crate::vm_worker::VmWorker;

use crate::subsystem::scene::{Scene, SceneAction, SceneMachine};
use crate::subsystem::scheduler::Scheduler;
use crate::subsystem::world::GameData;
use crate::{config::app_config::AppConfig, subsystem::event_handler::update_input_events};
use crate::rfvp_render::{BindGroupLayouts, GpuCommonResources, Pipelines, RenderTarget};
use crate::rfvp_render::vertices::{PosVertex, VertexSource};


use crate::rendering::gpu_prim::GpuPrimRenderer;
use crate::subsystem::resources::motion_manager::DissolveType;

pub struct App {
    config: AppConfig,
    game_data: Arc<RwLock<GameData>>,
    title: String,
    vm_worker: VmWorker,
    pending_vm_frame_ms: u64,
    pending_vm_frame_ms_valid: bool,
    scheduler: Scheduler,
    layer_machine: SceneMachine,
    window: Option<Arc<Window>>,
    render_target: RenderTarget,
    resources: Arc<GpuCommonResources>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    prim_renderer: GpuPrimRenderer,
    virtual_size: (u32, u32),
    render_tree: RenderTree,
    dissolve_vertex_buffer: wgpu::Buffer,
    dissolve_index_buffer: wgpu::Buffer,
    dissolve_num_indices: u32,

    // Tracks dissolve completion on the main thread so we can wake contexts
    // waiting on DISSOLVE_WAIT immediately via an EngineEvent.
    last_dissolve_type: DissolveType,
}

impl App {
    #[allow(dead_code)]
    pub fn app() -> AppBuilder {
        let app_config = AppConfig::default();
        App::app_with_config(app_config)
    }

    pub fn app_with_config(app_config: AppConfig) -> AppBuilder {
        crate::utils::logger::Logger::init_logging(app_config.logger_config.clone());
        log::info!(
            "Starting the app, with the following configuration \n {:?}",
            app_config
        );
        AppBuilder::new(app_config)
    }

    fn setup(&mut self) {
        self.initialize_internal_resources();
        {
            let mut gd = self.game_data.write().unwrap();
            self.layer_machine.apply_scene_action(SceneAction::Start, &mut gd);
            if gd.has_cursor(1) {
                gd.switch_cursor(1);
            }
        }
}

    fn initialize_internal_resources(&mut self) {
        let mut gd = self.game_data.write().unwrap();

        let window = self.window.as_ref().expect("No window found during setup");
        gd
            .set_window(crate::subsystem::resources::window::Window::new(
                (window.inner_size().width, window.inner_size().height),
                window.scale_factor(),
            ));
    }
    
    fn window(&self) -> &Arc<Window> {
        self.window.as_ref().expect("No window found")
    }

    fn debug_title(&mut self, x: i32, y: i32) {
        let title = self.title.clone();
        let (x, y, down, up) = {
            let gd = self.game_data.read().unwrap();
            (
                gd.inputs_manager.get_cursor_x(),
                gd.inputs_manager.get_cursor_y(),
                self.debug_keydown(),
                self.debug_keyup(),
            )
        };
        if let Ok(test) = std::env::var("DEBUG") {
            if test == *"1" {
                let title = format!("{} | {},{} | down {}, up {} | ", title, x, y, down, up);
                self.window.as_mut().unwrap().set_title(&title);
            }
        }
}

    fn debug_keydown(&self) -> String {
        let gd = self.game_data.read().unwrap();
        gd.inputs_manager.get_input_down().to_string()
}

    fn debug_keyup(&self) -> String {
        let gd = self.game_data.read().unwrap();
        gd.inputs_manager.get_input_up().to_string()
}

    fn run(mut self, event_loop: EventLoop<()>) {
        let _result = event_loop.run(move |event, loopd| {
            loopd.set_control_flow(ControlFlow::Wait);

            match event {
                Event::WindowEvent {
                    ref event,
                    window_id,
                } if window_id == self.window.as_mut().unwrap().id() => {
                    let is_input_event = matches!(
                        event,
                        WindowEvent::KeyboardInput { .. }
                            | WindowEvent::MouseInput { .. }
                            | WindowEvent::MouseWheel { .. }
                            | WindowEvent::CursorMoved { .. }
                            | WindowEvent::CursorEntered { .. }
                            | WindowEvent::CursorLeft { .. }
                    );
                    match event {
                        WindowEvent::CloseRequested => loopd.exit(),
                        WindowEvent::Resized(physical_size) => {
                            self.game_data.write().unwrap().window_mut().set_dimensions(physical_size.width, physical_size.height);

                            // Update swapchain configuration.
                            self.surface_config.width = physical_size.width.max(1);
                            self.surface_config.height = physical_size.height.max(1);
                            self.surface.configure(&self.resources.device, &self.surface_config);
                        }
                        WindowEvent::ScaleFactorChanged {  .. } => {
                            // self.renderer.as_mut().unwrap().resize(
                            //     self.window.as_ref().expect("Missing window").inner_size(),
                            //     *scale_factor,
                            // );
                        }
                        WindowEvent::RedrawRequested => {
                            // Drive the simulation from redraws so we do not busy-spin.
                            self.next_frame();
                            self.layer_machine
                                .apply_scene_action(SceneAction::EndFrame, &mut self.game_data.write().unwrap());
                            if let Err(e) = self.render_frame() {
                                log::error!("render_frame: {e:?}");
                            }
                        }
                        _ => {}
                    }
                    {
                        let mut gd = self.game_data.write().unwrap();
                        update_input_events(event, &mut gd);
                    }
                    // Wake the VM immediately on user input so scripts that poll
                    // InputGetEvent/InputGetDown respond without waiting for the next frame.
                    if is_input_event {
                        self.vm_worker.send_input_signal();
                    }
                }
                Event::AboutToWait => {
                    // Allow the VM to run between frames. We only advance the VM once per
                    // simulation frame (set in `next_frame`) to avoid the event loop calling
                    // AboutToWait multiple times and over-advancing scripts.
                    if self.pending_vm_frame_ms_valid {
                        self.vm_worker.send_frame_ms(self.pending_vm_frame_ms);
                        self.pending_vm_frame_ms_valid = false;
                    }

                    // Schedule the next redraw. This keeps the event loop responsive while
                    // avoiding a hard-coded FPS cap.
                    self.window.as_mut().unwrap().request_redraw();
                }
                _ => (),
            }
        });
    }

    fn next_frame(&mut self) {
        let frame_duration = {
            let mut gd = self.game_data.write().unwrap();
            gd.time_mut_ref().frame()
        };
        self.pending_vm_frame_ms = frame_duration.as_millis() as u64;
        self.pending_vm_frame_ms_valid = true;
        let mut notify_dissolve_done = false;
        {
            let gd = self.game_data.write();
            let mut gd = gd.unwrap();
            let prev_dissolve = self.last_dissolve_type;

            // Movie update must run even when the VM/scheduler is halted for modal playback.
            {
                let motion_manager = &mut gd.motion_manager;
                if let Err(e) = self.game_data.write().unwrap().video_manager.tick(motion_manager) {
                    log::error!("VideoPlayerManager::tick failed: {:?}", e);
                    let motion_manager = &mut gd.motion_manager;
                    self.game_data.write().unwrap().video_manager.stop(motion_manager);
                    gd.set_halt(false);
                }
            }

            let modal_movie = gd.video_manager.is_modal_active();

            if !modal_movie {
                self.layer_machine.apply_scene_action(SceneAction::Update, &mut gd);
                self.scheduler.execute(&mut gd);
                self.layer_machine.apply_scene_action(SceneAction::LateUpdate, &mut gd);
            }

            // If a dissolve finished on this frame, wake contexts waiting on DISSOLVE_WAIT
            // on the VM thread. We only emit the event on the transition to None/Static.
            let cur_dissolve = gd.motion_manager.get_dissolve_type();
            if (prev_dissolve != DissolveType::None && prev_dissolve != DissolveType::Static)
                && (cur_dissolve == DissolveType::None || cur_dissolve == DissolveType::Static)
            {
                notify_dissolve_done = true;
            }
            self.last_dissolve_type = cur_dissolve;

            if gd.get_halt() {
                // Preserve halt while a modal Movie is active.
                if !gd.video_manager.is_modal_active() {
                    gd.set_halt(false);
                }
            } else {
                gd.inputs_manager.refresh_input();
                gd.set_current_thread(0);
            }
            gd.inputs_manager.frame_reset();
        }

        if notify_dissolve_done {
            self.vm_worker.send_dissolve_done();
        }
        self.update_cursor();
}

    fn update_cursor(&mut self) {
        let cursor_frame = {
            let mut gd = self.game_data.write().unwrap();
            gd.update_cursor()
        };
        let w = self.window.as_mut().expect("A window is mandatory to run this game !");
        if let Some(frame) = cursor_frame {
            w.set_cursor(frame);
        }
        {
            let mut gd = self.game_data.write().unwrap();
            let mut window = gd.window_mut();
            window.reset_future_settings()
        }
}

    fn render_frame(&mut self) -> anyhow::Result<()> {
        let gd = self.game_data.read().unwrap();

        // Build primitive draw list and upload any modified GraphBuffs to the GPU.
        self.prim_renderer.rebuild(&self.resources, &gd.motion_manager);

        let mut encoder = self
            .resources
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rfvp frame encoder"),
            });

        // Pass 1: render primitives into the virtual render target.
        {
            let mut pass = self
                .render_target
                .begin_srgb_render_pass(&mut encoder, Some("rfvp virtual pass"));
            self.prim_renderer.draw_virtual(&mut pass, &self.resources.pipelines.sprite, self.render_target.projection_matrix());
            // Global dissolve overlay (rendered in virtual space).
            let dissolve_type = gd.motion_manager.get_dissolve_type();
            if dissolve_type != crate::subsystem::resources::motion_manager::DissolveType::None {
                // NOTE: Mask-based dissolves are currently rendered as a simple colored fade.
                let alpha = gd.motion_manager.get_dissolve_alpha();
                if alpha > 0.0 {
                    let cid = gd.motion_manager.get_dissolve_color_id() as u8;
                    let c = gd.motion_manager.color_manager.get_entry(cid);
                    let color = vec4(
                        c.get_r() as f32 / 255.0,
                        c.get_g() as f32 / 255.0,
                        c.get_b() as f32 / 255.0,
                        (c.get_a() as f32 / 255.0) * alpha,
                    );
                    let src = VertexSource::VertexIndexBuffer {
                        vertex_buffer: &self.dissolve_vertex_buffer,
                        index_buffer: &self.dissolve_index_buffer,
                        indices: 0..self.dissolve_num_indices,
                        instances: 0..1,
                    };
                    self.resources.pipelines.fill.draw(
                        &mut pass,
                        src,
                        self.render_target.projection_matrix(),
                        color,
                    );
                }
            }
        }

        // Pass 2: present to the swapchain with aspect-preserving scaling.
        let output = match self.surface.get_current_texture() {
            Ok(o) => o,
            Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                // Recreate swapchain.
                self.surface.configure(&self.resources.device, &self.surface_config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => {
                // Skip a frame.
                return Ok(());
            }
            Err(e) => return Err(anyhow::anyhow!(e)),
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rfvp present pass"),
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

            let win_w = self.surface_config.width.max(1) as f32;
            let win_h = self.surface_config.height.max(1) as f32;
            let virt_w = self.virtual_size.0.max(1) as f32;
            let virt_h = self.virtual_size.1.max(1) as f32;

            let s = (win_w / virt_w).min(win_h / virt_h);
            let screen_proj = mat4(
                vec4(2.0 / win_w, 0.0, 0.0, 0.0),
                vec4(0.0, -2.0 / win_h, 0.0, 0.0),
                vec4(0.0, 0.0, 1.0, 0.0),
                vec4(0.0, 0.0, 0.0, 1.0),
            );
            let transform = screen_proj * Mat4::from_scale(vec3(s, s, 1.0));

            self.resources.pipelines.sprite_screen.draw(
                &mut pass,
                self.render_target.vertex_source(),
                self.render_target.bind_group(),
                transform,
            );
        }

        self.resources.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    
}

    pub fn find_hcb(game_path: impl AsRef<Path>) -> Result<PathBuf> {
        let mut path = game_path.as_ref().to_path_buf();
        path.push("*.hcb");

        let matches: Vec<_> = glob::glob(&path.to_string_lossy())?.flatten().collect();

        if matches.is_empty() {
            anyhow::bail!("No hcb file found in the game directory: {}", game_path.as_ref().display());
        }

        Ok(matches[0].to_path_buf())
    }
}

pub struct AppBuilder {
    config: AppConfig,
    scheduler: Scheduler,
    scene: Option<Box<dyn Scene>>,
    world: GameData,
    title: String,
    size: (u32, u32),
    parser: Parser,
    script_engine: ThreadManager,
}

impl AppBuilder {
    fn new(config: AppConfig) -> Self {
        let builder = Self {
            config,
            scheduler: Default::default(),
            scene: Default::default(),
            world: Default::default(),
            title: Default::default(),
            size: Default::default(),
            parser: Default::default(),
            script_engine: Default::default(),
        };
        builder
    }

    /// Specify a system to add to the scheduler.
    pub fn with_system(mut self, system: fn(&mut GameData)) -> Self {
        self.scheduler.add_system(system);
        self
    }

    /// Add a normal game layer to the pile. Every layer added before in the pile will be called
    pub fn with_scene<T: Scene + Default + 'static>(mut self) -> Self {
        self.scene = Some(Box::<T>::default());
        self
    }

    pub fn with_vfs(mut self, nls: Nls) -> anyhow::Result<Self> {
        self.world.vfs = crate::subsystem::resources::vfs::Vfs::new(nls)?;
        Ok(self)
    }

    pub fn with_window_title(mut self, title: &str) -> Self {
        self.title = title.to_owned();
        self
    }

    pub fn with_window_size(mut self, size: (u32, u32)) -> Self {
        self.size = size;
        self
    }

    pub fn with_script_engine(mut self, script_engine: ThreadManager) -> Self {
        self.script_engine = script_engine;
        self
    }

    pub fn with_parser(mut self, parser: Parser) -> Self {
        self.parser = parser;
        self
    }

    async fn init_render(
        window: Arc<Window>,
        virtual_size: (u32, u32),
    ) -> (
        Arc<GpuCommonResources>,
        RenderTarget,
        wgpu::Surface<'static>,
        wgpu::SurfaceConfiguration,
    ) {
        let size = window.inner_size();
        let backends = wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::all());
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        // wgpu 0.19 ties `Surface` to the lifetime of the window reference used to create it.
        // The window is stored inside `App` and outlives the surface, so extending the lifetime
        // to `'static` is sound here.
        let surface = {
            let s = instance.create_surface(window.as_ref()).unwrap();
            unsafe { std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(s) }
        };
        let adapter = wgpu::util::initialize_adapter_from_env_or_default(
            &instance,
            // NOTE: this select the low-power GPU by default
            // it's fine, but if we want to use the high-perf one in the future we will have to ditch this function
            Some(&surface),
        )
        .await
        .unwrap();

        // Create the logical device and command queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::PUSH_CONSTANTS,
                    // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                    required_limits: wgpu::Limits {
                        max_push_constant_size: 256,
                        ..wgpu::Limits::downlevel_webgl2_defaults()
                            .using_resolution(adapter.limits())
                    },
                },
                Some(Path::new("wgpu_trace")),
            )
            .await
            .expect("Failed to create device");

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        let bind_group_layouts = BindGroupLayouts::new(&device);
        let pipelines = Pipelines::new(&device, &bind_group_layouts, swapchain_format);

        let resources = Arc::new(GpuCommonResources {
            device,
            queue,
            render_buffer_size: RwLock::new(virtual_size),
            bind_group_layouts,
            pipelines,
        });

        let render_target = RenderTarget::new(
            &resources,
            virtual_size,
            Some("Window RenderTarget"),
        );

        (resources, render_target, surface, config)
    }

    /// Builds, setups and runs the application, must be called at the end of the building process.
    pub fn run(mut self) {
        let event_loop = EventLoop::new().expect("Event loop could not be created");
        event_loop.set_control_flow(ControlFlow::Poll);

        let window_builder: WindowAttributes = self
            .config
            .window_config
            .clone()
            .expect("The window configuration has not been found")
            .into(&self.config);
        let window = event_loop.create_window(window_builder)
            .expect("An error occured while building the main game window");

        let window = Arc::new(window);

        self.add_late_internal_systems_to_schedule();

        // let renderer_state =
        //     futures::executor::block_on(RendererState::new(window.clone()));

        let (resources, render_target, surface, surface_config) =
            futures::executor::block_on(AppBuilder::init_render(window.clone(), self.size));

        let entry_point = self.parser.get_entry_point();
        let non_volatile_global_count = self.parser.get_non_volatile_global_count();
        let volatile_global_count = self.parser.get_volatile_global_count();
        GLOBAL
            .lock()
            .unwrap()
            .init_with(non_volatile_global_count, volatile_global_count);
        
        self.script_engine.start_main(entry_point);
        self.world.nls = self.parser.nls.clone();


        let mut cursor_table = HashMap::new();
        if let Ok(cursor_paths) = self.world.vfs.find_ani() {
            let re = Regex::new(r"^([a-zA-Z_]+)(\d+)$").unwrap();
            for path in &cursor_paths {
                // split cursor1.ani into `cursor` and `1`
                let filename = path
                    .file_stem()
                    .unwrap_or_default() 
                    .to_string_lossy();

                if let Some(caps) = re.captures(&filename) {
                    let prefix = caps[1].to_string();
                    let number = caps[2].to_string();
                    
                    if let Ok(index) = number.parse::<u32>() {
                        let file = File::open(path).unwrap();
                        if let Ok(cursor) = ani::Decoder::new(file).decode() {
                            let mut failed = false;
                            let mut sources = vec![];
                            for frame in &cursor.frames {
                                match icondir_to_custom_cursor(frame) {
                                    Ok(s) => {
                                        sources.push(s);
                                    }
                                    Err(e) => {
                                        log::error!("{:#?}", e);
                                        failed = true;
                                        break;
                                    }
                                }
                            }

                            if failed {
                                log::error!("Failed to load icon : {}", path.display());
                                continue;
                            }

                            let mut new_cursors = vec![];
                            for s in sources {
                                let c = event_loop.create_custom_cursor(s);
                                new_cursors.push(c);
                            }

                            let cb = CursorBundle {
                                animated_cursor: cursor,
                                frames: new_cursors,
                                current_frame: 0,
                                last_update: Instant::now(),
                            };

                            cursor_table.insert(index, cb);
                        }
                    }
                } else {
                    continue;
                }

            }
        }

        self.world.set_cursor_table(cursor_table);


        // Fullscreen quad used for dissolve overlays (virtual space, pixel coordinates).
        let (dissolve_vertex_buffer, dissolve_index_buffer, dissolve_num_indices) = {
            let w = self.size.0.max(1) as f32;
            let h = self.size.1.max(1) as f32;
            let vertices: [PosVertex; 4] = [
                PosVertex { position: vec3(0.0, 0.0, 0.0) },
                PosVertex { position: vec3(w, 0.0, 0.0) },
                PosVertex { position: vec3(w, h, 0.0) },
                PosVertex { position: vec3(0.0, h, 0.0) },
            ];
            let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

            let vb = resources.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rfvp dissolve quad VB"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = resources.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rfvp dissolve quad IB"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            (vb, ib, indices.len() as u32)
        };

        let game_data = Arc::new(RwLock::new(self.world));
        let vm_worker = VmWorker::spawn(game_data.clone(), self.parser, self.script_engine);

        let mut app = App {
            config: self.config,
            game_data,
            title: self.title,
            scheduler: self.scheduler,
            layer_machine: SceneMachine {
                current_scene: self.scene,
            },
            window: Some(window.clone()),
            vm_worker,
            pending_vm_frame_ms: 0,
            pending_vm_frame_ms_valid: false,
            render_target,
            resources: resources.clone(),
            surface,
            surface_config,
            prim_renderer: GpuPrimRenderer::new(resources.clone(), self.size),
            virtual_size: self.size,
            render_tree: RenderTree::new(),
            dissolve_vertex_buffer,
            dissolve_index_buffer,
            dissolve_num_indices,
            last_dissolve_type: DissolveType::None,
        };

        app.setup();
        app.run(event_loop);
    }

    fn add_late_internal_systems_to_schedule(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_find_hcb() {
        let filepath = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase"));

        let hcb_path = App::find_hcb(filepath).unwrap();
        println!("{:?}", hcb_path);
    }
}
use anyhow::Result;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use crate::{
    script::{
        context::{CONTEXT_STATUS_DISSOLVE_WAIT, CONTEXT_STATUS_RUNNING, CONTEXT_STATUS_WAIT},
        global::GLOBAL,
        parser::{Nls, Parser}, Variant,
    },
    subsystem::resources::{
        motion_manager::DissolveType, thread_manager::ThreadManager, thread_wrapper::ThreadRequest,
    },
};
use winit::dpi::{PhysicalSize, Size};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};



use crate::subsystem::scene::{Scene, SceneAction, SceneMachine};
use crate::subsystem::scheduler::Scheduler;
use crate::subsystem::world::GameData;
use crate::{config::app_config::AppConfig, subsystem::event_handler::update_input_events};
use rfvp_render::{
    BindGroupLayouts, Camera, GpuCommonResources, Pipelines, RenderTarget,
};

pub struct App {
    config: AppConfig,
    game_data: GameData,
    scheduler: Scheduler,
    layer_machine: SceneMachine,
    window: Option<Arc<Window>>,
    // renderer: Option<RendererState>,
    parser: Parser,
    thread_manager: ThreadManager,
    render_target: RenderTarget,
    resources: Arc<GpuCommonResources>,
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
        self.layer_machine
            .apply_scene_action(SceneAction::Start, &mut self.game_data);
    }

    fn initialize_internal_resources(&mut self) {
        let window = self.window.as_ref().expect("No window found during setup");
        self.game_data
            .set_window(crate::subsystem::resources::window::Window::new(
                (window.inner_size().width, window.inner_size().height),
                window.scale_factor(),
            ));
    }

    fn run(mut self, event_loop: EventLoop<()>) {
        let _result = event_loop.run(move |event, loopd| {
            loopd.set_control_flow(ControlFlow::Poll);

            match event {
                Event::WindowEvent {
                    ref event,
                    window_id,
                } if window_id == self.window.as_mut().unwrap().id() => {
                    match event {
                        WindowEvent::CloseRequested => loopd.exit(),
                        WindowEvent::Resized(physical_size) => {
                            self.game_data
                                .window()
                                .set_dimensions(physical_size.width, physical_size.height);
                            // self.renderer.as_mut().unwrap().resize(
                            //     *physical_size,
                            //     self.window.as_ref().expect("Missing window").scale_factor(),
                            // );
                        }
                        WindowEvent::ScaleFactorChanged {  .. } => {
                            // self.renderer.as_mut().unwrap().resize(
                            //     self.window.as_ref().expect("Missing window").inner_size(),
                            //     *scale_factor,
                            // );
                        }
                        WindowEvent::RedrawRequested => {
                            // self.renderer.as_mut().unwrap().update(&mut self.game_data);
                            // match self
                            //     .renderer
                            //     .as_mut()
                            //     .unwrap()
                            //     .render(&mut self.game_data, &self.config)
                            // {
                            //     Ok(_) => {}
                            //     Err(e) => log::error!("{:?}", e),
                            // }
                        }
                        _ => {}
                    }
                    update_input_events(event, &mut self.game_data);
                }
                Event::AboutToWait => {
                    self.next_frame();
                    self.layer_machine
                        .apply_scene_action(SceneAction::EndFrame, &mut self.game_data);
                    self.window.as_mut().unwrap().request_redraw();
                }
                _ => (),
            }
        });
    }

    fn exec_script_bytecode(&mut self, id: u32, frame_time: u64) {
        // let ctx = self.game_data.thread_manager.get_thread(id);
        let dissolve_type = self.game_data.motion_manager.get_dissolve_type();
        let status = self.thread_manager.get_thread(id).get_status();
        if status & CONTEXT_STATUS_WAIT != 0 {
            let wait_time = self.thread_manager.get_thread(id).get_waiting_time();
            if wait_time > frame_time {
                self.thread_manager
                    .get_thread(id)
                    .set_waiting_time(wait_time - frame_time);
            } else {
                self.thread_manager.get_thread(id).set_waiting_time(0);
                self.thread_manager
                    .get_thread(id)
                    .set_status(status & 0xFFFFFFFD);
            }
        }

        // dissolve wait
        if status & CONTEXT_STATUS_DISSOLVE_WAIT != 0
            && (dissolve_type == DissolveType::None || dissolve_type == DissolveType::Static)
        {
            self.thread_manager
                .get_thread(id)
                .set_status(status & 0xFFFFFFEF);
        }

        if status & CONTEXT_STATUS_RUNNING != 0 {
            self.thread_manager.get_thread(id).set_should_break(false);
            while !self.thread_manager.get_thread(id).should_break() {
                // let mut ctx = self.thread_manager.get_thread(id);
                log::info!("tid: {}", id);
                let result = self
                    .thread_manager
                    .get_thread(id)
                    .dispatch_opcode(&mut self.game_data, &mut self.parser);
                if let Err(e) = result {
                    log::error!("Error while executing the script {:?}", e);
                    std::process::exit(1);
                }
                let thread_event = self.game_data.thread_wrapper.peek();
                if let Some(event) = thread_event {
                    match event {
                        ThreadRequest::Start(id, addr) => {
                            self.thread_manager.thread_start(id, addr);
                        }
                        ThreadRequest::Wait(time) => {
                            self.thread_manager.thread_wait(time);
                        }
                        ThreadRequest::Sleep(time) => {
                            self.thread_manager.thread_sleep(time);
                        }
                        ThreadRequest::Raise(time) => {
                            self.thread_manager.thread_raise(time);
                        }
                        ThreadRequest::Next() => {
                            self.thread_manager.thread_next();
                        }
                        ThreadRequest::Exit(id) => {
                            self.thread_manager.thread_exit(id);
                        }
                        ThreadRequest::ShouldBreak() => {
                            self.thread_manager.set_should_break(true);
                        }
                    }
                }
            }
        }
    }

    fn next_frame(&mut self) {
        let frame_duration = self.game_data.time().frame();
        // self.game_data.time().add_delta_duration(frame_duration);

        self.layer_machine
            .apply_scene_action(SceneAction::Update, &mut self.game_data);
        self.scheduler.execute(&mut self.game_data);
        self.layer_machine
            .apply_scene_action(SceneAction::LateUpdate, &mut self.game_data);

        for i in 0..self.thread_manager.total_contexts() {
            if !self.thread_manager.get_should_break() {
                self.thread_manager.set_current_id(i as u32);
                self.exec_script_bytecode(i as u32, frame_duration.as_micros() as u64);
            }
        }
        // self.thread_manager.set_current_id(0);

        self.update_cursor();
        // self.game_data.inputs().reset_inputs();
    }

    fn update_cursor(&mut self) {
        {
            let mut window = self.game_data.window();
            if let Some(icon) = window.new_cursor() {
                let w = self
                    .window
                    .as_mut()
                    .expect("A window is mandatory to run this game !");
                w.set_cursor_icon(*icon);
            }
            if let Some(dimensions) = window.new_dimensions() {
                let w = self
                    .window
                    .as_mut()
                    .expect("A window is mandatory to run this game !");
                let _r = w.request_inner_size(Size::Physical(PhysicalSize::new(
                    dimensions.0 * window.dpi() as u32,
                    dimensions.1 * window.dpi() as u32,
                )));
            }
            window.reset_future_settings()
        }
    }

    pub fn find_hcb(game_path: impl AsRef<Path>) -> Result<PathBuf> {
        let mut path = game_path.as_ref().to_path_buf();
        path.push("*.hcb");

        let macthes: Vec<_> = glob::glob(&path.to_string_lossy())?.flatten().collect();

        if macthes.is_empty() {
            anyhow::bail!("No hcb file found in the game directory");
        }

        Ok(macthes[0].to_path_buf())
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

    async fn init_render(window: Arc<Window>) -> (Arc<GpuCommonResources>, RenderTarget) {
        let size = window.inner_size();
        let backends = wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::all());
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        let surface = instance.create_surface(window.as_ref()).unwrap();

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

        let window_size = (window.inner_size().width, window.inner_size().height);
        let camera = Camera::new(window_size);

        let resources = Arc::new(GpuCommonResources {
            device,
            queue,
            render_buffer_size: RwLock::new(camera.render_buffer_size()),
            bind_group_layouts,
            pipelines,
        });

        let render_target = RenderTarget::new(
            &resources,
            camera.render_buffer_size(),
            Some("Window RenderTarget"),
        );

        (resources, render_target)
    }

    /// Builds, setups and runs the application, must be called at the end of the building process.
    pub fn run(mut self) {
        let event_loop = EventLoop::new().expect("Event loop could not be created");
        event_loop.set_control_flow(ControlFlow::Poll);

        let window_builder: WindowBuilder = self
            .config
            .window_config
            .clone()
            .expect("The window configuration has not been found")
            .into(&self.config);
        let window = window_builder
            .build(&event_loop)
            .expect("An error occured while building the main game window");

        let window = Arc::new(window);

        self.add_late_internal_systems_to_schedule();

        // let renderer_state =
        //     futures::executor::block_on(RendererState::new(window.clone()));

        let (resources, render_target) =
            futures::executor::block_on(AppBuilder::init_render(window.clone()));

        let entry_point = self.parser.get_entry_point();
        let non_volatile_global_count = self.parser.get_non_volatile_global_count();
        let volatile_global_count = self.parser.get_volatile_global_count();
        GLOBAL
            .lock()
            .unwrap()
            .init_with(non_volatile_global_count, volatile_global_count);
        
        self.script_engine.start_main(entry_point);
        self.world.nls = self.parser.nls.clone();

        let mut app = App {
            config: self.config,
            game_data: self.world,
            scheduler: self.scheduler,
            layer_machine: SceneMachine {
                current_scene: self.scene,
            },
            window: Some(window.clone()),
            // renderer: Some(renderer_state),
            parser: self.parser,
            thread_manager: self.script_engine,
            render_target,
            resources,
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

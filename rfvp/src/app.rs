use anyhow::Result;
use std::{
    collections::HashMap, fs::File, path::{Path, PathBuf}, slice::Windows, sync::{Arc, RwLock}, time::Instant
};
use regex::Regex;
use crate::{
    script::{
        context::ThreadState,
        global::GLOBAL,
        parser::{Nls, Parser}, Variant,
    },
    subsystem::resources::{
        motion_manager::DissolveType, thread_manager::ThreadManager, thread_wrapper::ThreadRequest,
    }, utils::ani::{self, icondir_to_custom_cursor, CursorBundle},
};
use winit::{dpi::{PhysicalSize, Size}, window::CustomCursor};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

use crate::rendering::render_tree::RenderTree;



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
    current_thread_id: Option<u32>,
    render_tree: RenderTree,
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
        if self.game_data.has_cursor(1) {
            self.game_data.switch_cursor(1);
        }
    }

    fn initialize_internal_resources(&mut self) {
        let window = self.window.as_ref().expect("No window found during setup");
        self.game_data
            .set_window(crate::subsystem::resources::window::Window::new(
                (window.inner_size().width, window.inner_size().height),
                window.scale_factor(),
            ));
    }

    fn debug_title(&mut self, x: i32, y: i32) {
        let title = self.parser.get_title();
        let test = std::env::var("FVP_TEST");
        if let Ok(test) = test {
            if test == *"1" {
                let title = format!("{} | {},{} | down {}, up {} | ", 
                    title, 
                    x, 
                    y,
                    self.debug_keydown(),
                    self.debug_keyup()
                );
                self.window.as_mut().unwrap().set_title(&title);
            }
        }
    }

    fn debug_keydown(&self) -> String {
        self.game_data.inputs_manager.get_input_down().to_string()
    }

    fn debug_keyup(&self) -> String {
        self.game_data.inputs_manager.get_input_up().to_string()
    }

    fn get_current_thread(&self) -> u32 {
        self.thread_manager.get_current_id()
    }

    fn set_current_thread(&mut self, id : u32) {
        self.thread_manager.set_current_id(id);
        self.game_data.set_current_thread(id);
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
                    self.debug_title(self.game_data.inputs_manager.get_cursor_x(), self.game_data.inputs_manager.get_cursor_y());
                }
                _ => (),
            }
        });
    }

    fn exec_script_bytecode(&mut self, id: u32, frame_time: u64) {
        // let ctx = self.game_data.thread_manager.get_thread(id);
        let dissolve_type = self.game_data.motion_manager.get_dissolve_type();
        let status = self.thread_manager.get_context_status(id);
        if status.contains(ThreadState::CONTEXT_STATUS_WAIT) {
            let wait_time = self.thread_manager.get_context_waiting_time(id);
            if wait_time > frame_time {
                self.thread_manager
                    .set_context_waiting_time(id, wait_time - frame_time);
            } else {
                self.thread_manager.set_context_waiting_time(id, 0);
                let mut new_status = status.clone();
                new_status.remove(ThreadState::CONTEXT_STATUS_WAIT);
                self.thread_manager
                    .set_context_status(id, new_status);
            }
        }

        // dissolve wait
        if status.contains(ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT)
            && (dissolve_type == DissolveType::None || dissolve_type == DissolveType::Static)
        {
            let mut new_status = status.clone();
            new_status.remove(ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT);
            self.thread_manager
                .set_context_status(id, new_status);
        }

        if status.contains(ThreadState::CONTEXT_STATUS_RUNNING) {
            self.thread_manager.set_context_should_break(id, false);
            while !self.thread_manager.get_context_should_break(id) {
                // let mut ctx = self.thread_manager.get_thread(id);
                let result = self
                    .thread_manager
                    .context_dispatch_opcode(id, &mut self.game_data, &mut self.parser);
                if self.thread_manager.get_contexct_should_exit(id) {
                    self.thread_manager.thread_exit(Some(id));
                    break;
                }
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


        if self.game_data.get_halt() {
            self.game_data.set_halt(false);
        }
        else {
            self.game_data.inputs_manager.refresh_input();
            self.set_current_thread(0);
        }

        if !self.game_data.get_halt() {
            let mut current_thread = self.get_current_thread();
            loop {
                if current_thread >= self.thread_manager.total_contexts() as u32 {
                    break;
                }

                if !self.game_data.get_game_should_exit() || self.game_data.get_last_current_thread() == current_thread {
                    self.set_current_thread(current_thread);
                    self.exec_script_bytecode(current_thread, frame_duration.as_micros() as u64);
                }

                current_thread += 1;
                if self.game_data.get_halt() {
                    break;
                }
            }
        }

        self.game_data.inputs_manager.frame_reset();
        self.update_cursor();
        // self.game_data.inputs().reset_inputs();
    }

    fn update_cursor(&mut self) {
        let cursor_frame = self.game_data.update_cursor();
        let mut window = self.game_data.window();
        let w = self
            .window
            .as_mut()
            .expect("A window is mandatory to run this game !");
        if let Some(frame) = cursor_frame {
            w.set_cursor(frame);
        }
        
        window.reset_future_settings()
    }

    pub fn find_hcb(game_path: impl AsRef<Path>) -> Result<PathBuf> {
        let mut path = game_path.as_ref().to_path_buf();
        path.push("*.hcb");

        let matches: Vec<_> = glob::glob(&path.to_string_lossy())?.flatten().collect();

        if matches.is_empty() {
            anyhow::bail!("No hcb file found in the game directory");
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
            current_thread_id: None,
            render_tree: RenderTree::new(),
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

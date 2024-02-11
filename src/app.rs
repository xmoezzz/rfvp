use anyhow::Result;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    rendering,
    script::{
        global::Global,
        parser::{Nls, Parser},
    },
    subsystem::resources::scripter::ScriptScheduler,
};
use winit::dpi::{PhysicalSize, Size};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

use crate::subsystem::package::Package;
use crate::subsystem::resources::time::Time;
use crate::subsystem::scene::{Scene, SceneAction, SceneMachine};
use crate::subsystem::scheduler::Scheduler;
use crate::subsystem::systems::collider_systems::collider_cleaner_system;
use crate::subsystem::systems::InternalPackage;
use crate::subsystem::world::GameData;
use crate::{
    config::app_config::{AppConfig, AppConfigReader},
    rendering::{renderer_state::RendererState, RendererType},
    subsystem::event_handler::update_input_events,
};

pub struct App {
    config: AppConfig,
    game_data: GameData,
    scheduler: Scheduler,
    layer_machine: SceneMachine,
    window: Option<Arc<Window>>,
    renderer: Option<RendererState>,
    script_engine: ScriptScheduler,
    parser: Parser,
    global: Global,
}

impl App {
    pub fn app() -> AppBuilder {
        let app_config = AppConfigReader::read_or_create_default_scion_json().expect(
            "Fatal error when trying to retrieve and deserialize `app.json` configuration file.",
        );
        App::app_with_config(app_config)
    }

    pub fn app_with_config_path(config_path: impl AsRef<Path>) -> AppBuilder {
        let app_config = AppConfigReader::read_app_json(config_path.as_ref()).expect(
            "Fatal error when trying to retrieve and deserialize `app.json` configuration file.",
        );
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
            .insert_resource(crate::subsystem::resources::window::Window::new(
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
                            self.renderer.as_mut().unwrap().resize(
                                *physical_size,
                                self.window.as_ref().expect("Missing window").scale_factor(),
                            );
                        }
                        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                            self.renderer.as_mut().unwrap().resize(
                                self.window.as_ref().expect("Missing window").inner_size(),
                                *scale_factor,
                            );
                        }
                        WindowEvent::CursorMoved {
                            device_id: _,
                            position,
                            ..
                        } => {
                            let dpi_factor = self
                                .window
                                .as_mut()
                                .unwrap()
                                .current_monitor()
                                .expect("Missing the monitor")
                                .scale_factor();
                            self.game_data.inputs().set_mouse_position(
                                position.x / dpi_factor,
                                position.y / dpi_factor,
                            );
                        }
                        WindowEvent::RedrawRequested => {
                            self.renderer.as_mut().unwrap().update(&mut self.game_data);
                            match self
                                .renderer
                                .as_mut()
                                .unwrap()
                                .render(&mut self.game_data, &self.config)
                            {
                                Ok(_) => {}
                                Err(e) => log::error!("{:?}", e),
                            }
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

    #[inline]
    fn sixty_fps_time() -> u64 {
        16
    }

    fn next_frame(&mut self) {
        let frame_duration = self
            .game_data
            .get_resource_mut::<Time>()
            .expect("Time is an internal resource and can't be missing")
            .frame();
        self.game_data.timers().add_delta_duration(frame_duration);

        let rendering_time = std::time::Instant::now();
        self.layer_machine
            .apply_scene_action(SceneAction::Update, &mut self.game_data);
        self.scheduler.execute(&mut self.game_data);
        self.layer_machine
            .apply_scene_action(SceneAction::LateUpdate, &mut self.game_data);

        let rendering_time = rendering_time.elapsed().as_millis() as u64;
        let script_time = if rendering_time < Self::sixty_fps_time() {
            Self::sixty_fps_time() - rendering_time
        } else {
            5 // 5ms is the minimum time we want to give to the script engine
        };

        if let Err(e) = self.script_engine.execute(
            rendering_time,
            script_time,
            &self.game_data,
            &mut self.parser,
            &mut self.global,
        ) {
            log::error!("script error: {:?}", e);
        }
        self.update_cursor();
        self.game_data.inputs().reset_inputs();
        self.game_data.events().cleanup();
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

    // pub fn new(game_path: impl AsRef<Path>) -> Result<Self> {
    //     let opcode_file = Self::find_hcb(game_path.as_ref())?;
    //     let parser = Parser::new(opcode_file, Nls::ShiftJIS).unwrap();
    //     let app = App { parser, game_path: game_path.as_ref().to_path_buf()};
    //     Ok(app)
    // }

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
    renderer: RendererType,
    scene: Option<Box<dyn Scene>>,
    world: GameData,
    title: String,
    size: (u32, u32),
    script_engine: ScriptScheduler,
    parser: Parser,
    global: Global,
}

impl AppBuilder {
    fn new(config: AppConfig) -> Self {
        let builder = Self {
            config,
            scheduler: Default::default(),
            renderer: Default::default(),
            scene: Default::default(),
            world: Default::default(),
            title: Default::default(),
            size: Default::default(),
            script_engine: Default::default(),
            parser: Default::default(),
            global: Default::default(),
        };
        builder.with_package(InternalPackage)
    }

    /// Specify a system to add to the scheduler.
    pub fn with_system(mut self, system: fn(&mut GameData)) -> Self {
        self.scheduler.add_system(system);
        self
    }

    /// Specify which render type you want to use. Note that by default if not set, `Scion` will use [`crate::rendering::RendererType::Scion2D`].
    pub fn with_renderer(mut self, renderer_type: RendererType) -> Self {
        self.renderer = renderer_type;
        self
    }

    /// Add a normal game layer to the pile. Every layer added before in the pile will be called
    pub fn with_scene<T: Scene + Default + 'static>(mut self) -> Self {
        self.scene = Some(Box::new(T::default()));
        self
    }

    ///
    pub fn with_package<P: Package>(mut self, package: P) -> Self {
        package.prepare(&mut self.world);
        package.load(self)
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

    pub fn with_script_engine(mut self, script_engine: ScriptScheduler) -> Self {
        self.script_engine = script_engine;
        self
    }

    pub fn with_parser(mut self, parser: Parser) -> Self {
        self.parser = parser;
        self
    }

    pub fn with_global(mut self, global: Global) -> Self {
        self.global = global;
        self
    }

    /// Builds, setups and runs the Scion application, must be called at the end of the building process.
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

        let renderer = self.renderer.into_boxed_renderer();
        let renderer_state =
            futures::executor::block_on(RendererState::new(window.clone(), renderer));

        let mut app = App {
            config: self.config,
            game_data: self.world,
            scheduler: self.scheduler,
            layer_machine: SceneMachine {
                current_scene: self.scene,
            },
            window: Some(window.clone()),
            renderer: Some(renderer_state),
            script_engine: self.script_engine,
            parser: self.parser,
            global: self.global,
        };

        app.setup();
        app.run(event_loop);
    }

    fn add_late_internal_systems_to_schedule(&mut self) {
        self.scheduler.add_system(collider_cleaner_system);
    }
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

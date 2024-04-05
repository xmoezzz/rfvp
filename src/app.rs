use anyhow::Result;
use std::{
    cell::{RefCell, RefMut},
    path::{Path, PathBuf},
    process::exit,
    rc::Rc,
    sync::Arc,
};

use crate::{
    rendering,
    script::{
        context::{
            Context, CONTEXT_STATUS_DISSOLVE_WAIT, CONTEXT_STATUS_RUNNING, CONTEXT_STATUS_WAIT,
        }, global::Global, opcode::Opcode, parser::{Nls, Parser}, VmSyscall
    },
    subsystem::{
        components::syscalls::other_anm::Dissolve,
        resources::{motion_manager::DissolveType, scripter::ScriptScheduler},
    },
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
use crate::subsystem::systems::InternalPackage;
use crate::subsystem::world::GameData;
use crate::{
    config::app_config::AppConfig,
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
    parser: Parser,
    global: Global,
}

impl App {
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

    // #[inline]
    // fn dispatch_opcode(mut ctx: RefMut<'_, Context>, syscaller: &mut impl VmSyscall, parser: &mut Parser, global: &mut Global) -> Result<()> {
    //     let opcode = parser.read_u8(ctx.get_pc())? as i32;
        
    //     match opcode.try_into() {
    //         Ok(Opcode::Nop) => {
    //             ctx.nop()?;
    //         }
    //         Ok(Opcode::InitStack) => {
    //             ctx.init_stack(&mut self.parser)?;
    //         }
    //         Ok(Opcode::Call) => {
    //             ctx.call(&mut self.parser)?;
    //         }
    //         Ok(Opcode::Syscall) => {
    //             ctx.syscall(syscaller, parser)?;
    //         }
    //         Ok(Opcode::Ret) => {
    //             ctx.ret()?;
    //         }
    //         Ok(Opcode::RetV) => {
    //             ctx.retv()?;
    //         }
    //         Ok(Opcode::Jmp) => {
    //             ctx.jmp(&mut self.parser)?;
    //         }
    //         Ok(Opcode::Jz) => {
    //             ctx.jz(&mut self.parser)?;
    //         }
    //         Ok(Opcode::PushNil) => {
    //             ctx.push_nil()?;
    //         }
    //         Ok(Opcode::PushTrue) => {
    //             ctx.push_true()?;
    //         }
    //         Ok(Opcode::PushI32) => {
    //             ctx.push_i32(&mut self.parser)?;
    //         }
    //         Ok(Opcode::PushI16) => {
    //             ctx.push_i16(&mut self.parser)?;
    //         }
    //         Ok(Opcode::PushI8) => {
    //             ctx.push_i8(&mut self.parser)?;
    //         }
    //         Ok(Opcode::PushF32) => {
    //             ctx.push_f32(&mut self.parser)?;
    //         }
    //         Ok(Opcode::PushString) => {
    //             ctx.push_string(&mut self.parser)?;
    //         }
    //         Ok(Opcode::PushGlobal) => {
    //             ctx.push_global(parser, global)?;
    //         }
    //         Ok(Opcode::PushStack) => {
    //             ctx.push_stack(&mut self.parser)?;
    //         }
    //         Ok(Opcode::PushGlobalTable) => {
    //             ctx.push_global_table(parser, global)?;
    //         }
    //         Ok(Opcode::PushLocalTable) => {
    //             ctx.push_local_table(&mut self.parser)?;
    //         }
    //         Ok(Opcode::PushTop) => {
    //             ctx.push_top()?;
    //         }
    //         Ok(Opcode::PushReturn) => {
    //             ctx.push_return_value()?;
    //         }
    //         Ok(Opcode::PopGlobal) => {
    //             ctx.pop_global(parser, global)?;
    //         }
    //         Ok(Opcode::PopStack) => {
    //             ctx.local_copy(&mut self.parser)?;
    //         }
    //         Ok(Opcode::PopGlobalTable) => {
    //             ctx.pop_global_table(parser, global)?;
    //         }
    //         Ok(Opcode::PopLocalTable) => {
    //             ctx.pop_local_table(&mut self.parser)?;
    //         }
    //         Ok(Opcode::Neg) => {
    //             ctx.neg()?;
    //         }
    //         Ok(Opcode::Add) => {
    //             ctx.add()?;
    //         }
    //         Ok(Opcode::Sub) => {
    //             ctx.sub()?;
    //         }
    //         Ok(Opcode::Mul) => {
    //             ctx.mul()?;
    //         }
    //         Ok(Opcode::Div) => {
    //             ctx.div()?;
    //         }
    //         Ok(Opcode::Mod) => {
    //             ctx.modulo()?;
    //         }
    //         Ok(Opcode::BitTest) => {
    //             ctx.bittest()?;
    //         }
    //         Ok(Opcode::And) => {
    //             ctx.and()?;
    //         }
    //         Ok(Opcode::Or) => {
    //             ctx.or()?;
    //         }
    //         Ok(Opcode::SetE) => {
    //             ctx.sete()?;
    //         }
    //         Ok(Opcode::SetNE) => {
    //             ctx.setne()?;
    //         }
    //         Ok(Opcode::SetG) => {
    //             ctx.setg()?;
    //         }
    //         Ok(Opcode::SetLE) => {
    //             ctx.setle()?;
    //         }
    //         Ok(Opcode::SetL) => {
    //             ctx.setl()?;
    //         }
    //         Ok(Opcode::SetGE) => {
    //             ctx.setge()?;
    //         }
    //         _ => {
    //             ctx.nop()?;
    //             log::error!("unknown opcode: {}", opcode);
    //         }
    //     };

    //     Ok(())
    // }

    fn exec_script_bytecode(&mut self, id: u32, frame_time: u64) {
        // let ctx = self.game_data.thread_manager.get_thread(id);
        let dissolve_type = self.game_data.motion_manager.get_dissolve_type();
        let status = self.game_data.thread_manager.get_thread(id).get_status();
        if status & CONTEXT_STATUS_WAIT != 0 {
            let wait_time = self.game_data.thread_manager.get_thread(id).get_waiting_time();
            if wait_time > frame_time {
                self.game_data.thread_manager.get_thread(id).set_waiting_time(wait_time - frame_time);
            } else {
                self.game_data.thread_manager.get_thread(id).set_waiting_time(0);
                self.game_data.thread_manager.get_thread(id).set_status(status & 0xFFFFFFFD);
            }
        }

        // dissolve wait
        if status & CONTEXT_STATUS_DISSOLVE_WAIT != 0
            && (dissolve_type == DissolveType::None || dissolve_type == DissolveType::Static)
        {
            self.game_data.thread_manager.get_thread(id).set_status(status & 0xFFFFFFEF);
        }

        if status & CONTEXT_STATUS_RUNNING != 0 {
            self.game_data.thread_manager.get_thread(id).set_should_break(false);
            while !self.game_data.thread_manager.get_thread(id).should_break() {
                let mut ctx = self.game_data.thread_manager.get_thread(id);
                let result = ctx.dispatch_opcode(&mut self.game_data, &mut self.parser, &mut self.global); // Pass game_data instead of self.game_data
                
            }
        }

    }

    fn next_frame(&mut self) {
        let frame_duration = self
            .game_data
            .get_resource_mut::<Time>()
            .expect("Time is an internal resource and can't be missing")
            .frame();
        self.game_data.timers().add_delta_duration(frame_duration);

        self.layer_machine
            .apply_scene_action(SceneAction::Update, &mut self.game_data);
        self.scheduler.execute(&mut self.game_data);
        self.layer_machine
            .apply_scene_action(SceneAction::LateUpdate, &mut self.game_data);

        for i in 0..self.game_data.thread_manager.total_contexts() {
            if !self.game_data.thread_manager.get_should_break() {
                self.game_data.thread_manager.set_current_id(i as u32);
                self.exec_script_bytecode(i as u32, frame_duration.as_micros() as u64);
            }
        }
        self.game_data.thread_manager.set_current_id(0);

        self.update_cursor();
        // self.game_data.inputs().reset_inputs();
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
    parser: Parser,
    global: Global,
    script_engine: ScriptScheduler,
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
            parser: Default::default(),
            global: Default::default(),
            script_engine: Default::default(),
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
        self.scene = Some(Box::<T>::default());
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

        self.world.script_scheduler = self.script_engine;
        let entry_point = self.parser.get_entry_point();
        let non_volatile_global_count = self.parser.get_non_volatile_global_count();
        let volatile_global_count = self.parser.get_volatile_global_count();
        self.global
            .init_with(non_volatile_global_count, volatile_global_count);
        self.world.thread_manager.start_main(entry_point);
        self.world.nls = self.parser.nls.clone();

        let mut app = App {
            config: self.config,
            game_data: self.world,
            scheduler: self.scheduler,
            layer_machine: SceneMachine {
                current_scene: self.scene,
            },
            window: Some(window.clone()),
            renderer: Some(renderer_state),
            parser: self.parser,
            global: self.global,
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

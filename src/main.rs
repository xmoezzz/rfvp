mod script;
mod subsystem;
mod app;
mod utils;
mod rendering;
mod config;


use script::{global::Global, parser::{Nls, Parser}};
use subsystem::{resources::scripter::ScriptScheduler, world::World};

use crate::{
    config::{
        logger_config::LoggerConfig, app_config::{AppConfigBuilder, AppConfig},
        window_config::WindowConfigBuilder,
    },
    subsystem::scene::Scene,
    app::App,
    utils::file::app_base_path,
};

use crate::subsystem::world::GameData;

use anyhow::Result;
use log::LevelFilter;


#[derive(Default)]
pub struct MainScene {
}

impl Scene for MainScene {
    fn on_start(&mut self, data: &mut GameData) {
        data.add_default_camera();
    }
}

fn app_config(title: &str, size: (u32, u32)) -> AppConfig {
    AppConfigBuilder::new()
        .with_app_name(title.to_string())
        .with_logger_config(LoggerConfig { app_level_filter: LevelFilter::Info, level_filter: LevelFilter::Debug })
        .with_window_config(
            WindowConfigBuilder::new()
                .with_dimensions(size)
                .with_resizable(false)
                .get(),
        )
        .get()
}


fn load_script(nls: Nls) -> Result<Parser> {
    let base_path = app_base_path();
    let opcode_path = App::find_hcb(base_path.get_path())?;

    Parser::new(opcode_path, nls)
}

fn main() -> Result<()> {
    let parser = load_script(Nls::ShiftJIS)?;
    let title  = parser.get_title();
    let size = parser.get_screen_size();
    let script_engine = ScriptScheduler::new();

    App::app_with_config(app_config(&title, size))
        .with_scene::<MainScene>()
        .with_script_engine(script_engine)
        .with_window_title(&title)
        .with_window_size(size)
        .with_global(Global::new())
        .with_parser(parser)
        .with_vfs(Nls::ShiftJIS)?
        .run();
    Ok(())
}

mod script;
mod subsystem;
mod app;
mod utils;
mod rendering;
mod config;
mod window;
mod audio_player;

use script::parser::{Nls, Parser};
use subsystem::{anzu_scene::AnzuScene, resources::thread_manager::ThreadManager};

use crate::{
    config::{
        logger_config::LoggerConfig, app_config::{AppConfigBuilder, AppConfig},
        window_config::WindowConfigBuilder,
    },
    app::App,
    utils::file::app_base_path,
};

use crate::subsystem::world::GameData;

use anyhow::Result;
use log::LevelFilter;


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
    let script_engine = ThreadManager::new();

    App::app_with_config(app_config(&title, size))
        .with_scene::<AnzuScene>()
        .with_script_engine(script_engine)
        .with_window_title(&title)
        .with_window_size(size)
        .with_parser(parser)
        .with_vfs(Nls::ShiftJIS)?
        .run();
    Ok(())
}

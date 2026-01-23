use anyhow::Result;
use crate::{
    config::{
        logger_config::LoggerConfig, app_config::{AppConfigBuilder, AppConfig},
        window_config::WindowConfigBuilder,
    },
    app::App,
    utils::file::app_base_path,
};
use log::LevelFilter;

use crate::script::parser::{Nls, Parser};
use crate::subsystem::{anzu_scene::AnzuScene, resources::thread_manager::ThreadManager};


pub fn app_config(title: &str, size: (u32, u32)) -> AppConfig {
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


pub fn load_script(nls: Nls) -> Result<Parser> {
    let base_path = app_base_path();
    let opcode_path = App::find_hcb(base_path.get_path())?;

    Parser::new(opcode_path, nls)
}

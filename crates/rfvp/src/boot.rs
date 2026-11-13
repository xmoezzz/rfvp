use crate::{
    app::App,
    config::{
        app_config::{AppConfig, AppConfigBuilder},
        logger_config::LoggerConfig,
        window_config::WindowConfigBuilder,
    },
    utils::file::app_base_path,
};
use anyhow::Result;
use log::LevelFilter;

use crate::script::parser::{Nls, Parser};
use crate::subsystem::{anzu_scene::AnzuScene, resources::thread_manager::ThreadManager};

pub fn app_config(title: &str, size: (u32, u32)) -> AppConfig {
    AppConfigBuilder::new()
        .with_app_name(title.to_string())
        .with_logger_config(LoggerConfig {
            app_level_filter: LevelFilter::Info,
            level_filter: LevelFilter::Debug,
        })
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
    if let Some(parent) = opcode_path.parent() {
        if let Some(parent) = parent.to_str() {
            crate::utils::file::set_base_path(parent);
            crate::utils::file::set_hcb_root_path(parent);
        }
    }

    Parser::new(opcode_path, nls)
}

mod script;
mod subsystem;
mod app;
mod utils;
mod rendering;
mod config;
mod window;
mod audio_player;
mod debug;
mod vm_worker;
mod rfvp_render;
mod rfvp_audio;
mod vm_runner;

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
    // let mut handle = start_debug_ui();
    // let _ = handle.start_capture();

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

    // handle.shutdown();
    
    Ok(())
}


// test
mod tests {
    use std::{thread::sleep, time::Duration};
    use super::*;

    #[test]
    fn test_audio_system() {
        std::env::set_var("FVP_TEST", "1");
        let mut world = GameData::default();
        let vfs=  crate::subsystem::resources::vfs::Vfs::new(Nls::ShiftJIS).unwrap();
        let buff = vfs.read_file("bgm/001").unwrap();
        // is oggs?
        assert!(&buff[0..4] == [0x4fu8, 0x67u8, 0x67u8, 0x53u8].as_slice(), "BGM file is not OGG format");
        println!("BGM data size: {}", buff.len());
        world.bgm_player_mut().load(0, buff).unwrap();
        let mut fade_in = kira::Tween {
            duration: Duration::from_secs(0),
            ..Default::default()
        };
        fade_in.duration = Duration::from_secs(0);
        world.bgm_player_mut().play(0, true, 1.0, 0.5, fade_in).unwrap();
        sleep(Duration::from_secs(20));
    }

    #[test]
    fn test_audio_system_mix() {
        std::env::set_var("FVP_TEST", "1");
        let mut world = GameData::default();
        let vfs=  crate::subsystem::resources::vfs::Vfs::new(Nls::ShiftJIS).unwrap();
        let buff = vfs.read_file("bgm/001").unwrap();
        let buff2 = vfs.read_file("bgm/002").unwrap();
        // is oggs?
        assert!(&buff[0..4] == [0x4fu8, 0x67u8, 0x67u8, 0x53u8].as_slice(), "BGM file is not OGG format");
        assert!(&buff2[0..4] == [0x4fu8, 0x67u8, 0x67u8, 0x53u8].as_slice(), "BGM file is not OGG format");
        println!("BGM data size: {}", buff.len());
        world.bgm_player_mut().load(0, buff).unwrap();
        let mut fade_in = kira::Tween {
            duration: Duration::from_secs(0),
            ..Default::default()
        };
        world.bgm_player_mut().load(1, buff2).unwrap();
        fade_in.duration = Duration::from_secs(0);
        world.bgm_player_mut().play(0, true, 1.0, 0.5, fade_in).unwrap();
        world.bgm_player_mut().play(1, true, 1.0, 0.5, fade_in).unwrap();
        sleep(Duration::from_secs(20));
    }
}

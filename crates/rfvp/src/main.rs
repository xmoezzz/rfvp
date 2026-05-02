mod script;
mod subsystem;
mod app;
mod utils;
mod rendering;
mod config;
mod window;
mod audio_player;
mod debug_ui;
mod vm_worker;
mod rfvp_render;
mod rfvp_audio;
mod vm_runner;
mod trace;
mod font;
mod boot;
mod legacy_save_load_ui;
mod exit_confirm_ui;

pub(crate) mod platform_time;

use script::parser::{Nls, Parser};
use subsystem::{anzu_scene::AnzuScene, resources::thread_manager::ThreadManager};

use anyhow::Result;
use log::LevelFilter;
use boot::{app_config, load_script};
use crate::app::App;

/// Parse `--nls <value>` or `--nls=<value>` from argv, default to ShiftJIS.
fn parse_nls_arg() -> Nls {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if let Some(val) = a.strip_prefix("--nls=") {
            return val.parse().unwrap_or_else(|e| {
                eprintln!("rfvp: {e}");
                std::process::exit(1);
            });
        } else if a == "--nls" {
            if let Some(val) = args.get(i + 1) {
                return val.parse().unwrap_or_else(|e| {
                    eprintln!("rfvp: {e}");
                    std::process::exit(1);
                });
            } else {
                eprintln!("rfvp: --nls requires a value (sjis, gbk, utf8)");
                std::process::exit(1);
            }
        }
        i += 1;
    }
    Nls::ShiftJIS
}

// use dhat;

// #[global_allocator]
// static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() -> Result<()> {
    // let _profiler = dhat::Profiler::new_heap();
    // env_logger::init();
    let nls = parse_nls_arg();
    let parser = load_script(nls)?;
    let title  = parser.get_title();
    let size = parser.get_screen_size();
    let script_engine = ThreadManager::new();

    App::app_with_config(app_config(&title, size))
        .with_scene::<AnzuScene>()
        .with_script_engine(script_engine)
        .with_window_title(&title)
        .with_window_size(size)
        .with_parser(parser)
        .with_vfs(nls)?
        .run();

    // handle.shutdown();
    
    Ok(())
}


// test
mod tests {
    use std::{thread::sleep, time::Duration};
    use super::*;
    use crate::subsystem::world::GameData;

    #[test]
    fn test_audio_system() {
        std::env::set_var("FVP_TEST", "1");
        let mut world = GameData::default();
        let vfs=  crate::subsystem::resources::vfs::Vfs::new(Nls::ShiftJIS).unwrap();
        let buff = vfs.read_file("bgm/001").unwrap();
        // is oggs?
        assert!(&buff[0..4] == [0x4fu8, 0x67u8, 0x67u8, 0x53u8].as_slice(), "BGM file is not OGG format");
        crate::trace::vm(format_args!("BGM data size: {}", buff.len()));
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
        crate::trace::vm(format_args!("BGM data size: {}", buff.len()));
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

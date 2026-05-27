extern crate alloc;

#[cfg(feature = "no_std")]
fn main() {
    eprintln!("rfvp `no_std` is a library-only core surface; link the rfvp library from a platform SDK host.");
}

#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod app;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod audio_player;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod boot;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod config;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod debug_ui;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod exit_confirm_ui;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod font;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod legacy_save_load_ui;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod rendering;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod rfvp_audio;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod rfvp_render;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod script;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod subsystem;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod trace;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod utils;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod vm_runner;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod vm_worker;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
mod window;

#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
pub(crate) mod platform_random;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
pub(crate) mod platform_time;

#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use script::parser::Nls;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use subsystem::{anzu_scene::AnzuScene, resources::thread_manager::ThreadManager};

#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use crate::app::App;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use crate::utils::file::set_base_path;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use anyhow::Result;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use boot::{app_config, load_script};

/// Parse `--project-dir <path>` or `--project-dir=<path>` from argv.
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
fn parse_project_dir_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if let Some(val) = a.strip_prefix("--project-dir=") {
            if !val.is_empty() {
                return Some(val.to_string());
            }
        } else if a == "--project-dir" {
            if let Some(val) = args.get(i + 1) {
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
        i += 1;
    }
    None
}

/// Parse `--nls <value>` or `--nls=<value>` from argv, default to ShiftJIS.
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
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

/// Parse `--system-font`; when present, system-wide CJK fallback fonts are scanned.
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
fn parse_system_font_arg() -> bool {
    std::env::args().skip(1).any(|a| a == "--system-font")
}

// use dhat;

// #[global_allocator]
// static ALLOC: dhat::Alloc = dhat::Alloc;

#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
fn main() -> Result<()> {
    // let _profiler = dhat::Profiler::new_heap();
    // env_logger::init();
    if let Some(project_dir) = parse_project_dir_arg() {
        set_base_path(&project_dir);
    }
    let nls = parse_nls_arg();
    let system_font = parse_system_font_arg();
    let parser = load_script(nls)?;
    let title = parser.get_title();
    let size = parser.get_screen_size();
    let script_engine = ThreadManager::new();

    let app = App::app_with_config(app_config(&title, size))
        .with_scene::<AnzuScene>()
        .with_script_engine(script_engine)
        .with_window_title(&title)
        .with_window_size(size)
        .with_parser(parser)
        .with_vfs(nls)?;
    let app = if system_font {
        app.with_system_font(true)
    } else {
        app
    };
    app.run();

    // handle.shutdown();

    Ok(())
}

#[cfg(all(
    not(feature = "no_std"),
    not(feature = "gpu-render"),
    feature = "rfvp-os"
))]
fn main() -> anyhow::Result<()> {
    rfvp::rfvp_os_host::run_from_args()
}

#[cfg(all(
    not(feature = "no_std"),
    not(feature = "gpu-render"),
    not(feature = "rfvp-os"),
    feature = "soft-render-desktop"
))]
fn main() -> anyhow::Result<()> {
    rfvp::soft_host::run_from_args()
}

#[cfg(all(
    not(feature = "no_std"),
    not(feature = "gpu-render"),
    not(feature = "rfvp-os"),
    feature = "soft-render-core",
    not(feature = "soft-render-desktop")
))]
fn main() -> anyhow::Result<()> {
    let renderer =
        rfvp::soft_render::create_soft_renderer(320, 240, rfvp::soft_render::PixelFormat::Rgba8)?;
    let fb = renderer.framebuffer();
    println!(
        "rfvp soft-render-core initialized: {}x{} stride={} format={:?} bytes={}",
        fb.width(),
        fb.height(),
        fb.stride(),
        fb.format(),
        fb.pixels().len()
    );
    println!("No platform presentation loop is wired for soft-render-core. Use soft-render/soft-render-desktop or rfvp-os.");
    Ok(())
}

#[cfg(all(
    not(feature = "no_std"),
    not(feature = "gpu-render"),
    not(feature = "soft-render"),
    not(feature = "soft-render-core"),
    feature = "no-audio"
))]
fn main() -> anyhow::Result<()> {
    println!("rfvp no-audio feature check path initialized; enable gpu-render, soft-render-desktop, or rfvp-os to run an engine host.");
    Ok(())
}

#[cfg(all(
    not(feature = "no_std"),
    not(feature = "gpu-render"),
    not(feature = "soft-render"),
    not(feature = "soft-render-core"),
    not(feature = "no-audio")
))]
compile_error!("rfvp binary requires `gpu-render`, `soft-render`, `soft-render-core`, or `no-audio` for feature-check builds.");

// test
#[cfg(all(test, feature = "gpu-render", feature = "audio"))]
mod tests {
    use super::*;
    use crate::subsystem::world::GameData;
    use std::{thread::sleep, time::Duration};

    #[test]
    fn test_audio_system() {
        std::env::set_var("FVP_TEST", "1");
        let mut world = GameData::default();
        let vfs = crate::subsystem::resources::vfs::Vfs::new(Nls::ShiftJIS).unwrap();
        let buff = vfs.read_file("bgm/001").unwrap();
        // is oggs?
        assert!(
            &buff[0..4] == [0x4fu8, 0x67u8, 0x67u8, 0x53u8].as_slice(),
            "BGM file is not OGG format"
        );
        crate::trace::vm(format_args!("BGM data size: {}", buff.len()));
        world.bgm_player_mut().load(0, buff).unwrap();
        let mut fade_in = crate::rfvp_audio::Tween {
            duration: Duration::from_secs(0),
            ..Default::default()
        };
        fade_in.duration = Duration::from_secs(0);
        world
            .bgm_player_mut()
            .play(0, true, 1.0, 0.5, fade_in, &vfs)
            .unwrap();
        sleep(Duration::from_secs(20));
    }

    #[test]
    fn test_audio_system_mix() {
        std::env::set_var("FVP_TEST", "1");
        let mut world = GameData::default();
        let vfs = crate::subsystem::resources::vfs::Vfs::new(Nls::ShiftJIS).unwrap();
        let buff = vfs.read_file("bgm/001").unwrap();
        let buff2 = vfs.read_file("bgm/002").unwrap();
        // is oggs?
        assert!(
            &buff[0..4] == [0x4fu8, 0x67u8, 0x67u8, 0x53u8].as_slice(),
            "BGM file is not OGG format"
        );
        assert!(
            &buff2[0..4] == [0x4fu8, 0x67u8, 0x67u8, 0x53u8].as_slice(),
            "BGM file is not OGG format"
        );
        crate::trace::vm(format_args!("BGM data size: {}", buff.len()));
        world.bgm_player_mut().load(0, buff).unwrap();
        let mut fade_in = crate::rfvp_audio::Tween {
            duration: Duration::from_secs(0),
            ..Default::default()
        };
        world.bgm_player_mut().load(1, buff2).unwrap();
        fade_in.duration = Duration::from_secs(0);
        world
            .bgm_player_mut()
            .play(0, true, 1.0, 0.5, fade_in, &vfs)
            .unwrap();
        world
            .bgm_player_mut()
            .play(1, true, 1.0, 0.5, fade_in, &vfs)
            .unwrap();
        sleep(Duration::from_secs(20));
    }
}

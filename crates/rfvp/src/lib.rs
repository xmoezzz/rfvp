pub mod script;
pub mod subsystem;
pub mod app;
pub mod utils;
pub mod vm_runner;
pub mod rendering;
pub mod config;
pub mod audio_player;
pub mod window;
pub mod rfvp_render;
pub mod rfvp_audio;
pub mod vm_worker;
pub mod debug_ui;
pub mod trace;
pub mod boot;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;


use anyhow::Result;
use log::LevelFilter;
use boot::{app_config, load_script};
use crate::app::App;
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::utils::file::set_base_path;
use crate::script::parser::Nls;
use crate::subsystem::anzu_scene::AnzuScene;

fn run_rfvp(game_root: &str, nls: Nls) -> Result<()> {
    set_base_path(game_root);
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

#[no_mangle]
pub unsafe extern "C" fn rfvp_run_entry(game_root_utf8: *const c_char, nls_utf8: *const c_char) -> i32 {
    if game_root_utf8.is_null() || nls_utf8.is_null() {
        return 2;
    }

    let game_root = match CStr::from_ptr(game_root_utf8).to_str() {
        Ok(s) => s.to_string(),
        _ => {
            return 3;
        }
    };

    let nls_str = match CStr::from_ptr(nls_utf8).to_str() {
        Ok(s) if !s.is_empty() => s.to_lowercase(),
        _ => {
            return 4;
        }
    };

    let nls = match nls_str.as_str() {
        "shiftjis" | "sjis" => Nls::ShiftJIS,
        "utf8" | "utf-8" => Nls::UTF8,
        "gbk" | "gb2312" => Nls::GBK,
        _ => {
            return 5;
        }
    };

    match run_rfvp(&game_root, nls) {
        Ok(_) => 0,
        Err(e) => {
            log::error!("Error running RFVP: {:?}", e);
            1
        }
    }
}


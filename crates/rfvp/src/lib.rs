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

// iOS host-mode (SwiftUI / UIKit) entry points.
#[cfg(target_os = "ios")]
mod ios_host;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr::null_mut;
use std::time::Duration;


use anyhow::Result;
use log::LevelFilter;
use boot::{app_config, load_script};
use crate::app::App;
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::utils::file::set_base_path;
use crate::script::parser::Nls;
use crate::subsystem::anzu_scene::AnzuScene;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use winit::platform::pump_events::PumpStatus;

fn run_rfvp(game_root: &str, nls: Nls) -> Result<()> {
    set_base_path(game_root);
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

    Ok(())
}

/// Opaque pump handle for GUI hosts (e.g. SwiftUI launcher) that already own the platform main loop.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub struct RfvpPumpHandle {
    inst: crate::app::PumpInstance,
}

/// Create a pump-driven instance. Returns NULL on error.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[no_mangle]
pub unsafe extern "C" fn rfvp_pump_create(game_root_utf8: *const c_char, nls_utf8: *const c_char) -> *mut RfvpPumpHandle {
    if game_root_utf8.is_null() || nls_utf8.is_null() {
        return null_mut();
    }

    let game_root = match CStr::from_ptr(game_root_utf8).to_str() {
        Ok(s) if !s.is_empty() => s.to_string(),
        _ => return null_mut(),
    };

    let nls_str = match CStr::from_ptr(nls_utf8).to_str() {
        Ok(s) if !s.is_empty() => s.to_string(),
        _ => return null_mut(),
    };

    let nls: Nls = match nls_str.parse() {
        Ok(v) => v,
        Err(e) => {
            log::error!("rfvp_pump_create: invalid NLS '{nls_str}': {e:?}");
            return null_mut();
        }
    };

    // Build the app but do not enter the blocking run loop.
    set_base_path(&game_root);
    let parser = match load_script(nls) {
        Ok(p) => p,
        Err(e) => {
            log::error!("rfvp_pump_create: failed to load script: {e:?}");
            return null_mut();
        }
    };
    let title = parser.get_title();
    let size = parser.get_screen_size();
    let script_engine = ThreadManager::new();

    let builder = match App::app_with_config(app_config(&title, size))
        .with_scene::<AnzuScene>()
        .with_script_engine(script_engine)
        .with_window_title(&title)
        .with_window_size(size)
        .with_parser(parser)
        .with_vfs(nls)
    {
        Ok(b) => b,
        Err(e) => {
            log::error!("rfvp_pump_create: failed to build AppBuilder: {e:?}");
            return null_mut();
        }
    };

    let inst = match builder.build_pump() {
        Ok(i) => i,
        Err(e) => {
            log::error!("rfvp_pump_create: build_pump failed: {e:?}");
            return null_mut();
        }
    };

    Box::into_raw(Box::new(RfvpPumpHandle { inst }))
}

/// Pump events for up to `timeout_ms` milliseconds.
///
/// Return values:
/// - 0: continue running
/// - 1: app requested exit
/// - 2: invalid handle
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[no_mangle]
pub unsafe extern "C" fn rfvp_pump_step(handle: *mut RfvpPumpHandle, timeout_ms: u32) -> i32 {
    if handle.is_null() {
        return 2;
    }
    let h = &mut *handle;
    match h.inst.pump(Duration::from_millis(std::cmp::max(timeout_ms as u64, 1))) {
        PumpStatus::Continue => 0,
        _ => 1,
    }
}

/// Destroy a pump-driven instance created by `rfvp_pump_create`.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[no_mangle]
pub unsafe extern "C" fn rfvp_pump_destroy(handle: *mut RfvpPumpHandle) {
    if handle.is_null() {
        return;
    }
    drop(Box::from_raw(handle));
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


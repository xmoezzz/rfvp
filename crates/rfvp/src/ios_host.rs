//! iOS host-mode entry points.
//!
//! On iOS, GUI hosts like SwiftUI already own the platform main loop (UIApplicationMain / RunLoop).
//! Running winit's EventLoop::run from inside that environment will panic.
//!
//! This module exposes a small C ABI that lets the host:
//!   * create an engine instance bound to a UIKit view (CAMetalLayer-backed)
//!   * step the engine once per frame
//!   * resize the presentation surface
//!   * destroy the instance

#![cfg(target_os = "ios")]

use std::ffi::{c_char, c_void, CStr};
use std::ptr::NonNull;

use anyhow::{Context, Result};

use crate::app::{App, AppBuilder};
use crate::boot::{app_config, load_script};
use crate::script::parser::Nls;
use crate::subsystem::anzu_scene::AnzuScene;
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::utils::file::set_base_path;

struct IosInstance {
    app: Box<App>,
}

fn cstr_to_string(ptr: *const c_char) -> Result<String> {
    if ptr.is_null() {
        return Err(anyhow::anyhow!("null string pointer"));
    }
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .context("invalid utf-8")?;
    Ok(s.to_string())
}

fn build_app(view: NonNull<c_void>, surface_w: u32, surface_h: u32, scale: f64, game_root: &str, nls: Nls) -> Result<Box<App>> {
    set_base_path(game_root);
    let parser = load_script(nls)?;
    let title = parser.get_title();
    let virtual_size = parser.get_screen_size();
    let script_engine = ThreadManager::new();

    let builder = App::app_with_config(app_config(&title, virtual_size))
        .with_scene::<AnzuScene>()
        .with_script_engine(script_engine)
        .with_window_title(&title)
        .with_window_size(virtual_size)
        .with_parser(parser)
        .with_vfs(nls)?;

    builder.build_ios(view, (surface_w.max(1), surface_h.max(1)), scale)
}

/// Create an iOS host-mode instance bound to a UIKit view.
///
/// # Safety
/// - `ui_view` must be a valid pointer to a UIKit `UIView` (or a subclass) that is backed by a
///   `CAMetalLayer`.
/// - The view must remain alive for the lifetime of the returned instance.
#[no_mangle]
pub unsafe extern "C" fn rfvp_ios_create(
    ui_view: *mut c_void,
    surface_width: u32,
    surface_height: u32,
    scale: f64,
    game_root_utf8: *const c_char,
    nls_utf8: *const c_char,
) -> *mut c_void {
    let view = match NonNull::new(ui_view) {
        Some(v) => v,
        None => {
            eprintln!("[RFVP-IOS] rfvp_ios_create: ui_view is null");
            return std::ptr::null_mut();
        }
    };

    let root = match cstr_to_string(game_root_utf8) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[RFVP-IOS] rfvp_ios_create: invalid game_root_path: {e:?}");
            return std::ptr::null_mut();
        }
    };

    let nls_str = match cstr_to_string(nls_utf8) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[RFVP-IOS] rfvp_ios_create: invalid nls: {e:?}");
            return std::ptr::null_mut();
        }
    };

    let nls: Nls = match nls_str.parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[RFVP-IOS] rfvp_ios_create: invalid NLS '{nls_str}': {e:?}");
            return std::ptr::null_mut();
        }
    };

    match build_app(view, surface_width, surface_height, scale, &root, nls) {
        Ok(app) => {
            let inst = Box::new(IosInstance { app });
            Box::into_raw(inst) as *mut c_void
        }
        Err(e) => {
            eprintln!("[RFVP-IOS] rfvp_ios_create: build failed: {e:?}");
            std::ptr::null_mut()
        }
    }
}

/// Step the engine by `dt_ms` milliseconds.
///
/// Returns 1 if the engine requests exit (ExitMode/WindowClose), otherwise 0.
#[no_mangle]
pub unsafe extern "C" fn rfvp_ios_step(handle: *mut c_void, dt_ms: u32) -> i32 {
    if handle.is_null() {
        return 1;
    }
    let inst = &mut *(handle as *mut IosInstance);
    let should_exit = inst.app.host_step_ios(dt_ms);
    if should_exit { 1 } else { 0 }
}

/// Resize the presentation surface.
#[no_mangle]
pub unsafe extern "C" fn rfvp_ios_resize(handle: *mut c_void, surface_width: u32, surface_height: u32) {
    if handle.is_null() {
        return;
    }
    let inst = &mut *(handle as *mut IosInstance);
    inst.app.host_resize(surface_width, surface_height);
}

/// Destroy the iOS host-mode instance.
#[no_mangle]
pub unsafe extern "C" fn rfvp_ios_destroy(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    drop(Box::from_raw(handle as *mut IosInstance));
}

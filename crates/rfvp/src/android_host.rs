//! Android host-driven entry points.
//!
//! These APIs mirror the iOS host-driven mode: the platform UI thread owns the main loop and
//! calls into Rust to step the engine.
//!
//! Coordinate conventions:
//! - Surface sizes are **physical pixels** (ANativeWindow buffer size).
//! - Touch coordinates are **physical pixels**.

#![cfg(target_os = "android")]

use std::ffi::{c_char, c_void, CStr};
use std::ptr::NonNull;
use std::sync::Once;

use anyhow::{Context, Result};

use crate::app::App;
use crate::script::parser::Nls;
use crate::subsystem::anzu_scene::AnzuScene;
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::utils::file::set_base_path;
use crate::boot::{app_config, load_script};

fn cstr_opt(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    // SAFETY: caller provides a valid NUL-terminated UTF-8 string.
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn build_android_app(
    native_window: NonNull<c_void>,
    surface_size_px: (u32, u32),
    native_scale_factor: f64,
    game_dir: Option<String>,
    nls: Option<String>,
) -> Result<Box<App>> {

    let game_root = match game_dir {
        Some(dir) => dir,
        None => {
            anyhow::bail!("game_dir is null or empty");
        }
    };

    let nls = match nls {
        Some(s) => s.parse::<Nls>()?,
        None => {
            log::warn!("NLS not specified, defaulting to ShiftJIS");
            Nls::default()
        },
    };

    set_base_path(&game_root);
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

    builder
        .build_android(native_window, surface_size_px, native_scale_factor)
        .context("build_android failed")
}

// ndk-context must be initialized from the JVM before any audio backend (CPAL/Oboe)
// attempts to attach threads to JNI. We expose a tiny C ABI hook so the Java/JNI
// layer can provide JavaVM* and a GlobalRef(android.content.Context).

static ANDROID_CTX_ONCE: Once = Once::new();

/// Initialize [`ndk_context`] (Android JVM context) for crates that rely on it (e.g. CPAL/Oboe).
///
/// This must be called **once** (idempotent on the Rust side) before creating the engine.
///
/// - `java_vm_ptr`: `JavaVM*` from JNI.
/// - `context_ptr`: a JNI GlobalRef to an `android.content.Context` instance.
#[no_mangle]
pub unsafe extern "C" fn rfvp_android_init_context(java_vm_ptr: *mut c_void, context_ptr: *mut c_void) {
    if java_vm_ptr.is_null() {
        log::error!("rfvp_android_init_context: java_vm_ptr is null");
        return;
    }
    if context_ptr.is_null() {
        log::error!("rfvp_android_init_context: context_ptr is null");
        return;
    }

    ANDROID_CTX_ONCE.call_once(|| {
        // SAFETY: caller passes valid JNI pointers. We guard with `Once` so we don't
        // re-initialize and panic inside ndk-context.
        unsafe {
            ndk_context::initialize_android_context(java_vm_ptr, context_ptr);
        }
        log::info!("rfvp_android_init_context: ndk_context initialized");
    });
}

/// Create an Android host-driven instance.
///
/// - `native_window_ptr`: `ANativeWindow*`.
/// - `surface_width_px`/`surface_height_px`: physical pixels.
/// - `native_scale_factor`: typically `DisplayMetrics.density`.
/// - `game_dir_utf8`: optional UTF-8 game directory.
/// - `nls_utf8`: optional UTF-8 NLS string.
///
/// Returns an opaque handle, or null on failure.
#[no_mangle]
pub unsafe extern "C" fn rfvp_android_create(
    native_window_ptr: *mut c_void,
    surface_width_px: u32,
    surface_height_px: u32,
    native_scale_factor: f64,
    game_dir_utf8: *const c_char,
    nls_utf8: *const c_char,
) -> *mut c_void {
    let Some(win) = NonNull::new(native_window_ptr) else {
        log::error!("rfvp_android_create: native_window_ptr is null");
        return std::ptr::null_mut();
    };

    let game_dir = cstr_opt(game_dir_utf8);
    let nls = cstr_opt(nls_utf8);

    match build_android_app(win, (surface_width_px, surface_height_px), native_scale_factor, game_dir, nls) {
        Ok(app) => Box::into_raw(app) as *mut c_void,
        Err(e) => {
            log::error!("rfvp_android_create: {e:?}");
            std::ptr::null_mut()
        }
    }
}

/// Step one frame.
/// Returns 1 if the engine requested exit, else 0.
#[no_mangle]
pub unsafe extern "C" fn rfvp_android_step(handle: *mut c_void, dt_ms: u32) -> i32 {
    if handle.is_null() {
        return 1;
    }
    let app: &mut App = &mut *(handle as *mut App);
    if app.host_step(dt_ms) {
        1
    } else {
        0
    }
}

/// Resize the surface (physical pixels).
#[no_mangle]
pub unsafe extern "C" fn rfvp_android_resize(handle: *mut c_void, surface_width_px: u32, surface_height_px: u32) {
    if handle.is_null() {
        return;
    }
    let app: &mut App = &mut *(handle as *mut App);
    app.host_resize_px(surface_width_px, surface_height_px);
}

/// Recreate the WGPU surface from a new `ANativeWindow*` (e.g. SurfaceView recreated).
#[no_mangle]
pub unsafe extern "C" fn rfvp_android_set_surface(
    handle: *mut c_void,
    native_window_ptr: *mut c_void,
    surface_width_px: u32,
    surface_height_px: u32,
) {
    if handle.is_null() {
        return;
    }
    let Some(win) = NonNull::new(native_window_ptr) else {
        log::error!("rfvp_android_set_surface: native_window_ptr is null");
        return;
    };
    let app: &mut App = &mut *(handle as *mut App);
    app.host_set_surface_android(win, surface_width_px, surface_height_px);
}

/// Inject a single-finger touch event.
///
/// `phase`:
/// - 0 = began
/// - 1 = moved
/// - 2 = ended
/// - 3 = cancelled
#[no_mangle]
pub unsafe extern "C" fn rfvp_android_touch(handle: *mut c_void, phase: i32, x_px: f64, y_px: f64) {
    if handle.is_null() {
        return;
    }
    let app: &mut App = &mut *(handle as *mut App);
    app.host_touch_android(phase, x_px, y_px);
}

/// Destroy an Android host-driven instance.
#[no_mangle]
pub unsafe extern "C" fn rfvp_android_destroy(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    drop(Box::from_raw(handle as *mut App));
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn android_main(_app: *mut core::ffi::c_void) {
}

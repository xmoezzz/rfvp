// The debug HUD (egui-wgpu) is only supported on desktop targets.
// On other targets we compile a stub module to avoid pulling winit 0.29 + android-activity 0.5.
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub mod hud;

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
#[path = "hud_stub.rs"]
pub mod hud;
pub mod log_ring;
pub mod vm_snapshot;

#[inline]
pub fn enabled() -> bool {
    if !cfg!(any(target_os = "windows", target_os = "linux", target_os = "macos")) {
        return false;
    }
    matches!(std::env::var("FVP_TEST").as_deref(), Ok("1"))
}

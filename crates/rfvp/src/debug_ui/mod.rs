pub mod hud;
pub mod log_ring;
pub mod vm_snapshot;

#[inline]
pub fn enabled() -> bool {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        return false;
    }
    matches!(std::env::var("FVP_TEST").as_deref(), Ok("1"))
}

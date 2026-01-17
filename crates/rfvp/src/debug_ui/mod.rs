pub mod hud;
pub mod log_ring;
pub mod vm_snapshot;

#[inline]
pub fn enabled() -> bool {
    matches!(std::env::var("FVP_TEST").as_deref(), Ok("1"))
}

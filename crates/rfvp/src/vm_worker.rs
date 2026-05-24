#[cfg(all(not(target_arch = "wasm32"), not(target_os = "uefi")))]
mod native;

#[cfg(target_os = "uefi")]
mod uefi;

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "uefi")))]
pub use native::VmWorker;

#[cfg(target_os = "uefi")]
pub use uefi::VmWorker;

#[cfg(target_arch = "wasm32")]
pub use wasm::VmWorker;

#![no_std]
#![cfg_attr(feature = "entrypoint", feature(alloc_error_handler))]

extern crate alloc;

pub mod app;
pub mod audio;
pub mod clock;
pub mod event;
pub mod fs;
pub mod host;
pub mod render;
pub mod status;

#[cfg(feature = "entrypoint")]
mod allocator;
#[cfg(feature = "entrypoint")]
mod entry;

pub use app::WiiUApp;
pub use audio::WiiUAudio;
pub use clock::WiiUClock;
pub use event::{WiiUEventQueue, WiiUInput};
pub use fs::{WiiUFile, WiiUFileSystem};
pub use host::WiiUHost;
pub use render::WiiURenderer;
pub use status::{wiiu_status_to_rfvp_error, WiiUStatus};

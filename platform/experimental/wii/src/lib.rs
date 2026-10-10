#![cfg_attr(feature = "entrypoint", no_std)]
#![cfg_attr(feature = "entrypoint", feature(alloc_error_handler))]

extern crate alloc;

pub mod app;
pub mod audio;
pub mod clock;
pub mod event;
pub mod fs;
pub mod host;
pub mod raw;
pub mod render;
pub mod status;
pub mod viewport;

#[cfg(feature = "entrypoint")]
mod allocator;
#[cfg(feature = "entrypoint")]
mod entry;

pub use app::WiiApp;
pub use audio::WiiAudio;
pub use clock::WiiClock;
pub use event::WiiEventQueue;
pub use fs::{WiiFile, WiiFileSystem};
pub use host::WiiHost;
pub use raw::{RawWiiHost, RawWiiLogFn};
pub use render::WiiRenderer;
pub use status::{wii_status_to_rfvp_error, WiiStatus};
pub use viewport::WiiViewport;

#![cfg_attr(target_os = "psp", no_std)]

extern crate alloc;

pub mod app;
pub mod audio;
pub mod clock;
pub mod event;
pub mod fs;
pub mod host;
#[cfg(target_os = "psp")]
pub mod platform;
pub mod raw;
pub mod render;
pub mod status;
pub mod viewport;

pub use app::PspApp;
pub use audio::PspAudio;
pub use clock::PspClock;
pub use event::PspEventQueue;
pub use fs::{PspFile, PspFileSystem};
pub use host::PspHost;
pub use raw::{RawPspHost, RawPspLogFn};
pub use render::PspRenderer;
pub use status::{psp_status_to_rfvp_error, PspStatus};
pub use viewport::PspViewport;

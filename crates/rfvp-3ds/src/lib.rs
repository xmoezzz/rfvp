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

pub use app::ThreeDsApp;
pub use audio::ThreeDsAudio;
pub use clock::ThreeDsClock;
pub use event::ThreeDsEventQueue;
pub use fs::{ThreeDsFile, ThreeDsFileSystem};
pub use host::ThreeDsHost;
pub use raw::{RawThreeDsHost, RawThreeDsLogFn};
pub use render::ThreeDsRenderer;
pub use status::{three_ds_status_to_rfvp_error, ThreeDsStatus};
pub use viewport::ThreeDsViewport;

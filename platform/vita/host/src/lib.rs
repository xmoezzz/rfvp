#![no_std]

extern crate alloc;

#[cfg(feature = "global-allocator")]
#[path = "alloc.rs"]
pub mod allocator;

pub mod app;
pub mod audio;
pub mod clock;
pub mod downscale;
pub mod event;
pub mod fs;
pub mod host;
pub mod raw;
pub mod render;
pub mod status;
pub mod texture_policy;
pub mod thumbnail;

pub use app::PsvApp;
pub use audio::PsvAudio;
pub use clock::PsvClock;
pub use event::PsvEventQueue;
pub use fs::{PsvFile, PsvFileSystem};
pub use host::PsvHost;
pub use raw::{RawPsvHost, RawPsvLogFn};
pub use render::PsvRenderer;
pub use status::{psv_status_to_rfvp_error, PsvStatus};

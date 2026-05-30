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

pub use app::PS3App;
pub use audio::PS3Audio;
pub use clock::PS3Clock;
pub use event::{PS3EventQueue, PS3Input};
pub use fs::{PS3File, PS3FileSystem};
pub use host::PS3Host;
pub use render::PS3Renderer;
pub use status::{ps3_status_to_rfvp_error, PS3Status};

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

pub use app::Ps2App;
pub use audio::Ps2Audio;
pub use clock::Ps2Clock;
pub use event::Ps2EventQueue;
pub use fs::{Ps2File, Ps2FileSystem};
pub use host::Ps2Host;
pub use raw::{RawPs2Host, RawPs2LogFn};
pub use render::Ps2Renderer;
pub use status::{ps2_status_to_rfvp_error, Ps2Status};
pub use viewport::Ps2Viewport;

#![no_std]

extern crate alloc;

pub mod app;
pub mod audio;
pub mod clock;
pub mod event;
pub mod fs;
pub mod host;
pub mod render;
pub mod status;

/// Turn a failed system call into `RfvpError::Backend`, logging which call it was: without
/// this a failure on the console leaves no clue.
pub(crate) fn backend_error<E: core::fmt::Debug>(
    what: &'static str,
) -> impl FnOnce(E) -> rfvp::host_api::RfvpError {
    move |err| {
        log::error!("{what} failed: {err:?}");
        rfvp::host_api::RfvpError::Backend
    }
}

pub use app::HorizonApp;
pub use audio::HorizonAudio;
pub use clock::HorizonClock;
pub use event::{HorizonEventQueue, HorizonInput};
pub use fs::{HorizonFile, HorizonFileSystem};
pub use host::HorizonHost;
pub use render::HorizonRenderer;
pub use status::horizon_status_to_result_code;

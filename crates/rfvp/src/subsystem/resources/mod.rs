#[cfg(feature = "no_std")]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
pub mod color_manager;
pub mod flag_manager;
pub mod gaiji_manager;
pub mod graph_buff;
pub mod history_manager;
pub mod input_manager;
pub mod motion_manager;
pub mod parts_manager;
pub mod prim;
#[cfg(not(feature = "no_std"))]
pub mod save_manager;
#[cfg(feature = "no_std")]
#[path = "save_manager_host.rs"]
pub mod save_manager;
pub mod text_manager;
pub mod texture;
pub mod thread_manager;
pub mod thread_wrapper;
pub mod time;
pub mod timer_manager;
#[cfg(not(feature = "no_std"))]
pub mod vfs;
#[cfg(feature = "no_std")]
#[path = "vfs_host.rs"]
pub mod vfs;
#[cfg(all(
    not(feature = "no_std"),
    not(target_os = "uefi"),
    not(target_arch = "wasm32"),
    feature = "native-video",
    feature = "audio"
))]
pub mod videoplayer;
#[cfg(all(
    any(
        feature = "no_std",
        feature = "native-video",
        feature = "uefi-native-video"
    ),
    any(
        feature = "no_std",
        target_os = "uefi",
        target_arch = "wasm32",
        all(not(target_os = "uefi"), not(feature = "audio"))
    )
))]
#[cfg(not(feature = "no_std"))]
#[path = "videoplayer_wasm.rs"]
pub mod videoplayer;
#[cfg(feature = "no_std")]
#[path = "videoplayer_host.rs"]
pub mod videoplayer;
#[cfg(all(
    not(feature = "no_std"),
    not(any(feature = "native-video", feature = "uefi-native-video"))
))]
#[path = "videoplayer_stub.rs"]
pub mod videoplayer;
pub mod window;

pub mod color_manager;
pub mod flag_manager;
pub mod gaiji_manager;
pub mod graph_buff;
pub mod history_manager;
pub mod input_manager;
pub mod motion_manager;
pub mod parts_manager;
pub mod prim;
pub mod save_manager;
pub mod text_manager;
pub mod texture;
pub mod thread_manager;
pub mod thread_wrapper;
pub mod time;
pub mod timer_manager;
pub mod vfs;
#[cfg(all(
    not(target_os = "uefi"),
    not(target_arch = "wasm32"),
    feature = "native-video",
    feature = "audio"
))]
pub mod videoplayer;
#[cfg(all(
    any(feature = "native-video", feature = "uefi-native-video"),
    any(
        target_os = "uefi",
        target_arch = "wasm32",
        all(not(target_os = "uefi"), not(feature = "audio"))
    )
))]
#[path = "videoplayer_wasm.rs"]
pub mod videoplayer;
#[cfg(not(any(feature = "native-video", feature = "uefi-native-video")))]
#[path = "videoplayer_stub.rs"]
pub mod videoplayer;
pub mod window;

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
pub mod save_manager;
pub mod text_manager;
pub mod texture;
pub mod thread_manager;
pub mod thread_wrapper;
pub mod time;
pub mod timer_manager;
pub mod vfs;
pub mod videoplayer;
pub mod window;

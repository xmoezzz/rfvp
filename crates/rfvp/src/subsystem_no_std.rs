#[path = "subsystem/resources/thread_manager.rs"]
pub mod thread_manager_impl;

pub mod resources {
    pub use super::thread_manager_impl as thread_manager;
}

#[cfg(feature = "no_std")]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
#[derive(Debug, Copy, Clone, Default)]
pub struct GameState {}

impl GameState {
    pub fn test(&self) -> bool {
        true
    }
}

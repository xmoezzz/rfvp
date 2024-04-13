pub mod color;
pub mod material;
pub mod maths;
pub mod tiles;
pub mod ui;
pub mod syscalls;

/// Struct to add to any entity to 'hide' it during rendering
pub struct Hide;

pub(crate) struct HidePropagated;

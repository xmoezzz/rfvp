//! Input-related result definitions

use crate::rc;

/// Result Submodule ID for the parent module
pub const RESULT_SUBMODULE: u32 = 800;

result_define_subgroup!(rc::RESULT_MODULE, RESULT_SUBMODULE => {
    InvalidControllerId: 1,
    InvalidTouchIndex: 2
});

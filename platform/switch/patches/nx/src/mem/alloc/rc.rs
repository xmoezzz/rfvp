//! Allocation-specific result definitions

use crate::rc;

/// Result Submodule ID for the parent module
pub const RESULT_SUBMODULE: u32 = 1000;

result_define_subgroup!(rc::RESULT_MODULE, RESULT_SUBMODULE => {
    OutOfMemory: 1,
    LayoutError: 2
});

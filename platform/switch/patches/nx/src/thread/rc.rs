//! Thread-specific result definitions

use crate::rc;

/// Result Submodule ID for the parent module
pub const RESULT_SUBMODULE: u32 = 900;

result_define_subgroup!(rc::RESULT_MODULE, RESULT_SUBMODULE => {
    InvalidStack: 1,
    InvalidState: 2,
    InvalidPriority: 112
});

//! Util-specific result definitions

use crate::rc;

/// Result Submodule ID for the parent module
pub const RESULT_SUBMODULE: u32 = 300;

result_define_subgroup!(rc::RESULT_MODULE, RESULT_SUBMODULE => {
    InvalidPointer: 1,
    InvalidSize: 2,
    InvalidUtf8Conversion: 3,
    InvalidUtf16Conversion: 4
});

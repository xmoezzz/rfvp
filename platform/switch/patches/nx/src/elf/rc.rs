//! ELF-related result definitions

use crate::rc;

/// Result Submodule ID for the parent module
pub const RESULT_SUBMODULE: u32 = 100;

result_define_subgroup!(rc::RESULT_MODULE, RESULT_SUBMODULE => {
    RelaSizeMismatch: 1,
    InvalidModuleMagic: 2,
    DuplicatedDtEntry: 3,
    MissingDtEntry: 4
});

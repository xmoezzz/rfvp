//! GPU-specific result definitions

use crate::rc;

/// Result Submodule ID for the parent module
pub const RESULT_SUBMODULE: u32 = 500;

result_define_subgroup!(rc::RESULT_MODULE, RESULT_SUBMODULE => {
    NvErrorCodeInvalid: 1,
    NvErrorCodeNotImplemented: 2,
    NvErrorCodeNotSupported: 3,
    NvErrorCodeNotInitialized: 4,
    NvErrorCodeInvalidParameter: 5,
    NvErrorCodeTimeOut: 6,
    NvErrorCodeInsufficientMemory: 7,
    NvErrorCodeReadOnlyAttribute: 8,
    NvErrorCodeInvalidState: 9,
    NvErrorCodeInvalidAddress: 10,
    NvErrorCodeInvalidSize: 11,
    NvErrorCodeInvalidValue: 12,
    NvErrorCodeAlreadyAllocated: 13,
    NvErrorCodeBusy: 14,
    NvErrorCodeResourceError: 15,
    NvErrorCodeCountMismatch: 16,
    NvErrorCodeSharedMemoryTooSmall: 17,
    NvErrorCodeFileOperationFailed: 18,
    NvErrorCodeIoctlFailed: 19
});

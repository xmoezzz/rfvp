//! Binder-specific result definitions

use crate::rc;

/// Result Submodule ID for the parent module
pub const RESULT_SUBMODULE: u32 = 1100;

result_define_subgroup!(rc::RESULT_MODULE, RESULT_SUBMODULE => {
    ErrorCodeInvalid: 1,
    ErrorCodePermissionDenied: 2,
    ErrorCodeNameNotFound: 3,
    ErrorCodeWouldBlock: 4,
    ErrorCodeNoMemory: 5,
    ErrorCodeAlreadyExists: 6,
    ErrorCodeNoInit: 7,
    ErrorCodeBadValue: 8,
    ErrorCodeDeadObject: 9,
    ErrorCodeInvalidOperation: 10,
    ErrorCodeNotEnoughData: 11,
    ErrorCodeUnknownTransaction: 12,
    ErrorCodeBadIndex: 13,
    ErrorCodeTimeOut: 14,
    ErrorCodeFdsNotAllowed: 15,
    ErrorCodeFailedTransaction: 16,
    ErrorCodeBadType: 17
});

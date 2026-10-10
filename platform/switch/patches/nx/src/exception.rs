//! Exception handling support (stubbed for now).

use crate::diag::abort::{AbortLevel, abort};
use crate::result::ResultCode;
use crate::svc;

#[unsafe(no_mangle)]
#[linkage = "weak"]
pub(crate) unsafe extern "C" fn __nx_exception_dispatch(
    _reason: svc::ExceptionType,
    _stack_top: *mut u8,
) -> ! {
    // immediately exit if a crate consumer hasn't definite their own exception handler.
    abort(
        AbortLevel::ProcessExit(),
        ResultCode::new(0x6C01 /* StopProcessingException */),
    );
}

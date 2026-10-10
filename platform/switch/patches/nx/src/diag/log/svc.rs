//! SVC-output logger implementation

use super::*;
use crate::svc;

/// Represents a logger through [`output_debug_string`][`svc::output_debug_string`]
pub struct SvcOutputLogger;

impl Logger for SvcOutputLogger {
    fn new() -> Self {
        Self {}
    }

    #[allow(unused_must_use)]
    fn log(&mut self, metadata: &LogMetadata) {
        let msg = format_plain_string_log_impl(metadata, "SvcOutputLog");
        unsafe { svc::output_debug_string(msg.as_c_str()) };
    }
}

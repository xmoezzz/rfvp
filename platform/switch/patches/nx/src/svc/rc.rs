//! Official SVC-related result definitions

pub const RESULT_MODULE: u32 = 1;

result_define_group!(RESULT_MODULE => {
    InvalidSize: 101,
    InvalidAddress: 102,
    InvalidCurrentMemory: 106,
    InvalidHandle: 114,
    TimedOut: 117,
    Cancelled: 118,
    SessionClosed: 123,
    NotHandled: 124,
    Debug: 128
});

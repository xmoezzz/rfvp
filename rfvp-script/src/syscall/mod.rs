//! Syscall integration points.
//!
//! The original engine uses a syscall descriptor table and calls into engine code.
//! In Rust, we replace that with a runtime registry/dispatcher.

mod registry;

pub use registry::{SyscallId, SyscallRegistry, SyscallRuntime};

//! Bytecode interpreter.
//!
//! Opcode semantics follow the decompiled VM you provided.
//! This module focuses on correctness/clarity first; optimization can come later.

mod opcode;
mod thread;

pub use opcode::Opcode;
pub use thread::{ExecOutcome, Frame, SyscallId, ThreadContext, VmError, VmRuntime};

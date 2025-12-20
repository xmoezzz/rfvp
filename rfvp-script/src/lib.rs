//! rfvp-script
//!
//! A small, self-contained script container/parser + bytecode VM module for rfvp.
//! This crate intentionally does **not** assume a fixed on-disk magic/layout.
//!
//! You provide a [`format::ScriptLayout`] (or implement your own) to locate bytecode,
//! global variables, syscall descriptors, etc.

pub mod format;
pub mod syscall;
pub mod variant;
pub mod vm;

/// Local developer utilities (kept as a module, not a binary).
pub mod test;

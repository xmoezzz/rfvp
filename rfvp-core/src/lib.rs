//! This crate implements the core functionality of shin engine
//!
//! This mostly includes file format parsing, virtual machine, and text layouting.

#![allow(clippy::uninlined_format_args)]

// macro hack
extern crate self as rfvp_core;

// re-export for convenience
pub use rfvp_tasks::create_task_pools;

pub mod format;
pub mod layout;
pub mod rational;
pub mod time;
pub mod vm;

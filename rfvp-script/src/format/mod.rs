//! Script container parsing.
//!
//! The upstream engine's `.hcb` layout is not assumed here.
//! Instead, parsing is driven by a [`ScriptLayout`], which you can:
//! - Construct from reverse-engineered offsets, or
//! - Implement your own parser that yields a [`ParsedScript`].

mod layout;
mod parsed;
mod probe;

pub use layout::{ScriptLayout, SyscallDescLayout};
pub use parsed::{ParsedScript, ScriptHeader};
pub use probe::{probe_bytecode_offset, probe_entry_point};

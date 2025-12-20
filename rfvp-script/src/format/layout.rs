/// A declarative layout for locating fields inside a script container.
///
/// This crate intentionally keeps the layout *minimal* and *extensible*:
/// - If you already know offsets/sizes, fill them here.
/// - If you don't, use `probe_*` helpers and iterate.
#[derive(Clone, Debug, Default)]
pub struct ScriptLayout {
    /// Offset to the bytecode stream (where opcodes begin).
    pub bytecode_off: u32,

    /// Length of the bytecode stream; if `None`, it runs until EOF.
    pub bytecode_len: Option<u32>,

    /// Offset to a UTF-8 title string (optional, engine-specific).
    pub title_off: Option<u32>,
    pub title_len: Option<u32>,

    /// (w, h) offsets for screen size (optional, engine-specific).
    pub screen_w_off: Option<u32>,
    pub screen_h_off: Option<u32>,

    /// Global variable table layout (optional).
    pub globals_off: Option<u32>,
    pub globals_count: Option<u32>,

    /// Syscall descriptor table layout (optional).
    pub syscalls_off: Option<u32>,
    pub syscalls_count: Option<u32>,
    pub syscall_desc: Option<SyscallDescLayout>,
}

#[derive(Clone, Debug)]
pub struct SyscallDescLayout {
    /// Size of each syscall descriptor entry.
    pub stride: u32,

    /// Offset within descriptor to `argc` (u32).
    pub argc_off: u32,

    /// Offset within descriptor to `func_ptr` (u32/usize in original; you won't use it in Rust).
    pub funcptr_off: u32,

    /// Offset within descriptor to `this_offset` (u32) (original uses this->ptr + this_offset).
    pub this_off: u32,
}

impl Default for SyscallDescLayout {
    fn default() -> Self {
        Self { stride: 16, argc_off: 4, funcptr_off: 8, this_off: 12 }
    }
}

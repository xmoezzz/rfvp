use std::collections::HashMap;

use anyhow::{anyhow, Result};

use crate::variant::Variant;

pub type SyscallId = u16;

/// The runtime interface the VM expects.
///
/// You can implement this on your `Scene`/`GameData`/`World` type.
pub trait SyscallRuntime {
    /// Query how many arguments a syscall expects.
    fn syscall_argc(&self, id: SyscallId) -> Option<usize>;

    /// Dispatch syscall.
    ///
    /// `args` are the *last* `argc` elements on the VM operand stack (in call order).
    fn syscall_call(&mut self, id: SyscallId, args: &[Variant]) -> Result<Variant>;
}

/// A simple in-crate registry (useful for tests and prototyping).
///
/// In the full engine, you may prefer a hand-written `match id { ... }` for performance.
#[derive(Default)]
pub struct SyscallRegistry {
    argc: HashMap<SyscallId, usize>,
    fns: HashMap<SyscallId, Box<dyn Fn(&mut dyn std::any::Any, &[Variant]) -> Result<Variant> + Send + Sync>>,
}

impl SyscallRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(&mut self, id: SyscallId, argc: usize, f: F)
    where
        F: Fn(&mut dyn std::any::Any, &[Variant]) -> Result<Variant> + Send + Sync + 'static,
    {
        self.argc.insert(id, argc);
        self.fns.insert(id, Box::new(f));
    }

    pub fn argc(&self, id: SyscallId) -> Option<usize> {
        self.argc.get(&id).copied()
    }

    pub fn call(&self, host: &mut dyn std::any::Any, id: SyscallId, args: &[Variant]) -> Result<Variant> {
        let f = self.fns.get(&id).ok_or_else(|| anyhow!("unknown syscall id={id}"))?;
        f(host, args)
    }
}

//! Developer-facing utilities for the rfvp script VM.
//!
//! This is intentionally a module (not `src/bin/...`) so it can be reused
//! from unit tests and other workspace crates.

use std::{fs, path::Path, sync::Arc};

use anyhow::{Context, Result};

use crate::{
    format::{probe_bytecode_offset, ScriptLayout},
    variant::Variant,
    vm::{ExecOutcome, ThreadContext, VmRuntime},
};

/// A tiny "host" for running scripts in isolation.
/// Replace it with your real Scene/GameData integration.
pub struct NullRuntime;

impl VmRuntime for NullRuntime {
    fn syscall_argc(&self, _id: u16) -> Option<usize> {
        Some(0)
    }

    fn syscall_call(&mut self, _id: u16, _args: &[Variant]) -> Result<Variant> {
        Ok(Variant::Nil)
    }
}

/// Load a container file, probe a bytecode offset, and run the VM for a bounded number of steps.
pub fn run_container_for_smoke(path: impl AsRef<Path>, max_steps: usize) -> Result<ExecOutcome> {
    let bytes = fs::read(&path).with_context(|| format!("read {:?}", path.as_ref()))?;
    let bytes: Arc<[u8]> = bytes.into();

    let bytecode_off = probe_bytecode_offset(&bytes).context("failed to probe bytecode_off; provide a ScriptLayout explicitly")?;
    let layout = ScriptLayout { bytecode_off, bytecode_len: None, ..Default::default() };

    let parsed = crate::format::ParsedScript::parse(bytes.clone(), &layout)?;
    let mut globals: Vec<Variant> = Vec::new();

    let mut vm = ThreadContext::new(parsed.bytecode.clone(), 0);
    let mut rt = NullRuntime;

    vm.run(&mut rt, &mut globals, max_steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_no_file_ok() {
        let p = std::path::Path::new("testcase/test.hcb");
        if !p.exists() {
            // repo user will provide this file; keep test green for CI.
            return;
        }
        let out = run_container_for_smoke(p, 10_000);
        // We only assert it doesn't crash; semantics will be validated once layout/syscalls are implemented.
        assert!(out.is_ok());
    }
}

# ADR-0008: Translation patching via offline HCB repacking instead of runtime engine probing

- Status: Accepted
- Date: 2026-10-25

## Context

Visual novel localization and fan translations modify dialogue scripts, menu text, system messages, and
occasionally graphical assets and fonts. In the original FVP engine, game bytecode and localized text are
stored in compiled `.hcb` files, often with custom or varied text encodings (Shift-JIS, GBK, UTF-8).

When integrating translation patches into game engines, two primary architectural approaches exist:

1. **Engine-Level Dynamic Patching and Probing:**
   - The engine attempts to detect and load loose translation files, text replacement tables, or loose
     script overrides at runtime.
   - The Virtual File System (VFS) must constantly probe directories to check whether an asset exists as
     a loose file before reading it from `.bin` archives.
   - Text strings might be intercepted or dynamically injected into the script VM during execution.

2. **Offline Asset Repacking:**
   - Translated scripts and assets are pre-processed offline and repackaged back into standard engine
     formats: compiled `.hcb` bytecode and standard `.bin` package archives (or clean standalone `.hcb`
     scripts placed in the game directory).
   - The runtime engine remains completely agnostic to whether a script is original or translated; it
     simply executes the bytecode provided.

On resource-constrained and console platforms (particularly PlayStation Vita, Nintendo Switch, and
embedded devices), dynamic filesystem probing introduces severe problems:
- **Filesystem latency & syscall overhead:** Checking the filesystem for loose overrides on every asset
  access results in frequent, expensive disk/SD card seek operations and OS syscalls.
- **Diagnostics & emulator clutter:** Continuous probe failures generate excess logging and noise in
  console emulators (e.g., Vita3K, Ryujinx) and hardware debuggers.
- **Port portability:** Different target filesystems have inconsistent case sensitivity, path separators,
  and directory traversal semantics.
- **Engine complexity:** Building translation-specific injection layers inside the VM bloats the `no_std`
  engine core and introduces subtle behavioral bugs.

## Decision

1. **No Runtime Translation Probing or Dynamic Hooking in the Engine:**
   - rfvp will **not** implement engine-level runtime translation probing, dynamic string hot-patching,
     or loose-asset override hooks.
   - The engine core remains lean, deterministic, and focused solely on executing compliant FVP bytecode
     and standard archives.

2. **Translation Patches Repackaged Back into HCB:**
   - Translation patches must be packaged offline into standard `.hcb` bytecode files using rfvp's
     existing toolchain (`crates/hcb2lua_decompiler`, `crates/lua2hcb_compiler`, and related rebuilder
     crates).
   - Patched scripts can be selected via the standard `config.toml` mechanism (specifying `hcb = "..."`
     and the target encoding `nls = "gbk"` or `"utf8"`), or by replacing the root script file.

3. **Repackaging Scripts Maintained in `translation/`:**
   - Automation scripts, conversion utilities, and packaging workflows for translation patches should
     be submitted and maintained in a dedicated top-level `translation/` directory.
   - This keeps tooling close to contributors and translators without polluting the runtime engine
     crates with one-off patching logic.

## Consequences

- **Clean, uniform cross-platform behavior:** All target platforms (desktop, mobile, Switch, PS Vita,
  WASM) execute translated games identically using the standard VFS and VM pipeline.
- **Elimination of I/O interference:** Eliminates expensive loose-file filesystem probes, disk thrashing,
  and emulator debug log spam during gameplay.
- **Decoupled tooling:** Translators benefit from clear, reproducible offline build pipelines without
  requiring new engine releases or engine-level code changes for each translated title.

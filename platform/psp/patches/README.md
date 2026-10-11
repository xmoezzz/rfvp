# Patched dependencies

## psp

`psp/` is the [psp](https://crates.io/crates/psp) crate 0.3.12 (rust-psp, MIT), used through
`[patch]` in the workspace `Cargo.toml`. Changes:

- `src/alloc_impl.rs`: the global allocator (`SystemAlloc`, one kernel memory block per
  allocation) is behind a new feature, `external-allocator`, off by default. rfvp's PSP host
  turns it on and installs its own heap: one block reserved at startup, managed by `talc`. With
  `SystemAlloc`, loading the bitmap font alone made tens of thousands of kernel calls, each
  allocation took at least 256 bytes and a kernel object, and the game never got past startup.
- `Cargo.toml`: built as an rlib only. The `staticlib` crate type (for C programs) needs a global
  allocator of its own, which the crate no longer has with `external-allocator`.
- `src/panic.rs`: the C `free` the crate exports ignores a null pointer, as C requires. The C
  code built into rfvp's PSP host (stb_vorbis) frees null pointers when it closes a decoder, and
  the original read a size from before address 0 and corrupted the heap.

To move to a newer psp, copy the release here and apply the change again (search for
"rfvp patch").

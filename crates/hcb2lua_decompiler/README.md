# hcb2lua

A standalone **first-pass** HCB bytecode decompiler that emits Lua.

## Build

```bash
cargo build --release
```

## Run

```bash
./target/release/hcb2lua_decompiler --input /path/to/script.hcb --output script.lua --lang sjis
```

## Notes

- The generated Lua targets **Lua 5.3+** (uses bitwise operators for `BitTest`).
- `Call/Syscall` store their return value in `__ret`; `PushReturn` pushes `__ret` onto the value stack.
- Syscalls are emitted as `__syscall("Name", ...)` stubs.

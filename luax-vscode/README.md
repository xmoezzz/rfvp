# luax-vscode

VS Code extension for Luax projects.

This extension is designed for the Rust workspace layout described in the conversation:

- `crates/luax-lsp`
- `crates/lua2hcb_compiler`
- `crates/rfvp`

A Luax project is considered valid only when a `.luax` file has exactly one sibling YAML project file in the same directory, and that YAML file contains the required project metadata fields.

## What the extension does

1. Registers the `luax` language for `.luax` files.
2. Provides syntax highlighting and snippets aligned with the current decompiler style.
3. Starts `luax-lsp` as a standard LSP server.
4. Validates the current Luax project layout.
5. Compiles the current Luax project back to `.hcb` by invoking `lua2hcb_compiler`.

## Commands

- `Luax: Restart Language Server`
- `Luax: Open Project YAML`
- `Luax: Validate Current Project`
- `Luax: Compile Current Project to HCB`

## Project validation rules

For the current `.luax` file, the extension requires exactly one sibling `.yaml` or `.yml` file.

The YAML file must contain these required top level fields:

- `nls`
- `custom_syscall_count`
- `game_mode`
- `game_mode_reserved`
- `game_title`
- `syscall_count`
- `syscalls`

The extension also validates:

- `syscall_count` matches the number of entries inside `syscalls`
- syscall ids are contiguous and start at `0`
- every syscall entry has `name` and `args`

If any of these checks fail, the project is treated as invalid and diagnostics are shown on the `.luax` file and the YAML file.

## How the language server is resolved

The extension looks for `luax-lsp` in this order:

1. `luax.server.path` from VS Code settings
2. `LUAX_LSP_BIN` from the environment
3. a bundled binary under `server/`
4. `cargo run -q -p luax-lsp --` in an open Rust workspace that contains `crates/luax-lsp`

## How the compiler is resolved

The extension looks for `lua2hcb_compiler` in this order:

1. `luax.compiler.path` from VS Code settings
2. `LUAX_COMPILER_BIN` from the environment
3. `cargo run -q -p lua2hcb_compiler --` in an open Rust workspace that contains `crates/lua2hcb_compiler`

When compiling, the extension invokes the compiler in the CLI form you described:

```text
--meta <project-yaml> --lua <script.luax> -o <output.hcb>
```

## Recommended setup

If you already built the Rust binaries yourself, put this in VS Code settings:

```json
{
  "luax.server.path": "/absolute/path/to/luax-lsp",
  "luax.compiler.path": "/absolute/path/to/lua2hcb"
}
```

If you work directly in the Rust workspace, opening the workspace root is enough, because the extension can fall back to `cargo run -p ...`.

## Development

```bash
npm install
```

Then press `F5` in VS Code to run the extension host.

## Notes

- Syscall completion, hover, and signature help should still come from `luax-lsp`.
- This extension validates YAML structure, but it does not try to infer undocumented syscall semantics by itself.

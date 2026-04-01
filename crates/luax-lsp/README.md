# luax-lsp

`luax-lsp` is a standalone Language Server Protocol implementation for the Luax scripts produced by the current decompiler workflow.

This revision is based on the uploaded decompiler output and project YAML shape, not on an invented Lua dialect. In particular, the server now models the real top-level declaration style `global ...` and `volatile global ...`, and it expects a project YAML file in the same directory as each script.

## Implemented Luax syntax model

The server currently parses these constructs:

### Top-level declarations

```luax
global g0, g1, g2
volatile global vg0, vg1

function main(a0, a1, a2)
  local __ret = nil
  local __pc = 0
  while true do
    if __pc == 0 then
      __ret = TextOutSize(0, 4, nil)
      __ret = TextColor(0, 10, 11, 100)
      return
    end
  end
end
```

### Statements

Supported statements:

- `local name = expr`
- `global name, name2, ...`
- `global name = expr`
- `volatile global name, name2, ...`
- `volatile global name = expr`
- assignment, including member assignment
- `function ... end`
- `if ... then ... elseif ... else ... end`
- `while ... do ... end`
- `repeat ... until ...`
- numeric `for`
- `return`
- `break`
- expression statements

### Expressions

Supported expressions:

- identifiers
- `nil`, `true`, `false`
- strings and numbers
- unary `-` and `not`
- binary operators: `or`, `and`, `==`, `~=`, `<`, `<=`, `>`, `>=`, `+`, `-`, `*`, `/`, `%`
- function calls, including direct syscall-style calls such as `TextColor(...)`
- member access with `.`
- index access with `[]`
- table literals like `{ foo = 1, bar = 2 }`
- anonymous function expressions

## Required project YAML

For every script file, `luax-lsp` looks for exactly one `.yaml` or `.yml` file in the same directory.

If no YAML exists, if more than one YAML exists, or if the YAML does not match the required project-file schema, the LSP reports an error diagnostic.

The required schema is:

```yaml
nls: ShiftJIS
custom_syscall_count: 0
game_mode: 7
game_mode_reserved: 0
game_title: アストラエアの白き永遠 ver1.1
syscall_count: 148
syscalls:
  0:
    args: 2
    name: AudioLoad
  1:
    args: 2
    name: AudioPlay
```

Required fields:

- `nls`
- `custom_syscall_count`
- `game_mode`
- `game_mode_reserved`
- `game_title`
- `syscall_count`
- `syscalls`

The loader also validates that:

- `game_title` is not empty
- `syscall_count` equals the number of syscall entries
- syscall ids are contiguous from `0` to `syscall_count - 1`

## Implemented LSP features

The current server implements these features:

- diagnostics for syntax errors, unresolved symbols, and invalid or missing project YAML
- keyword, snippet, symbol, and syscall completion
- hover on declarations and resolved references
- go to definition
- find references in the current document
- rename in the current document
- document symbols
- workspace symbols
- semantic tokens
- signature help
- workspace scan for `.luax` and `.lua` files
- direct syscall metadata ingestion from the project YAML

Syscalls from the YAML are available to the LSP as named callable symbols. Their names are completed directly, and signature help uses placeholder argument names such as `arg1`, `arg2`, and so on, derived from the YAML `args` count.

## Current limitations

These limits are real in the current codebase:

- there is no validated build result in this environment because the Rust toolchain is not installed here, so I could not run `cargo check`
- rename and references are currently document-scoped, not full-workspace semantic refactoring
- cross-file module resolution is lightweight
- formatting, code actions, inlay hints, code lens, and call hierarchy are not implemented yet
- syscall documentation is not yet comprehensively scraped from the RFVP engine source; currently the YAML provides the canonical name and arity data used by completion and signature help

## Run

```bash
cargo run
```

## VS Code client snippet

```json
{
  "name": "luax-lsp",
  "command": "cargo",
  "args": ["run", "--quiet", "--manifest-path", "/absolute/path/to/luax-lsp/Cargo.toml"],
  "filetypes": ["luax", "lua"]
}
```

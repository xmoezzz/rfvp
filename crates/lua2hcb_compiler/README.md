# lua2hcb_compiler

This tool compiles a Lua **5.3-looking** decompiler output back into **HCB** bytecode.

It targets the practical subset used by the current rfvp/hcb decompiler pipeline:

- Top-level `function NAME(...) ... end`
- Structured control flow: `if/elseif/else/end`, `while/do/end`, `break`
- Function calls and syscalls
- No closures/upvalues, no vararg, no multi-return

In addition to structured `if/while`, the compiler also supports the decompiler's
`__pc`-dispatcher form:

```lua
local __pc = 0
while true do
  if __pc == 0 then
    ...
    if S0 == 0 then
      __pc = 2
    else
      __pc = 1
    end
  elseif __pc == 1 then
    ...
    __pc = 2
  else
    return
  end
end
```

Pure local declarations such as `local S0, S1, S2` are accepted and ignored.

Important IR convention

This compiler assumes the input Lua is *decompiler IR style* (stack-machine reconstruction), e.g.

- `S0 = 123` means “push 123”
- `S1 = (S1 == S2)` means “compare top two values and push the result”
- `__ret = Foo(...)` means “call Foo; return value is available via `__ret` / `push_return`”

The `if`/`while` conditions are treated as the same style used previously in the CFG/state-machine form:

- `if S0 ~= 0 then` / `while S0 ~= 0 do`: `jz` is used to branch on zero
- `if S0 == 0 then` / `while S0 == 0 do`: layout is inverted (still uses `jz`)

## Strings / encoding

All C-strings in HCB are stored as:

- 1-byte length **including** the trailing NUL
- bytes of the string followed by `\0`

This applies to:

- `push_string` immediates inside code
- `game_title` and syscall names inside `sysdesc`

Encoding is controlled by `nls` in YAML (`ShiftJIS`, `UTF-8`, `GB18030`).

## YAML meta format

Minimal example (your current format):

```yaml
nls: ShiftJIS
sys_desc_offset: 0          # ignored (recomputed)
entry_point: 0              # ignored (recomputed from function entry_point())
non_volatile_global_count: 1915
volatile_global_count: 1990
custom_syscall_count: 0
game_mode: 7
game_title: "..."
syscall_count: 148          # optional; validated if present
syscalls:
  0:   { args: 2,  name: AudioLoad }
  1:   { args: 2,  name: AudioPlay }
  ...
  147: { args: 1,  name: WindowMode }
```

`syscalls` may also be provided as a YAML list (legacy), but the map form is recommended.

## The difference from Lua 5.3

| Area                         | Lua 5.3 (full language)                                                                                | This project’s supported subset                                                                                          | Practical implication / reminder                                                                               |                                                                                    |
| ---------------------------- | ------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Intended scope               | General-purpose scripting language                                                                     | Deterministic “decompiler-friendly” subset for round-tripping to HCB bytecode                                                            | Treat this as a *compilation format*, not a general Lua target                                                 |                                                                                    |
| Entry point                  | No fixed name; host decides what to call                                                               | `function entry_point()` is required and is used to compute the HCB entry address                                                        | Renaming/omitting `entry_point` breaks linkage                                                                 |                                                                                    |
| Program structure            | Arbitrary chunk with any order of statements                                                           | Multiple `function f_xxxxxxxx(...) ... end` allowed in any order; forward calls allowed                                                  | Function definitions and uses do not need to be ordered                                                        |                                                                                    |
| Lexical / comments           | Full Lua comments (`--`, `--[[...]]`), long strings                                                    | Same (as tolerated by the parser)                                                                                                        | Prefer simple line comments; avoid exotic long-bracket nesting                                                 |                                                                                    |
| Identifiers                  | Any valid Lua identifier                                                                               | Expected conventions: `aN` args, `lN` locals (optional), `S0..` temporaries, `G[i]` globals, `LT[idx][key]` / `GT[idx][key]` tables      | Using arbitrary variable names may become unsupported                                                          |                                                                                    |
| Types / values               | `nil`, boolean, number (int/float), string, table, function, userdata, thread                          | `nil`, boolean, number, string; tables only via explicit VM-style access; functions only as named top-level functions                    | No general table constructors, userdata, threads                                                               |                                                                                    |
| Truthiness                   | `false` and `nil` are falsey; everything else truthy (including `0`, `""`)                             | Control flow is primarily compiled from **0/1-style conditions** (e.g., `if S0 == 0 then ...`)                                           | Do not rely on Lua truthiness of non-boolean values; prefer explicit comparisons                               |                                                                                    |
| Expressions (general)        | Full expression grammar, precedence, short-circuit, metamethods                                        | Limited expression shapes that map to known HCB ops (arithmetic, comparisons, simple boolean composition patterns)                       | Complex expressions may fail to compile; keep expressions simple and explicit                                  |                                                                                    |
| Arithmetic operators         | `+ - * / // % ^`                                                                                       | `+ - * / %` supported; `//` (integer division) and `^` only if explicitly mapped in the bytecode set you use                             | Avoid `//` unless you confirmed the opcode mapping exists                                                      |                                                                                    |
| Bitwise operators            | `&                                                                                                     | ~ << >>`and unary`~`                                                                                                                     | Only patterns that map to the VM’s bit ops (commonly `bit_test`-style patterns)                                | Do not write general bitwise arithmetic unless the compiler explicitly supports it |
| Concatenation                | `..`                                                                                                   | Only if mapped; otherwise unsupported                                                                                                    | Prefer precomputed strings / avoid dynamic concatenation                                                       |                                                                                    |
| Length operator              | `#`                                                                                                    | Generally unsupported unless mapped                                                                                                      | Avoid `#t` and `#s` unless you confirmed support                                                               |                                                                                    |
| Relational operators         | `== ~= < <= > >=`                                                                                      | Supported when operands are in supported forms (`Sx`, `aN`, `G[i]`, literals)                                                            | Keep operands “simple” and VM-addressable                                                                      |                                                                                    |
| Boolean operators            | `and`, `or`, `not` with short-circuit semantics                                                        | `not` may be supported if mapped; `and/or` only in restricted decompiler-style patterns (not general short-circuit truthiness)           | Avoid idiomatic Lua `a and b or c` constructs                                                                  |                                                                                    |
| Assignment                   | Multiple assignment, destructuring, local init                                                         | Single-target assignments are expected; multi-assign may be partially supported only for `local S0,S1,...` declarations                  | Prefer one assignment per line                                                                                 |                                                                                    |
| Local declarations           | `local x`, `local x = expr`, `local a,b = ...`                                                         | `local S0, S1, ...` and similar declarations are accepted and ignored (no codegen)                                                       | Declarations exist only to keep Lua syntax valid/readable                                                      |                                                                                    |
| Control flow                 | `if/elseif/else/end`, `while`, `repeat/until`, numeric `for`, generic `for`, `goto/::label::`, `break` | `if/elseif/else`, `while`, `break` supported; `repeat/until`, `for`, `goto` not supported                                                | Keep loops to `while`; avoid `for` and `repeat`                                                                |                                                                                    |
| Return statements            | `return`, `return a,b,c` (multi-return), tail-call behaviors                                           | `return` and **single-value** `return S0` supported (as VM `ret/retv`)                                                                   | Do not use multi-return; return at most one value                                                              |                                                                                    |
| Functions / closures         | `function` statements/expressions, closures, upvalues, recursion, methods (`:`), vararg (`...`)        | **No closures/upvalues**; **no vararg**; named functions only; recursion is okay if VM supports it                                       | Do not define nested functions; do not capture outer locals                                                    |                                                                                    |
| Multiple return values       | Full multiple return semantics (`return a,b`, assignment from multi-return calls)                      | Not supported                                                                                                                            | Every call is treated as producing at most one usable return value                                             |                                                                                    |
| Method call syntax           | `obj:method(a)` sugar for `obj.method(obj,a)`                                                          | Not supported unless emitted in a VM-specific lowered form                                                                               | Use explicit function names / VM syscall patterns instead                                                      |                                                                                    |
| Table constructors           | `{}`, `{a=1}`, `{[k]=v}`, array parts, mixed                                                           | Not supported                                                                                                                            | Tables are manipulated only via explicit VM table ops (e.g., `LT[idx][key] = v`)                               |                                                                                    |
| Metatables / metamethods     | `setmetatable`, operator overloading, `__index`, etc.                                                  | Not supported                                                                                                                            | Operator behavior is VM-defined, not Lua metamethod-driven                                                     |                                                                                    |
| Modules                      | `require`, package loaders, environments                                                               | Not supported                                                                                                                            | No module system; single compilation unit                                                                      |                                                                                    |
| Error handling               | `error`, `assert`, `pcall`, `xpcall`                                                                   | Not supported unless present as explicit syscalls                                                                                        | Do not rely on Lua exception mechanisms                                                                        |                                                                                    |
| Coroutines                   | `coroutine.*`, yielding, resumptions                                                                   | Not supported                                                                                                                            | Even if the engine uses “threads,” they are VM/syscall-level, not Lua coroutines                               |                                                                                    |
| Standard library             | Full Lua 5.3 base/string/table/math/io/os/debug/utf8                                                   | Not supported as Lua-level libraries; functionality expected via syscalls and VM globals                                                 | Use syscalls listed in YAML; do not call `string.*` / `table.*` etc.                                           |                                                                                    |
| Special decompiler artifacts | Not part of Lua spec                                                                                   | `__ret` (return register marker), optional `__pc` dispatcher form, `S0..` temporaries, `G[i]` globals                                    | These are *compiler contract* elements, not standard Lua                                                       |                                                                                    |
| Strings encoding             | Lua source is bytes; conventionally UTF-8, but not required                                            | Encoding controlled by YAML `nls` (e.g., ShiftJIS); all emitted C-strings are NUL-terminated and length-prefixed with `u8` including NUL | Keep string literals compatible with the chosen encoding; avoid characters not representable in that code page |                                                                                    |
| Determinism                  | Lua execution depends on runtime semantics, metamethods, libs                                          | Compilation must be deterministic and match the VM opcode model                                                                          | Avoid any construct whose meaning depends on Lua runtime facilities                                            |                                                                                    |


## Build

```bash
cargo build --release
```

## Usage

```bash
./target/release/lua2hcb --meta meta.yml --lua script.lua -o script.hcb
```

The tool always uses `function entry_point()` as the entry.

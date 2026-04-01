# lua2hcb_compiler

`lua2hcb_compiler` compiles the project's constrained Lua-like source format back into HCB bytecode.

This is **not** full Lua 5.3. It is a fixed compilation language designed for round-tripping with the HCB decompiler and for hand-written scripts that stay inside the supported subset.

## Design rules

- The source language is a **compiler contract**, not a general scripting language.
- The compiler only accepts the supported syntax described below.
- The only valid entry function is `main`.
- Global counts are **derived from source declarations**. They are **not** read from YAML.
- YAML is treated as **project configuration**, not as a place to carry source semantics.

## File structure

A source file consists of:

1. Optional blank lines and comments
2. Optional top-level global declarations
3. Top-level function definitions

Only these top-level forms are accepted before the first function:

```lua
global some_name
volatile some_other_name
```

Any other top-level statement is rejected.

## Entry point

The compiler requires:

```lua
function main()
    ...
end
```

`entry_point` is not accepted.

## Top-level global declarations

Two declaration forms are supported:

```lua
global flag
volatile current_voice
```

Semantics:

- `global name`
  - declares one **non-volatile** global
- `volatile name`
  - declares one **volatile** global

Rules:

- Declarations must appear before the first function.
- Duplicate names are rejected.
- Initializers are not supported.
- Declaration order defines the generated global indices.

That means:

```lua
global a
volatile b
global c
```

will allocate:

- non-volatile globals: `a`, `c`
- volatile globals: `b`

and the compiler will derive both counts automatically.

## Identifiers and special names

The compiler recognizes these naming conventions:

- `main`
  - required entry function
- `f_xxxxxxxx` or other top-level function names
  - callable user functions
- `aN`
  - argument slots, for example `a0`, `a1`
- `lN`
  - local frame slots, for example `l0`, `l1`
- `S0`, `S1`, ...
  - stack-machine temporaries used by the decompiler IR style
- `__ret`
  - return register marker after a function or syscall call
- declared global names
  - names introduced by `global` or `volatile`
- `GT[idx][Sx]`
  - global table access form
- `LT[idx][Sx]`
  - local table access form

## Comments and blank lines

Supported:

```lua
-- line comment
```

Blank lines are ignored.

Do not rely on advanced Lua long-bracket comment edge cases.

## Supported top-level function syntax

Top-level functions use normal Lua-looking syntax:

```lua
function some_func(a0, a1)
    ...
end
```

Also accepted:

```lua
local function some_func(a0, a1)
    ...
end
```

Practical restrictions:

- top-level named functions only
- no closures
- no nested semantic functions
- no upvalues
- no varargs
- no method definitions

## Supported statements

### 1. Simple call statements

Supported:

```lua
Foo()
Foo(a0, a1)
AudioPlay(a0, a1)
```

Resolution order:

- syscall name from YAML
- user function name

### 2. Call into `__ret`

Supported:

```lua
__ret = Foo()
__ret = AudioPlay(a0)
```

This is the canonical way to capture a call result.

### 3. `Sx = ...` assignments

Supported right-hand-side forms are intentionally limited.

#### Literals and direct values

```lua
S0 = nil
S0 = true
S0 = 123
S0 = 1.25
S0 = "abc"
S0 = __ret
S0 = S1
S0 = a0
S0 = l0
S0 = some_global
```

Notes:

- `false` is not currently emitted as a dedicated immediate in the compiler contract.
- string literals use normal double-quoted form
- `some_global` is rewritten to an internal `G[idx]` reference by the compiler

#### Table reads

```lua
S0 = GT[3][S1]
S0 = LT[2][S1]
```

#### Unary operator

```lua
S0 = -S1
```

#### Arithmetic

```lua
S0 = S1 + S2
S0 = S1 - S2
S0 = S1 * S2
S0 = S1 / S2
S0 = S1 % S2
```

#### Comparisons

```lua
S0 = (S1 == S2)
S0 = (S1 ~= S2)
S0 = (S1 > S2)
S0 = (S1 <= S2)
S0 = (S1 < S2)
S0 = (S1 >= S2)
```

#### Bit-test style form

Supported decompiler-style form:

```lua
S0 = (S1 & S2) ~= 0
```

This maps to the VM bit-test opcode. General Lua bitwise expressions are not supported.

#### Restricted boolean composition

Only decompiler-style restricted patterns are supported. This is **not** full Lua short-circuit semantics.

```lua
S0 = S1 and S2 ~= nil
S0 = S1 or S2 ~= nil
```

Use these only when they come from decompiler IR or when you know the exact lowering expected by the compiler.

#### RHS calls

```lua
S0 = Foo(a0)
S0 = AudioState(a0)
```

The compiler lowers the call, then pushes `__ret`.

### 4. Stores out of `Sx` or `__ret`

#### Global stores

```lua
some_global = S0
some_global = __ret
```

#### Frame-slot stores

```lua
a0 = S0
l0 = S1

a0 = __ret
l0 = __ret
```

#### Table stores

```lua
GT[3][S0] = S1
LT[2][S0] = S1
```

## Supported control flow

### `if / elseif / else`

Supported:

```lua
if S0 ~= 0 then
    ...
elseif S1 == 0 then
    ...
else
    ...
end
```

### `while`

Supported:

```lua
while S0 ~= 0 do
    ...
end
```

### `break`

Supported inside `while`.

### `return`

Supported:

```lua
return
return S0
return nil
return true
return 123
```

Multi-return is not supported.

## Supported condition forms

The compiler is designed for decompiler-style conditions.

The supported and expected forms are:

```lua
if S0 then
if S0 ~= 0 then
if S0 == 0 then
if true then
if false then
if nil then

while S0 do
while S0 ~= 0 do
while S0 == 0 do
while true do
while false do
```

Recommended style:

```lua
if S0 ~= 0 then
while S0 ~= 0 do
```

Do not rely on full Lua truthiness rules for arbitrary expressions.

## Local declarations

The compiler accepts decompiler-style local declarations for readability and ignores them when they are pure declarations.

Examples:

```lua
local S0, S1, S2
local l0, l1
```

These are accepted and ignored.

Assignments remain semantic:

```lua
local S0 = __ret
```

This is treated as:

```lua
S0 = __ret
```

## `__pc` dispatcher form

The compiler also supports the decompiler's explicit state-machine style.

Example:

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

This form is supported specifically for round-tripping decompiler output.

## Unsupported syntax

The following are outside the supported language:

- `entry_point`
- top-level executable statements other than `global` and `volatile`
- global initializers
- closures
- nested functions as a language feature
- upvalues
- varargs `...`
- generic `for`
- numeric `for`
- `repeat/until`
- `goto`
- labels
- table constructors
- method syntax `obj:method(...)`
- metatables
- modules
- coroutines
- multiple return values
- general Lua standard library usage as part of the language contract
- arbitrary bitwise expressions
- arbitrary boolean expressions with full Lua short-circuit semantics

## YAML project configuration

YAML is project configuration. It does not define source-level globals.

### Required / meaningful fields

```yaml
nls: ShiftJIS
game_mode: 7
game_mode_reserved: 0
custom_syscall_count: 0
game_title: "..."
syscalls:
  0:   { args: 2, name: AudioLoad }
  1:   { args: 2, name: AudioPlay }
```

### Optional field

```yaml
syscall_count: 148
```

If present, it is used for validation.

### Fields that should not be used anymore

These are not part of the source contract and should not be authored for new projects:

- `sys_desc_offset`
- `entry_point`
- `non_volatile_global_count`
- `volatile_global_count`

The compiler computes what it needs.

### Accepted `syscalls` forms

Preferred form, keyed by syscall id:

```yaml
syscalls:
  0: { args: 2, name: AudioLoad }
  1: { args: 1, name: AudioState }
```

Legacy sequence form is also accepted if your current code still uses it, but the id-keyed mapping is the recommended format.

## Strings and encoding

String encoding is controlled by `nls`:

- `UTF-8`
- `ShiftJIS`
- `GB18030`

HCB C-strings are emitted as:

- 1-byte length including trailing NUL
- bytes
- trailing `\0`

This applies to:

- string immediates in code
- `game_title`
- syscall names in `sysdesc`

## Minimal example

```lua
global chapter
volatile current_voice

function main()
    S0 = 1
    chapter = S0

    __ret = AudioState(a0)
    S1 = __ret

    if S1 ~= 0 then
        return
    else
        __ret = ShowMessage(a0)
        current_voice = __ret
        return
    end
end
```

## Build

```bash
cargo build --release
```

## Usage

```bash
./target/release/lua2hcb --meta meta.yml --lua script.lua -o script.hcb
```

## Practical recommendation

Write source in the decompiler IR style.

That means:

- keep expressions simple
- use `Sx` temporaries explicitly
- use explicit comparisons in conditions
- use top-level `global` / `volatile` declarations
- keep control flow structured unless you intentionally use `__pc` dispatcher form

If a construct does not obviously map to a specific VM opcode sequence, do not assume it is supported.

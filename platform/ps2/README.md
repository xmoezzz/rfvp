# PlayStation 2 (experimental)

`rfvp-ps2.elf`: the engine (`host/`, Rust) linked with a ps2sdk runtime (`runtime/main.c`)
that owns the GS (gsKit), the pad (libpad), sound (audsrv) and the main loop.

## Building

Needs the ps2dev toolchain (`PS2DEV`, default `~/ps2dev`, with gsKit), Homebrew LLVM
(`/opt/homebrew/opt/llvm`, LLVM 23 or newer: it knows the R5900), python3 and the nightly in
`build_ps2.sh`.

```sh
make -C platform/ps2/runtime          # -> target/ps2/rfvp-ps2.elf
```

## The game

A game repacked for the small consoles, as for the PSP:
`rfvp-convert convert --platform ps2 --input <game> --output <out>` (640x448 target, loose folders, the `defualt.tmap`
bitmap font, `rfvp.toml`). It is read from:

- `host:game` when the ELF is started from `host:` (an emulator's host file system: in PCSX2,
  enable Host Filesystem and boot the ELF; put the game in a `game` folder next to it);
- `mass:/RFVP` otherwise (a USB drive).

Saves go to `saves/<game>/` in the game folder. `last_error.txt` there says why a game stopped.
`nls.txt` (`sjis`, `gbk`, `utf8`) picks the script encoding.

## Controls

As on the PSP: Cross = Enter (advance, confirm), Circle = Escape (cancel, menu), D-pad = arrow
keys, Triangle = Space, Select = Escape, L1/R1/L2/R2 = Ctrl (hold to skip text), Start = quit.
Moving the left stick shows a cursor; Cross and Circle then click at it. The D-pad hides it.

## Debugging

With `debug.txt` in the game folder the port writes `debug_report.txt` (frame rate, log) once a
second, and saves the screen to `debug_screen.bmp` at frame `shot=N` (a line in `debug.txt`;
300 if absent), then stops: reading the GS back leaves the GIF unusable.

## How the Rust code is built

The EE is an R5900: MIPS III with 64-bit registers and 32-bit pointers (n32 ABI), a
single-precision FPU, no ll/sc and no 64-bit multiply/divide. rustc's LLVM has no R5900, so
`build_ps2.sh` has rustc emit LLVM bitcode (`mips64r5900el-sony-ps2.json`) and compiles it with
LLVM 23, working around what that LLVM gets wrong for the R5900:

- atomics are lowered to plain loads and stores (no ll/sc; the engine runs on one EE thread);
- compiler_builtins is compiled from its embedded bitcode, so float routines are LLVM's own;
- Rust's saturating float-to-int casts are rewritten (`fix_float_casts.py`): LLVM's expansion
  gave wrong results;
- the FPU compares only with `c.f`, `c.eq`, `c.olt`, `c.ole`: the unordered forms LLVM emits
  (`c.un`, `c.ueq`, `c.ult`, `c.ule`) are rewritten to those in the assembly;
- the object is checked for instructions the R5900 lacks.

No float crosses between Rust and C as an argument.

## Not done

- Speed: the EE has no FPU for Rust's float arithmetic (LLVM calls routines for it); motion and
  text are fine so far, but heavy scenes are untested.
- Movies; the save/load screens and long play are untested.

# The game script, compiled ahead of time (PlayStation)

The PlayStation runs the game's HCB script as MIPS code compiled from it, not through the
interpreter. `build_ps1.sh` does it for the converted game it builds:

1. `hcb_aot.py` compiles the HCB to C (`seg*.c`, one C function per *segment*).
2. GCC (PSn00bSDK's) compiles that C, `-Os`, each segment on its own.
3. `aot_link.py` packs the compiled segments into `SCRIPT.AOT`, resolving their calls into the
   runtime against the executable they were built for (rebuilding one means rebuilding the
   other; `build_ps1.sh` does both).

The runtime is `crates/rfvp/src/aot.rs` (feature `psx_aot`); `aot.h` is its interface with the
compiled code.

## What the compiler does

* The evaluation stack is resolved at compile time: values are C expressions and variables.
  What lives across a call or a yield is kept in the function's frame; nothing else is.
* Values are 64 bits (tag, payload; Nil is zero). Type inference over the whole script
  (arguments, returns, globals, locals along the control flow) finds what is always an int,
  nil or true: that arithmetic, those comparisons and branches are plain MIPS code. Where types
  are not known, the int case is still inline behind a check of the tags; floats, strings and
  tables go to the runtime, which uses the engine's own operations (so their results are the
  interpreter's, bit for bit; floats are software floats, the R3000 has no FPU).
  Truth is "not Nil", as in the VM.
* Constants are folded; unreachable code is dropped; small functions are inlined into the
  functions that run often.
* Calls and yields: a segment runs until it calls a script function, returns, yields (a
  thread syscall) or continues in another segment; the frame records where it resumes. Calls
  from code that runs often run the callee right there (nested on the native stack) and go on
  when it returns.
* Syscalls are called directly by index, with their arguments on the stack.
* Strings stay in the HCB file on the disc: the code holds a string's address there (also its
  identity for TextPrint, as in the interpreter). Short ones (file names, labels) travel with
  the segments that use them; the runtime reads the others from the HCB when a syscall needs
  their text.

## Loading code

`SCRIPT.AOT` keeps the segments in 8 KiB pages; the runtime caches *segments* (a fixed block of
RAM, taken at boot, so that the heap is not broken up), loads a missing one's page in one read
and keeps the segments of it that ran in the recorded run (below).

Their order on the disc comes from a recorded run of the game on the desktop harness
(`crates/rfvp/src/no_std_harness.rs`): `order/<script>.txt` lists the functions in the order
they were first called, with their call counts (the often called ones get inlining and nested
calls), and `order/<script>.pcs` how many times each instruction ran and when it first did
(segments that ran every frame first, then those that ran with each line of text, then the
rest, each in the order they first ran: what runs together is together on the disc). To
record them again:

```sh
RFVP_HARNESS_GAME=/path/to/game RFVP_HARNESS_FONT=/path/to/font.otf \
RFVP_HARNESS_FRAMES=9000 RFVP_HARNESS_FUNCS=order/akaseka.txt \
RFVP_HARNESS_PCS=order/akaseka.pcs RFVP_HARNESS_PCS_FROM=1200 \
  cargo test -p rfvp --lib --no-default-features --features no_std --release \
  no_std_harness::boots -- --nocapture --test-threads 1
```

A game without these files still compiles and runs; its code is then laid out in script order.

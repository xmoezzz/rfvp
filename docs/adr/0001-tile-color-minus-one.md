# ADR-0001: Tile color -1 is the window color from volatile globals

- Status: Accepted
- Date: 2026-10-19
- Issue: [#55](https://github.com/xmoezzz/rfvp/issues/55)

## Context

In はっぴぃ☆マーガレット! the message box and the name box were drawn opaque white, while the
original draws them translucent pink. The in-game "window color" R/G/B/A sliders had no
visible effect.

The script creates both boxes as `PrimSetTile(513, -1, ...)` and `PrimSetTile(514, -1, ...)`.
The bytecode really pushes `-1` (`push_i8 0xFF`); the compiler emits 255 as `push_i16 255`, so
-1 is deliberate. AngelWish and WA_funta use Tile -1 the same way.

rfvp treated -1 as palette slot 255 (`tile as u8`). The game's init code also runs
`ColorSet(-1, 174, 118, 118, 170)` followed by `ColorSet(255, 255, 255, 255, 255)`. Our
`ColorSet` cast the id to `u8` before its range check, so the first call wrote slot 255 and the
second overwrote it with opaque white.

## Evidence

- `PrimSetTile` (`0x417620`) stores the color id as a word and accepts -1..255. Anything else
  leaves the default of 255.
- Tile drawing (`0x416b83`, the Tile entry of the jump table at `0x416c20`):
  - id == -1: the color is `0x412bb0(5)`, `(6)`, `(7)`, `(8)` as R, G, B, A.
  - otherwise: palette lookup `[base + id*4 + 0xabd74]`.
- `0x412bb0(n)` reads global `base + n`, where `base` (`+0xadf74`) is set from the HCB header's
  non-volatile global count (`0x416eac`). It returns the int value when the global holds an
  Int, otherwise 0. Only the low byte of each channel is used.
- Marguerite has 1272 non-volatile globals, so Tile -1 reads `G[1277..1280]`. These are the
  globals the config screen's window color sliders edit (`f_00061F95(..., G[1277], ...)`).
  The engine reads them every frame, so a slider change shows immediately.
- `ColorSet` (`0x4132c0`) does a signed `0 <= id < 256` check. `ColorSet(-1, ...)` does nothing.

## Decision

- `ColorManager::tile_color(tile)` returns `get_int_var(5..=8) as u8` as R, G, B, A for -1,
  and the palette entry for any other id. The GPU, host-command and software renderers all draw
  Tile primitives through it.
- `ColorSet` range-checks the signed id and ignores anything outside 0..=255.

## Consequences

- Message boxes take the window color the player chose and update as the sliders move.
- `ColorSet(-1, ...)` no longer clobbers palette slot 255.
- Tile -1 reads `GLOBAL` while drawing, which takes its lock once per Tile -1 primitive. The
  script VM never holds that lock across a frame.
- Tile ids below -1 cannot occur: `PrimSetTile` rejects them and keeps 255, as the original
  does. The original's out-of-bounds palette read for such ids is unreachable and is not
  modeled.

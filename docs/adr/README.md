# Architecture Decision Records

Each record documents one decision about how rfvp reproduces the original FAVORITE View Point
engine, with the reverse-engineering evidence behind it.

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-tile-color-minus-one.md) | Tile color -1 is the window color from volatile globals | Accepted |
| [0002](0002-sprt-texture-minus-one.md) | Sprt texture -1 draws the frame last shown | Accepted |
| [0003](0003-prim-scale-clamp.md) | PrimSetRS/PrimSetRS2 clamp the scale to [100, 10000] | Accepted |
| [0004](0004-true-type-tag.md) | PrimSetZ and TimerSuspend test for `True`, not `Float` | Accepted |
| [0005](0005-behaviors-not-replicated.md) | Original-engine behaviors rfvp does not replicate | Accepted |

## Conventions

- Addresses refer to `Marguerite.exe` (はっぴぃ☆マーガレット!, 2007) unless stated otherwise.
  Decisions that could depend on the engine version were cross-checked against `Sakura.exe`.
- Script values carry a type tag in the original VM: 0 = Nil, 1 = True, 2 = Int, 3 = Float,
  4/5 = String. This matches the order of `Variant` in `crates/rfvp/src/script/mod.rs`.
- New records take the next number and are never renumbered. A decision that is replaced is
  marked `Superseded by ADR-NNNN` and kept.

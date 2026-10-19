# ADR-0003: PrimSetRS/PrimSetRS2 clamp the scale to [100, 10000]

- Status: Accepted
- Date: 2026-10-19

## Context

rfvp reset any scale outside 0..=10000 to 1000 and accepted 0..99 unchanged. WA_funta calls
`PrimSetRS2(249, nil, 0, 0)` and `PrimSetRS2(269, nil, 0, 0)`, which rfvp scaled to nothing.

## Evidence

- `PrimSetRS` (`0x4153e0`), Marguerite:
  - Rotation is stored modulo 3600 and normalized to be non-negative, when it is an Int.
  - The scale is clamped to [100, 10000] when it is an Int, and left unchanged otherwise.
- `PrimSetRS2` in Sakura (`0x43bb80`) clamps X and Y the same way, each independently. The
  behavior is not specific to one engine version.

## Decision

An Int scale is clamped to [100, 10000]. A non-Int scale keeps the prim's current scale.

## Consequences

- `PrimSetRS2(id, nil, 0, 0)` draws at 10%, as in the original, instead of disappearing.
- An out-of-range scale no longer snaps back to 100%.

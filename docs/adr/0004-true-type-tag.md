# ADR-0004: PrimSetZ and TimerSuspend test for `True`, not `Float`

- Status: Accepted
- Date: 2026-10-19

## Context

The original VM tags script values 0 = Nil, 1 = True, 2 = Int, 3 = Float, 4/5 = String
(`FloatToInt`, `0x413860`, tests tag 3). Scanning every syscall handler for comparisons
against tag 1 or 3 finds three: `FloatToInt` (3), `PrimSetZ` (1) and `TimerSuspend` (1). rfvp
had both tag-1 cases wrong.

## Evidence

- `PrimSetZ` (Marguerite `0x415650`, Sakura `0x43bff0`):
  - Int: clamp to [100, 10000], store it, and set attr bit 0x04.
  - Tag 1 (True): keep z and set attr bit 0x04.
  - Anything else: clear attr bit 0x04.
  - Always: set the dirty bit 0x40.

  rfvp applied the True case to Float, so True cleared the bit.
- `TimerSuspend` (Marguerite `0x416580`, Sakura `0x437cb0`) stores `type != 1`. The timer tick
  (`0x412430`) skips all timers while that flag is set. So only True resumes the timers, and
  nil, an Int or any other value suspends them. rfvp suspended only on nil
  (`!canbe_true()`), so an Int argument resumed the timers instead.

## Decision

- `PrimSetZ` keeps z and enables Z for `Variant::True`. Float falls into the "anything else"
  case.
- `TimerSuspend` suspends unless the argument is `Variant::True`.

## Consequences

Both syscalls now match the two engine versions checked. In the scripts checked (Marguerite,
AngelWish, WA_funta), nothing calls `TimerSuspend`, and nothing passes a literal `true` or
float to `PrimSetZ`. The change only matters for values held in variables and for other titles.

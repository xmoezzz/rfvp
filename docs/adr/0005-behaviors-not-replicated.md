# ADR-0005: Original-engine behaviors rfvp does not replicate

- Status: Accepted
- Date: 2026-10-19

## Context

A pass over all 143 syscall handlers of `Marguerite.exe` found behaviors that rfvp does not
reproduce. Some are defects in the original engine: a register left over from an earlier
step, a field written twice, an out-of-bounds read. Others are plain differences that no
script in the local game set (Marguerite, AngelWish, WA_funta) runs into. This record lists
them, so they are not rediscovered and "fixed" by accident, and so a title that does depend on
one can be handled with this context.

## Decision

rfvp keeps its current behavior for every item below. An item is revisited only when a title
is found whose scripts depend on it.

### Engine defects

**Dissolve rectangle: Y overwrites X.**
The 7-argument `Dissolve` stores both its 4th argument (X) and its 5th (Y) into the X field
of the dissolve rectangle. Y always keeps its default of 0. This is Marguerite `0x41355a` and
`0x41356a` (both write `+0x80fe6`), and Sakura `0x43a0bc` and `0x43a0cc` (both write
`+0x100ff0`), so it survives across engine versions. rfvp stores X and Y in their own fields.
No local script passes a rectangle to `Dissolve`.

**TextShadowDist with a non-Int distance uses the text id.**
`TextShadowDist` (`0x415f10`) clamps an Int distance to [0, 12]. With any other argument it
jumps straight to the store while `eax` still holds the text slot id, so
`TextShadowDist(id, nil)` sets the shadow distance to `id`. rfvp uses 0. No local script
calls it without an Int distance.

**PrimHit on a Sprt with texture -1 or -2.**
Hit-testing a Sprt (`sub_414E90`) takes the size of `graph[texture id]`. For -1 and -2 that
reads outside the graph table. rfvp registers no hit proxy for Sprt -1 (see ADR-0002). The
movie sprite (-2) is hit-tested against the movie graph.

**Tile ids below -1.**
The Tile draw path (`0x416bc7`) sign-extends the color id and indexes the palette with it, so
a negative id other than -1 would read memory in front of the palette (the end of the last
script thread's context). The path is unreachable: `PrimSetTile` (`0x417620`) only stores
-1..255 and keeps the default 255 otherwise. rfvp does the same (see ADR-0001), so no Tile
ever holds such an id.

### Differences not taken up

**IntToText width.**
The original (`0x413c50`) formats with `%0Nd` when the width is an Int in 1..10, and with
`%d` otherwise, including a nil width. rfvp returns nil when the width is not an Int, and pads
to whatever Int width it is given. The local scripts call `IntToText(x, nil)` 9 to 22 times
each, where the original returns the plain number. A negative width becomes a huge padding
width in rfvp (`width as usize` in `utils.rs`), which can exhaust memory.

**TextFontSet(-1).**
`TextFontSet` (`0x418a70`) accepts -5 up to the font count minus one, and replaces -1 with
-2 (MS Gothic) before looking up the font name. rfvp treats -1 as "the current font".
`TextFontSet(G[1032])` in Marguerite takes its value from the config screen; whether that is
ever -1 was not established.

## Consequences

- None of the defects shows in the local game set.
- The two differences change visible output for any script that hits them. `IntToText(x, nil)`
  is called by every local title.
- An entry moves out of this record into its own ADR when it is implemented.

# ADR-0002: Sprt texture -1 draws the frame last shown

- Status: Accepted
- Date: 2026-10-19

## Context

`PrimSetSprt` accepts texture ids from -2 up. rfvp drew -2 (the movie frame) but skipped -1.
Marguerite (prim 291, alpha 200, on the config screen) and WA_funta both create Sprt -1
primitives through the same helper.

## Evidence

- Sprt drawing (`0x416b02`): -1 calls `sub_407440` and -2 calls `sub_407b10` (the movie).
- `sub_407440` binds renderer texture `+0x1e0` (`this[120]`), one of three screen-sized render
  targets created in `0x40a6b0`.
- Present (`sub_4083B0`) runs `EndScene`, then `Present`, then `StretchRect`s the back buffer
  into that texture. The main loop (`sub_4199F0`) calls it every frame after drawing the prim
  tree. So the texture always holds the last presented frame, including dissolves and the
  overlay root.
- Geometry in `sub_407440`:
  - The quad is the screen size, or the prim's W/H when attr bit 1 is set.
  - UV always covers the whole captured frame; the prim's U/V are not used.
  - Rotation, scale, pivot (attr bit 2) and V3D (attr bit 4) work as for other sprites. The
    default pivot is (0, 0).
  - Color is white with the prim's alpha; the prim's blend mode applies.
- `PrimHit` tests a Sprt against `graph[texture id]`, which for -1 reads outside the graph
  table. It is not a meaningful hit area.

## Decision

Each renderer keeps a copy of the frame last shown, but only while a visible (alpha > 0)
Sprt -1 is being drawn:

- GPU (`gpu_prim.rs`, `app.rs`): `capture_prev_frame` copies the virtual render target into a
  same-sized render target on the GPU (`copy_texture_to_texture`) before the frame's render
  pass. The copy happens on the same frame the sprite first appears.
- Host commands (`prim_commands.rs`, `no_std_core.rs`): read the frame back with the existing
  `RfvpRenderer::capture_thumbnail` at the virtual size and upload it as a reserved texture
  handle. A host that cannot read its frame back returns `None` and the sprite is skipped, as
  before. The PlayStation build leaves this out, as it has no memory for a frame of RGBA. The
  frame is read back on the frame after the sprite first appears, so that first frame does not
  draw it.
- Software renderer (`soft_render/renderer.rs`): copy the framebuffer before clearing it, with
  the same one-frame delay.
- Sprt -1 registers no hit proxy.

## Consequences

- The config screen draws the game behind it translucent, as in the original.
- No `RfvpRenderer` API was added: console ports get the feature through `capture_thumbnail`
  where they already implement it.
- On hosts where `capture_thumbnail` is a GPU readback, each frame with a visible Sprt -1 pays
  for one full-frame readback. Frames without one pay nothing.
- The GPU copy samples with the render target's nearest filter. This only shows when attr bit 1
  stretches the frame to a different size.
- `prev_frame_tests` in `prim_commands.rs` covers the host-command path.

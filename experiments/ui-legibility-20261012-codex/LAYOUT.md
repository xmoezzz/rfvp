# UI layout experiment

`layout_probe.py` demonstrates a layout alternative using original raster
lettering. It does not recognize, replace or generate any text. All source code
and generated interface text are English. Localized letters remain image content.

Two source dialogs, AKASEKA_HD and sakura, are illustrated at PSP, PS2, PS1 and
3DS resolutions. Each has two variants, for 16 schematic screen images:

- `uniform`: resize the three original image assets using the platform's global
  scale, then place them in a compact schematic panel.
- `reflow`: remove exactly transparent exterior margins, resize directly from
  the original high-resolution pixels to a larger alpha-bounded image, and solve
  fresh panel and button-slot positions. Targets are 18 pixels high on PSP,
  20 on PS2, and 16 on PS1/3DS. These are image/ink heights, not measured font sizes.

Both variants use the same area filter, the same simple panel/slot primitives,
and the same encoding per platform. They are NOT original script layouts,
hardware captures, or installed runtime changes. The decorated game background
was not used. Simple shapes are assumed to be renderer primitives, not additional
bitmap textures; their execution and framebuffer costs are not measured.

The schematic slots are selected by resource identifiers and roles. No strings
or font metadata enter the image processing. For integrated game support, these
groups would be configured per game/scene rather than inferred semantically.
Reflowing a full baked panel with mixed lettering and decoration would first need
image-region metadata. Wrapping a long baked line also needs segmentation or the
script's text; moving a complete label image does not need text recognition.

## Observed texture costs

Only the prompt, confirm and cancel textures are counted. All display variants
stay within framebuffer bounds and their schematic slots do not overlap.

| Platform | Game | Uniform | Reflow |
|---|---|---:|---:|
| PS1 | AKASEKA_HD | 784 B | 2,240 B |
| PS1 | sakura | 720 B | 2,944 B |
| PSP | AKASEKA_HD | 10,752 B | 21,280 B |
| PSP | sakura | 10,000 B | 28,272 B |
| PS2 | AKASEKA_HD | 15,544 B | 26,080 B |
| PS2 | sakura | 15,156 B | 34,720 B |
| 3DS | AKASEKA_HD | 8,192 B | 12,288 B |
| 3DS | sakura | 6,144 B | 16,384 B |

PS1 uses the frozen project CLUT quantizer, dithering enabled, normal sprite
alpha 255. All three textures remain CLUT4 in both recorded PS1 dialog variants.
Costs include current tile padding and palettes, not allocator fragmentation or
full scene residency. PSP uses the earlier RGBA8888 storage estimate. PS2 figures
are PSMCT32 pixel payloads, NOT GS VRAM; scaling accounts for non-square pixels,
while previews display texel grids. 3DS uses the candidate RGBA4 format with
native tex3ds encoding/decoding, including min-8/power-of-two allocations, not a
verified runtime RGBA4 upload implementation. All 12 unique 3DS textures passed
the native round trip.

The PS1 prompt changes from 73x8 to 143x16 for AKASEKA_HD and from 65x8 to 131x16
for sakura. Visual inspection shows a more legible prompt when given more screen
space and source samples. This is a visual demonstration, not a user-study score.
The larger dialog also occupies more screen space and costs more texture memory.

These results support selective layout adaptation, not a free enlargement of all
assets. Shrinking decorative textures, simplifying panels, grouping states into
atlases or paging menus may fund the additional text storage, but those savings
have not been implemented or measured here. Atlas padding and sampling policy
matter: the currently inspected 3DS backend uses linear filtering, so packing
labels tightly without gutters would not be a safe integration assumption.

## Runtime integration points inspected read-only

- `crates/rfvp/src/subsystem/resources/graph_buff.rs` scales header offsets and
  pivots with the old-school global scale and sets display sizes to texture sizes.
- `crates/rfvp/src/rendering/prim_commands.rs` handles primitive source crops,
  offsets, pivots and parent transforms when drawing images.
- `crates/rfvp/src/subsystem/resources/motion_manager/mod.rs` performs sprite hit
  tests using positions, texture offsets, selected rectangles and alpha masks.
- `crates/rfvp/src/host_api/render.rs` also exposes render-frame hit proxies.

Changing only repacked width/height does not implement scene reflow. A layout
profile must keep final asset raster sizes, primitive placements, crop rectangles,
all visual states and the input/hit coordinate mapping consistent. Mask resources
must not accidentally receive the cosmetic image transformation. Runtime built-in
dialogs and script-driven resource menus are separate paths and need separate
integration decisions.

None of those shared files were edited. No Cargo builds were run. Outputs are
ignored local files beneath `output/layout/`. `layout.json` records asset sizes,
exact transparent-margin crops, positions and schematic hit slots. Input behavior
has NOT been tested in the runtime; the geometric checks apply only to this
schematic. Multi-frame animations, selected/hover/pressed states and script logic
have not been implemented.

Run from the repository root after the earlier experiment's source metadata and
frozen PS1 probe exist:

```sh
python3 experiments/ui-legibility-20261012-codex/layout_probe.py
```

Open `output/layout/report.html` for full native-size comparisons, or
`output/layout/ps1-overview.png` for the focused preview.

The [IGDA Game Accessibility SIG guidance](https://igda-gasig.org/how/game-accessibility-top-ten-se/)
also recommends large, easily readable default text. Its advice supports treating
legibility as a layout decision; it does not establish a universal pixel size
or validate these particular console examples.

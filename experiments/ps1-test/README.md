# PS1 test and low-resolution UI conversion

Launch the current PS1 disc from the command line:

```sh
bash /Users/xmoe/Documents/projects/rfvp/experiments/ps1-test/launch_ps1.sh
```

An optional first argument overrides the cue path. DuckStation and the configured
BIOS are already installed. Recording is started manually in the user's recording
application. No desktop automation or scripted controller input is used.

The independent disc and generated assets are ignored under `output/`. Original
converted resources and Claude's build directory are not overwritten. The test
emulator uses native resolution, nearest texture filtering, normal speed, a stock
CPU, and 2 MiB RAM. Internal TTY framebuffer dumps are available for verification;
these are game output, not captures of the desktop.

## Image processing

The PS1 and rebuilder converters share `rfvp-image`, a Rust implementation of
balanced detail-preserving image downscaling (DPID). It runs locally without OCR,
LLMs, model downloads, or extra services. Only color UI resources in `graph/`
use the detail filter. Masks, backgrounds, and character packs use area reduction.
PS1 palette encoding falls back to area when the DPID result exceeds the encoded
size or GPU allocation of area reduction at the same layout and dimensions.

The explicit profiles in `profiles/` are for AKASEKA_HD at 1920 x 1080. They enlarge
and reposition complete UI assets, including all button states. They do not infer
text, invent letters, or rewrite the game's scripts. Other games need their own
layout metadata. All four profiles passed viewport and title-row overlap checks.
The PS1 title screen has been validated in the actual runtime; dialog interactions
and full story playback still need manual validation.

PS1 image-only update example:

```sh
cargo run -p rfvp-psx --release -- \
  --input /path/to/original/game --output /path/to/converted/disc \
  --images-only --image-pack graph --image-filter dpid-balanced \
  --ui-layout /Users/xmoe/Documents/projects/rfvp/experiments/ps1-test/profiles/ps1.json
```

The rebuilder accepts the same filter and layout arguments for `--platform psp`,
`--platform ps2`, and `--platform 3ds`. Wii keeps the existing area path.

`output/comparison/report.html` shows decoded target-format assets and native
sizes. Across all 3,426 PS1 graph entries, balanced filtering did not exceed the
area-with-the-same-layout encoded or VRAM budgets. Enlarging the 44 explicitly
listed UI states increases their aggregate allocation by 25,632 bytes; this is
not a measurement of peak resident VRAM.

## CI

The desktop executable now declares its `text_codec` module. The desktop docs job
excludes `rfvp-ps1` alongside the other console hosts to avoid unifying incompatible
desktop and no_std decompression features. Both the desktop binary check and the
workflow's complete documentation command passed locally. The isolated CI fixes
are proposed in https://github.com/xmoezzz/rfvp/pull/41.

## Performance measurement

`DEBUG.TXT` with `profile=1` enables stage timings, reported every 30 frames.
The clock has VBlank resolution, so stage timings are averages and approximate.
The first frame's initialization is excluded. Do not enable `shot` while measuring
steady performance: dumping a framebuffer over the TTY introduces a large stall.

The isolated profiler executable and disc are under `output/perf/`, with logs in
`output/perf-before.log` and `output/perf-launch.log`. Its portable DuckStation
copy keeps logging and emulator settings separate from the user's installation.

## Paused handoff

Work stopped at the user's request. The last validated runnable performance disc is
`output/perf-speed/rfvp.cue`: stock CPU, 2 MiB RAM, sustained title-screen 15 FPS
with profiling enabled, heap 648 KiB and peak 700 KiB. Logs: `output/perf-speed.log`.
It contains the enlarged UI assets. Full gameplay performance is not measured.

Additional shared-corner and C interpolation optimizations remain in source and
have not been validated in a running PS1 executable. The attempt to create
`output/perf-final/` failed during linking: repeated Rust ThinLTO builds supplied
cached machine objects without the required bitcode entry points. An attempted
LTO-off workaround was reverted. Fix/recheck this build issue before relying on
any newly generated executable. The validated `perf-speed` disc was not replaced.

Core tests: 46 passed before the final shared-corner change; both transform tests
passed after it. Default desktop binary check passed after the source changes.
The save-header fixture now uses an opaque, representable palette color and the
matching thumbnail dimensions, so it also works with the PS1's lossy format.

## Image refinement, resumed 2026-10-12

The converters now support optional `coverage_threshold` in a layout asset rule.
This keeps more of a selected label's alpha above its drawing threshold after
downscaling, without changing dimensions, placement, animation count, or formats
on PSP, PS2 and 3DS. One gain is shared by all frames and capped at 1.5. It never
adds pixels outside existing nonzero alpha and never reduces alpha. Masks and
glyphs stay on their original path. Omit the field for soft panels and artwork.

This is an independent, conservative adaptation of the industry method documented
by Microsoft in [ScaleMipMapsAlphaForCoverage](https://github.com/microsoft/DirectXTex/wiki/ScaleMipMapsAlphaForCoverage)
and by its original author in [Computing Alpha Mipmaps](https://www.ludicon.com/castano/blog/articles/computing-alpha-mipmaps/).
It uses existing Rust dependencies, with no OCR, LLM, model download or new native
library. Coverage is a diagnostic, not a guarantee of readable lettering. Text
only two or three pixels high still needs deliberate layout enlargement.

The AKASEKA_HD profiles enable it on 42 title and confirmation label states:
threshold 192 for PS1's opaque alpha class, 128 for the other three platforms.
The existing scales and positions are unchanged. Other games need explicit
profiles; the experiment uses some additional selected labels for comparison.

PS1 quantization reserves a palette entry for every present alpha level before
allocating the remaining colors. Previously, a large opaque fill could exhaust
the palette and discard rare transparent edges. Palette and dithering ties are
now deterministic. Premultiplied colors are clamped to alpha before filtering.
Newly compacted CLUT4 candidates must meet alpha-weighted RGB RMS <= 8/255;
the previous alpha allocation is consulted to preserve eligibility on the same
resized pixels, but its palette is never written. Gradients previously using
CLUT8 remain CLUT8 when the color guard rejects CLUT4. Previously eligible CLUT4
gradients retain their existing quality compromise. The DPID cost guard still
compares actual encoded bytes and GPU allocation with Area at the same layout,
with coverage disabled in that baseline. An older converter's pack is a separate
comparison and can contain individual promotions.

### Measured results

`output/refinement/report/index.html` compares 24 original assets from three
games, across 216 actual encoded variants. The PS1 sample's summed texture
allocation decreases from 38,416 to 36,848 bytes (4.1%). Some thin strokes become
more visible; native-size text can still remain unreadable. PSP and 3DS show
modest alpha improvements. The PS2 sample is already stable and stays unchanged.
Previews decode converter output; they are not device captures. PS1 previews
assume normal opaque primitives; blended primitives also depend on STP.

All four full AKASEKA_HD graph sets were rebuilt separately in
`output/refinement/full/`. Every set has 3,426 entries with unchanged geometry,
placement and frame counts. The previous converted outputs remain available.

| Platform | File bytes before / after | Texture bytes before / after |
| --- | ---: | ---: |
| PS1 | 14,774,310 / 14,727,826 | 15,595,208 / 15,540,328 |
| PSP | 37,401,853 / 37,401,749 | 138,211,264 / 138,211,264 |
| PS2 | 60,902,558 / 60,902,450 | 239,487,340 / 239,487,340 |
| 3DS | 28,006,677 / 28,006,614 | 187,788,032 / 187,788,032 |

These sum all color assets, not peak resident memory. File totals exclude the
outer pack index and alignment. PS1 includes DMA padding and two CLUT copies;
PSP counts its GPU-readable RAM buffer; 3DS counts power-of-two RGBA8 storage;
PS2 counts RGBA pixels before GS page allocation. Masks are excluded from texture
totals. The PS1 full-set saving is 54,880 bytes (0.35%), smaller than the selected
sample suggests. 38 textures shrink, while four quality-guard decisions promote
individual textures to CLUT8: `a_bus_s1`, `a_cg_p3`, `sd_as_e003_52`, `ssel_ps4`.
Those four comparisons are in `output/refinement/report/full.html`. No per-asset
or peak-memory reduction is claimed. Full gameplay validation remains pending.

### Reproduce

Use a Python environment with Pillow for the reports; it is not a converter
dependency. The script freezes the previous PS1 converter at `3748fa3` and builds
both current converters with an isolated Cargo target under this experiment.
Run from the repository root:

```sh
python3 experiments/ps1-test/refine_images.py
python3 experiments/ps1-test/refine_images.py --audit-full
CARGO_TARGET_DIR=experiments/ps1-test/output/cargo \
  cargo test -p rfvp-image -p rfvp-psx -p rfvp-rebuilder
```

The audit requires the four converted sets in `output/refinement/full/`; it does
not regenerate them. Build the converters with `cargo build --release -p
rfvp-psx -p rfvp-rebuilder` using the same isolated target directory. For PS1,
copy `FILES.MAP` and `RFVP.TOM` from an existing conversion into the new output
directory, then run the PS1 image-only command above with the new output path.
For PSP, PS2 and 3DS use `rfvp-rebuilder --platform PLATFORM --input AKASEKA_HD
--output OUTPUT --images-only --image-pack graph --image-filter dpid-balanced
--ui-layout experiments/ps1-test/profiles/PLATFORM.json`.

Validation: 39 converter/shared-image tests passed, including rare alpha levels,
deterministic palettes, coverage through both encoders, unchanged masks, animated
metadata, and the PS1 encoding/VRAM guard. Wii behavior and console runtimes were
not changed in this resumed image work.

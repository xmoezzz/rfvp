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
  --images-only --image-pack graph.bin --image-filter dpid-balanced \
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

# rfvp for Nintendo Switch

The Nintendo Switch port of the `rfvp` `no_std` core, as a homebrew `.nro`.

**Status: experimental.** It runs in Ryujinx at 60 fps with picture and sound; it has not been run
on a console yet.

## Layout

- `runtime/main.c`: the libnx runtime. It owns the process loop, controller and touch input, audio
  (audren/audrv) and the display (libnx's framebuffer on the default window; presenting waits for
  vsync, which paces the loop at 60 fps). This is the path most homebrew takes, and the one
  emulators handle.
- `host/`: the engine side, a Rust static library (`rfvp-horizon`) linked into the runtime. It runs
  one frame per call (`rfvp_switch_frame`), draws into the framebuffer the runtime lends it, mixes
  audio when the runtime asks (`rfvp_switch_audio_render`), and reads files through newlib's stdio.
- `aarch64-nintendo-switch-rfvp.json`: the Rust target for the library.

## Building

```bash
./platform/switch/build_switch.sh       # -> target/rfvp-switch/switch/rfvp.nro
```

You need [devkitPro](https://devkitpro.org) with devkitA64, libnx and switch-tools (`DEVKITPRO`,
default `/opt/devkitpro`) and rustup. The script installs the pinned nightly from
`platform/consoles/toolchain.env`, so the build does not break when a newer nightly does.

The library builds for `aarch64-nintendo-switch-rfvp.json`, a copy of the built-in
`aarch64-nintendo-switch-freestanding` target with `dynamic-linking` turned off. The stock target
allows dynamic libraries, so rustc also tries to build `rfvp`'s `cdylib` (which the desktop and
mobile packages need) and fails for lack of a global allocator.

## Installing a game

Copy the NRO to `sdmc:/switch/rfvp/rfvp.nro` and the game to `sdmc:/switch/rfvp/game/`:

| File | |
| --- | --- |
| `*.hcb` | the game's script |
| `*.bin` | the game's resource packs (`graph.bin`, `bgm.bin`, ...) |
| `default.ttf` | a CJK font; the engine does not ship one |
| `save/` | made by the port; holds the saves |

Optionally `sdmc:/switch/rfvp/nls.txt` with `sjis` (default), `gbk` or `utf8`, for the script's
text encoding.

The packs are read from the card as the game asks for them, not loaded into memory. If the game
cannot start or stops, the reason is written to `sdmc:/switch/rfvp/last_error.txt`. If
`sdmc:/switch/rfvp/debug.txt` exists, the port writes a report (frame rate, slowest frame, recent
log) to `debug_report.txt` about once a second and the screen to `debug_screen.bmp` every five
seconds. Log lines also go to the system debug output, which emulators show in their log.

## Controls

| Switch | Game |
| --- | --- |
| A | left click |
| B | right click (menu / back) |
| X | Return |
| Y | Space |
| L, R, ZL, ZR (hold) | Ctrl (skip text) |
| − | Escape |
| + | quit |
| D-pad, left stick | move the cursor |
| touch screen | pointer |

## Saves

Saves go in `game/save/`: one `rfvp_sNNN.bin` per slot and `rfvp_global.bin` for the settings and
read-text marks (written about every two minutes, after each save, and on quit). They are not
interchangeable with the desktop build's saves, which use a different encoding. Saves are stamped
with the console's local time.

## Display

The game is drawn with a software rasterizer (`rfvp::host_api::SoftRaster`) into a 1280x720
frame, with the game frame scaled and letterboxed as it is drawn, then copied into libnx's
framebuffer once per frame. It blends in linear light, as the desktop GPU renderer does. Rendering is behind the host API (`RfvpRenderer`), so a GPU renderer can replace it
without touching the core.

## Testing without a console

The core and the rasterizer run natively. With a game directory the test harness boots it through
the same host API, plays into the story, saves, loads and checks the results:

```bash
RFVP_HARNESS_GAME=/path/to/game RFVP_HARNESS_FONT=/path/to/font.otf \
  cargo test -p rfvp --lib --release --no-default-features --features no_std \
  no_std_harness -- --nocapture
```

See `crates/rfvp/src/no_std_harness.rs` for the options (screen size, screenshots, frame count).

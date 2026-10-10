# rfvp-horizon

The Nintendo Switch (Horizon OS) host for the `rfvp` `no_std` core. It builds a homebrew `.nro`.

**Status: experimental.** It compiles, and the core it runs has been played through a desktop test
host (see below), but it has not been run on a Switch or an emulator yet.

## Building

```bash
./platform/switch/build_switch.sh       # -> target/rfvp-switch/switch/rfvp.nro
```

You need [devkitPro](https://devkitpro.org) with devkitA64 (`DEVKITPRO`, default `/opt/devkitpro`)
and rustup. The script installs the pinned nightly from `platform/consoles/toolchain.env`, so the
build does not break when a newer nightly does.

The port builds for `platform/switch/aarch64-nintendo-switch-rfvp.json`, a copy of the built-in
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
cannot start, the reason is written to `sdmc:/switch/rfvp/last_error.txt`.

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
interchangeable with the desktop build's saves, which use a different encoding.

A save is stamped with the date `0000/00/00`: the Switch's clock service is not wired up.

## Display

The game is drawn with a software rasterizer (`rfvp::host_api::SoftRaster`) straight into a 1280x720
framebuffer, with the game frame scaled and letterboxed as it is drawn, then copied to the screen
once per frame. Rendering is behind the host API (`RfvpRenderer`), so a GPU renderer can replace it
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

# rfvp-psv

`rfvp-psv` is the PlayStation Vita host crate for the `rfvp` `no_std` core surface.

It is intentionally separate from:

- `rfvp-horizon`
- `rfvp-os`
- `soft-render-core`
- UEFI runtime code
- desktop `wgpu` / `winit` code

The dependency on `rfvp` is fixed to the independent no-std core API:

```toml
rfvp = { path = "../rfvp", default-features = false, features = ["no_std"] }
```

## Features

```toml
default = []
entrypoint = ["vitasdk-backend", "global-allocator"]
c-glue = []
vitasdk-backend = ["c-glue"]
global-allocator = []
```

## Status

**Experimental.** It compiles and produces a VPK, and the core it runs has been played through a
desktop test host (see below), but it has not been run on a Vita or an emulator yet.

## Building

```bash
./platform/vita/build_vita.sh           # -> target/rfvp-psv/psv/rfvp.vpk
```

You need VitaSDK (`VITASDK`, default `/usr/local/vitasdk`), `cargo-vita` and rustup. The script
installs the pinned nightly from `platform/consoles/toolchain.env`.

## Installing a game

Install the VPK, then copy the game to `ux0:data/rfvp/game/` (the folder can be changed at build
time with `RFVP_PSV_VITASDK_ASSET_ROOT`):

| File | |
| --- | --- |
| `*.hcb` | the game's script |
| `*.bin` | the game's resource packs (`graph.bin`, `bgm.bin`, ...) |
| `default.ttf` | a CJK font; the engine does not ship one |
| `save/` | made by the port; holds the saves |

The packs are read from the card as the game asks for them, not loaded into memory. If the game
cannot start the reason is written to `last_error.txt` in the game folder, and a crash to
`panic.txt`.

## Controls

| Vita | Game |
| --- | --- |
| cross | left click |
| circle | right click (menu / back) |
| square | Return |
| triangle | Space |
| L, R (hold) | Ctrl (skip text) |
| select | Escape |
| start | quit |
| D-pad, left stick | move the cursor |
| front touch | pointer |

## Display

The game is drawn with a software rasterizer (`rfvp::host_api::SoftRaster`) straight into a
960x544 framebuffer, with the game frame scaled and letterboxed as it is drawn. The platform layer
only puts the finished frame on the display. This does not use GXM yet.

## Saves

Saves go in `save/` in the game folder: one `rfvp_sNNN.bin` per slot and `rfvp_global.bin` for the
settings and read-text marks. They are not interchangeable with the desktop build's saves, which
use a different encoding.

## Testing without a console

See `platform/switch/README.md`: the same test harness exercises the core both ports use.

## Not done

- A GXM renderer (the display path is the software rasterizer above).
- Hardware testing: audio in particular (the Rust mixer feeds `sceAudioOutOutput`, which blocks
  until the device has room, so it also paces the game while sound plays) has never run on a Vita.
- A date on saves comes from the RTC; nothing else about time is wired up.

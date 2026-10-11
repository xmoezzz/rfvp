# rfvp for Wii

The Wii port of the `rfvp` `no_std` core, as a homebrew `boot.dol` for the Homebrew Channel.

**Status: experimental.** It runs in Dolphin with picture, sound and input, into the game; it has
not been run on a console yet.

The Wii has 88 MiB, so it runs a repacked game: `rfvp-rebuilder --platform wii` scales the
pictures to 640x360 (the 16:9 frame of the Wii's 640x480 picture), turns the movies into MPEG-2
and the text into a bitmap font.

## Layout

- `runtime/main.c`: the libogc runtime. It owns the process, the Wii Remote and the GameCube pad
  (WPAD, PAD), sound (one ASND voice fed from the mixer), the SD card (libfat) and the GPU (GX:
  RGBA8 textures in GX's tiled layout, one TEV stage multiplying the texture by the vertex color).
  The engine runs on its own thread with a 4 MiB stack: libogc's main thread has 128 KiB, which
  the engine outgrows (it crashed in libogc's scheduler the first time a sound started).
- `host/`: the engine side, a Rust static library (`rfvp-wii`) linked into the runtime. Its heap
  is a block of MEM2 managed by talc; newlib's heap (MEM1) keeps libogc's buffers and the
  textures.
- `powerpc-nintendo-wii-rfvp.json`: the Rust target for the library (Broadway: a PowerPC 750,
  hard float, big endian).

## Building

```bash
./platform/wii/build_wii.sh       # -> target/rfvp-wii/apps/rfvp/boot.dol
cargo run -p rfvp-rebuilder --release -- --input <game> --output <repacked> --platform wii
```

You need [devkitPro](https://devkitpro.org) with devkitPPC and libogc (`DEVKITPRO`, default
`/opt/devkitpro`), and rustup; the script installs the pinned nightly from
`platform/consoles/toolchain.env`. The rebuilder needs `ffmpeg` for the movies.

## Installing a game

Copy `boot.dol` to `sd:/apps/rfvp/` and the repacked game to `sd:/apps/rfvp/game/`. If the
game cannot start, the reason is in `sd:/apps/rfvp/last_error.txt`.

In Dolphin, point the SD card at a folder (Config → Wii → SD card: "Enable folder
synchronization") holding `apps/rfvp/`, and boot `boot.dol`. Dolphin writes the folder back
when emulation stops.

## Controls

| Wii Remote | GameCube | Game |
| --- | --- | --- |
| point at the screen | stick | move the cursor |
| A | A | left click (Enter with the cursor hidden) |
| B | B | right click (Escape with the cursor hidden) |
| D-pad | D-pad | arrow keys |
| 1 | X | Space |
| 2 | Y | Enter |
| + / − | Start | Escape (menu) |
| Nunchuk Z, Classic ZL/ZR | L, R, Z (hold) | Ctrl (skip text) |
| HOME | | quit |

The Classic Controller works like the Wii Remote (X = 1, Y = 2). The D-pad hides the cursor until
the pointer moves again. On a console set to 16:9 the picture fills the screen (the TV stretches
it); on 4:3 it is letterboxed.

## Saves

Saves go in `sd:/apps/rfvp/saves/<game>/`.

## Debugging

With `sd:/apps/rfvp/debug.txt`, the log also goes to `log.txt` there. `debug.txt` can hold
`press=a@N` (press a button at frame N: `a`, `b`, `one`, `two`, `plus`, `minus`, `home`, `up`,
`down`, `left`, `right`, `skip`), `point=X,Y@N` (point the Wii Remote at X, Y from frame N;
`point=off@N`), `shot=N` (save the picture to `debug_screen.bmp`) and `quit=N` (end at frame N,
so that Dolphin writes the SD folder back), to test without a controller. A `debug.txt` in the
game folder turns on the engine's report (`debug_report.txt`: frame rate, time per stage).

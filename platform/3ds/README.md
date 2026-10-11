# Nintendo 3DS (experimental)

`rfvp.3dsx`: the engine (`host/`, Rust, `no_std` core) linked with a libctru runtime
(`runtime/main.c`). The runtime owns the GPU (citro3d), sound (ndsp), the buttons and the main
loop. The game is drawn on the top screen, and the bottom screen shows the console.

## Building

You need devkitARM with libctru and citro3d (`DEVKITPRO`, default `/opt/devkitpro`), plus the
nightly pinned in `consoles/toolchain.env` with `rust-src`.

```sh
platform/3ds/build_3ds.sh          # -> target/rfvp-3ds/3ds/rfvp.3dsx
```

## The game

Use `rfvp-rebuilder --platform 3ds --input <game> --output <out>` for native
400x240 top-screen output (400x225 viewport for a 16:9 game), loose folders,
the `defualt.tmap` bitmap font and `rfvp.toml`. It
is read from `sdmc:/3ds/rfvp/game`.

Saves go to `sdmc:/3ds/rfvp/saves/`. If a game stops, `sdmc:/3ds/rfvp/last_error.txt` says why.

Real hardware needs `sdmc:/3ds/dspfirm.cdc` (dumped with DSP1) for sound.

## Controls

- A = Enter (advance, confirm)
- B = Escape (cancel, menu)
- D-pad = arrow keys
- X = Space
- Select = Escape
- L/R/ZL/ZR = Ctrl (hold to skip text)
- Start = quit

Moving the circle pad shows a cursor; A and B then click at it.

## Debugging

With `debug.txt` in the game folder, the port writes `debug_report.txt` (frame rate, log) to
`sdmc:/3ds/rfvp` once a second. A `shot=N` line in `debug.txt` saves the top screen to
`debug_screen.bmp` there at frame N. Lines like `press=a@600` press a button at a frame.

## Not done

- Tested in Azahar only: the title screen runs at about 56 fps on an emulated New 3DS. Long play
  and real hardware are untested.
- Movies.

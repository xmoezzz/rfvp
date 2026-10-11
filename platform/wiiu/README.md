# rfvp for Wii U

The Wii U port of the `rfvp` `no_std` core, as a homebrew `.rpx`.

**Status: experimental.** It runs in Cemu at 60 fps with picture and sound, into the game; it has
not been run on a console yet.

The game runs as it is, at full resolution: no repack, no downscaled textures (a Wii U
application has hundreds of MiB).

## Layout

- `runtime/main.c`: the wut runtime. It owns the process (ProcUI, through libwhb), the GamePad,
  sound (two AX voices looping over a ring the mixer refills each frame) and the GPU (GX2, set up
  by libwhb). The engine's draws for a frame are recorded, then played on the TV and again on the
  GamePad, each letterboxed at its own size; presenting waits for the display, which paces the
  loop at 60 fps.
- `runtime/shaders/`: the one shader, a textured quad tinted by its vertex color, taken from
  devkitPro's SDL port (zlib license; see its README).
- `host/`: the engine side, a Rust static library (`rfvp-wiiu`) linked into the runtime. It runs
  one frame per call (`rfvp_wiiu_frame`), hands the runtime textures (RGBA8, kept in MEM2) and
  quads in the game's virtual coordinates, mixes audio when the runtime asks
  (`rfvp_wiiu_audio_render`), and reads files through newlib's stdio.
- `powerpc-nintendo-wiiu-rfvp.json`: the Rust target for the library (Espresso: a PowerPC 750,
  hard float, big endian).

## Building

```bash
./platform/wiiu/build_wiiu.sh       # -> target/rfvp-wiiu/wiiu/apps/rfvp/rfvp.rpx
```

You need [devkitPro](https://devkitpro.org) with devkitPPC and wut (`DEVKITPRO`, default
`/opt/devkitpro`), and rustup; the script installs the pinned nightly from
`platform/consoles/toolchain.env`. rustc's objects carry `.note.GNU-stack` as a section with
contents, which `elf2rpl` cannot place, so the script drops it before making the RPX.

## Installing a game

Copy `rfvp.rpx` to `sd:/wiiu/apps/rfvp/` and the game to `sd:/wiiu/apps/rfvp/game/`:

| File | |
| --- | --- |
| `*.hcb` | the game's script |
| `*.bin` | the game's resource packs (`graph.bin`, `bgm.bin`, ...) |
| `default.ttf` | a CJK font; the engine does not ship one |

Optionally `sd:/wiiu/apps/rfvp/nls.txt` with `sjis` (default), `gbk` or `utf8`, for the script's
text encoding. If the game cannot start, the screens stay dark red and the reason is in
`sd:/wiiu/apps/rfvp/last_error.txt`.

In Cemu the SD card is the `sdcard` folder of its data directory (on macOS
`~/Library/Application Support/Cemu/sdcard`); start the RPX with `Cemu -g rfvp.rpx`. The log
(`OSReport`) goes to Cemu's `log.txt` when its coreinit logging is on.

## Controls

| GamePad | Game |
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

The layout is the Switch port's (`platform/switch/host/src/event.rs`, shared as `event.rs`).

## Saves

Saves go in `sd:/wiiu/apps/rfvp/saves/<game>/`: one `rfvp_sNNN.bin` per slot and
`rfvp_global.bin` for the settings and read-text marks. Save thumbnails are blank: the GPU's
frame is not read back for them yet.

## Debugging

If `sd:/wiiu/apps/rfvp/debug.txt` exists, the port writes `debug_report.txt` there (frame rate,
time per stage, recent log) about once a second and logs the mixer's peak level each second.
`debug.txt` can also hold `shot=N` (save the TV picture at frame N to `debug_screen.bmp`) and
`press=a@N` lines (press a button at frame N; `a`, `b`, `x`, `y`, `up`, `down`, `left`, `right`,
`plus`, `minus`, `l`, `r`), to test without a controller.

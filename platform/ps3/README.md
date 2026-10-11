# PlayStation 3 (experimental)

`rfvp.elf` / `rfvp.self`: the engine (`host/`, Rust, `no_std` core) linked with a PSL1GHT runtime
(`runtime/main.c`). The runtime owns the RSX, the pad, the audio port and the main loop.

The game runs as it is, at full resolution: no repack. Textures stay at the game's size in RSX
memory (only a side over 4096 would be shrunk), and the RSX draws them at 1280x720 with two
small shaders (`runtime/sprite.vp`, `runtime/sprite.fp`).

## Building

You need [ps3toolchain](https://github.com/ps3dev/ps3toolchain) with PSL1GHT (`PS3DEV`,
default `/usr/local/ps3dev`), plus the nightly pinned in `consoles/toolchain.env` with `rust-src`.

```sh
platform/ps3/build_ps3.sh          # -> target/rfvp-ps3/ps3/rfvp.elf (and rfvp.self)
```

The shaders are written in assembly, so cgcomp builds them without NVIDIA's Cg toolkit.

Building ps3toolchain on macOS: put Apple's `ar` and `ranlib` first on `PATH` (Homebrew's GNU
binutils breaks the host link). Point `PKG_CONFIG_PATH` and `C_INCLUDE_PATH` at Homebrew for
gmp, libelf, ncurses and zlib. The portlibs (step 9) are not needed.

## The game

Use the game's own folder: the `.hcb` script and the `.bin` packs. Add `default.ttf`, a CJK font
(the engine does not ship one; OpenType works too). It is read from the first of these that
exists:

- `/dev_hdd0/game/RFVP00001/USRDIR/game`
- `/app_home/game`: in RPCS3, `game` next to the booted ELF
- `/dev_usb000/rfvp/game`

Saves go to `saves/` in the folder holding `game`. If a game stops, `last_error.txt` there says
why. `nls.txt` in the game folder (`sjis`, `gbk`, `utf8`) picks the script encoding.

## Controls

- Cross = Enter (advance, confirm)
- Circle = Escape (cancel, menu)
- D-pad = arrow keys
- Triangle = Space
- Select = Escape
- L1/R1/L2/R2 = Ctrl (hold to skip text)
- Start = quit

Moving the left stick shows a cursor; Cross and Circle then click at it.

## Debugging

The log goes to the TTY (RPCS3's `TTY.log`). With `debug.txt` in the game folder, the port writes
`debug_report.txt` (frame rate, log) next to the game folder once a second. `debug.txt` can also
hold these lines:

- `shot=N` saves the screen to `debug_screen.bmp` at frame N, and clears to dark red, so an empty
  frame can be told from one that was not read back. In RPCS3, "Write Color Buffers" must be on
  for the read-back (start it with `--config` and a copy of `config.yml` that turns it on).
- `press=cross@600` presses a button at a frame.

## Workarounds in the runtime

- cgcomp's assembler gives a named `TEMP` one register when it is written and another when it
  is read, so `sprite.fp` uses none.
- For assembly input, cgcomp leaves the fragment program's 2D texture coordinate mask empty;
  main.c sets it when it loads the program.
- PSL1GHT's texture format constants lack the hardware's bit 7 (A8R8G8B8 is 0x85).
- The command buffer is rewound to its start once a frame while the RSX is idle. Letting it wrap
  (through the system's callback) stalls the RSX after about 400 frames.

## Not done

- Movies.
- Tested in RPCS3 only (title, menus, the start of the story, saves, 60 fps). Sound is set up
  (the audio port opens), but nobody has listened to it.

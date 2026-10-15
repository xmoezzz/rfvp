# RFVP console converter

For the native desktop interface, see [RFVP Converter GUI](../rfvp-converter-gui/README.md).

One standalone CLI prepares games for PS1, PS2, PSP, 3DS, PS3, PS Vita and
Nintendo Switch. The host program runs on Windows, macOS or Linux; it does not
need a console SDK. The rolling pre-release includes a Windows x86-64 EXE,
a universal Intel/Apple Silicon macOS executable and a Linux x86-64 executable.

```sh
rfvp-convert platforms
rfvp-convert doctor --platform psp
rfvp-convert convert --platform psp --input /path/to/original/game --output /path/to/psp/game
```

Use `ps1`, `ps2`, `psp`, `3ds`, `ps3`, `psv` (or `vita`) or `switch` for
`--platform`. Use a separate output directory for each platform. Source and
destination cannot overlap. A full conversion refuses a nonempty destination
unless `--overwrite` is supplied; an existing directory belonging to another
platform is always rejected. Do not use symlinked files inside the output.

| Platform | Output |
| --- | --- |
| PS1 | ISO9660 source files, FILES.MAP, CLUT pictures, offline Bayer masks, SPU/XA/STR, bitmap font |
| PS2 | Loose resources, 640x448 profile, defualt.tmap and rfvp.toml |
| PSP | Loose resources, 480x272 profile, defualt.tmap and rfvp.toml |
| 3DS | Loose resources, 400x240 profile, defualt.tmap and rfvp.toml |
| PS3, PSV, Switch | Original packs and image geometry, default.ttf, profile.json |

Scaling retains aspect ratio; a 16:9 PS1 game uses a 320x180 viewport. Color
UI uses the existing balanced detail filter and optional `--ui-layout` profile;
masks retain Area reduction and glyphs retain their existing binary semantics.
`--image-filter area` selects the older filter. PS1 alone uses its native palette,
mask and audio/movie formats. The higher-resolution ports retain original image
sizes because their current runtimes do not apply the low-resolution coordinate
scale. They copy media unchanged; this does not add movie support to a runtime.

Low-resolution full conversions need FFmpeg/ffprobe on PATH for media. PS1
needs FFmpeg for movies; `--no-movies --no-music` omits those PS1 assets. The
PS1-only `--font-size` controls its generated bitmap font. Native resource
preparation and image-only updates need no external media tools. Fonts and
font licenses are included in the prepared game directory.

Image update examples:

```sh
rfvp-convert convert --platform 3ds --input /path/to/original/game \
  --output /path/to/3ds/game --images-only --image-pack graph
rfvp-convert convert --platform ps1 --input /path/to/original/game \
  --output /path/to/ps1/disc-source --masks-only --image-pack graph
```

PS1 `--masks-only` retains every existing converted non-mask resource. These
commands require an existing converted directory. Conversion does not build
the engine, create BIN/CUE/ISO images, or assemble installation packages; those
are separate steps. Place the prepared data at the path shown by `platforms`
and install the separately built console executable.

## GUI integration

`rfvp-converter` exposes `ConversionRequest`, `ConversionContext`, `convert`,
`platforms` and `dependencies`. Run `convert(&request, &context)` on a worker
thread. The two existing conversion engines are libraries; no child converter
executable, global Rayon pool or working-directory change is required. Their
existing CLIs (`rfvp-rebuilder`, `rfvp-psx-convert`) remain available.

The observer receives serialized stage/resource/message/finished events even
when multiple workers convert resources. It can return an error to cancel at
a resource boundary; in-flight media work completes first. The original game
is never modified. A failed or cancelled conversion may leave partial output.

For a process-based GUI:

```sh
rfvp-convert --json platforms
rfvp-convert --json doctor --platform ps1 --images-only
rfvp-convert --json convert --platform ps2 --input /path/to/original/game \
  --output /path/to/ps2/game --threads 4
```

Conversion stdout is newline-delimited JSON. Progress events have `kind`,
`stage`, `completed`, `total`, `resource` and `message`; counts reset per stage.
The final `result` contains the conversion summary. Runtime failures produce
an `error` and exit nonzero. CLI help/argument parsing uses the standard Clap
interface. Human progress goes to stderr. A successful conversion writes
`conversion-report.json` alongside the prepared data.

## Build

```sh
cargo build --release -p rfvp-converter
```

The executable is `target/release/rfvp-convert` (`rfvp-convert.exe` on Windows).

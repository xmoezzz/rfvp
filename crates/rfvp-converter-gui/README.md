# RFVP Converter GUI

Native Rust desktop application built with egui/eframe and system file dialogs.
It calls the same `rfvp-converter` library as the CLI; it does not launch a CLI
process or use a browser/webview. All UI text is English.

Download `rfvp-converter-gui-windows-x86_64.exe`,
`rfvp-converter-gui-macos-universal` or `rfvp-converter-gui-linux-x86_64` from
the rolling pre-release. On macOS/Linux, give the downloaded executable
permission to run with `chmod +x <filename>`, then launch it. macOS supports
Intel and Apple Silicon. These are raw executables; app/installer packaging is
separate. Linux needs a desktop session with OpenGL and X11/Wayland; folder
dialogs use the desktop's XDG portal.

Choose PS1, PS2, PSP, 3DS, PS3, PS Vita or Switch, select the original game
directory, and choose a separate destination (a new path can be typed). Start
conversion. Full conversion, image updates, and PS1 mask updates use the CLI's
existing formats and output protection. PS3/Vita/Switch preserve source geometry.
Advanced options expose image reduction, UI layout, worker count and PS1 font
size/movie/music selection. Settings are remembered in the OS application data
directory. `Check media tools` checks FFmpeg/ffprobe on PATH without changing
game data. Requirements are the same as the [CLI](../rfvp-converter/README.md).

Conversion runs on a worker thread. Progress and a bounded log remain responsive.
Cancel stops at a resource boundary after in-flight work finishes; partial output
can remain. Closing during conversion requests cancellation and waits for the
worker before closing. Successful conversion shows the output path and offers
to open the directory or copy the path. Conversion errors are shown in the UI.

```sh
cargo run --release -p rfvp-converter-gui
```

Rust 1.95 or newer is required. On Debian/Ubuntu, build dependencies include
`libxkbcommon-dev`, `libwayland-dev` and `libgl1-mesa-dev`. No console SDK is
required. CI builds Windows x86-64, universal macOS and Linux x86-64 GUI binaries
and publishes them alongside the CLI on pre-release and versioned releases.

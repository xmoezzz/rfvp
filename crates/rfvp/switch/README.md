# RFVP Switch Host

This directory is a separate Nintendo Switch homebrew chain for RFVP.
It is intentionally isolated from the desktop, Android, iOS, and wasm hosts.

Current stage: libnx host linked with dedicated Rust Switch backend crates, with an optional RFVP core C ABI link.

The default build still produces a `.nro` that starts on Switch, initializes a libnx console, initializes the Rust Switch host backend, probes common RFVP game-root directories on the SD card, ticks the Rust backend once per applet frame, and exits with PLUS.

The renderer, audio backend, and shared Switch core ABI live in separate crates under `platform/switch/crates/` and are guarded with crate-wide Switch cfgs. They do not depend on `winit`, `wgpu`, `cpal`, or GStreamer.

## Prerequisites

Install devkitPro with devkitA64 and libnx, then ensure these variables point to that installation:

```sh
export DEVKITPRO=/opt/devkitpro
export DEVKITA64=$DEVKITPRO/devkitA64
export PATH=$DEVKITPRO/tools/bin:$DEVKITA64/bin:$PATH
```

The build script also builds a small no-std Rust static library for the Switch backend using the `aarch64-unknown-none` target by default:

```sh
rustup target add aarch64-unknown-none
```

You can override the Rust target if your local Switch Rust toolchain uses a different target:

```sh
RFVP_SWITCH_RUST_TARGET=aarch64-nintendo-switch-freestanding ./platform/scripts/build_switch.sh
```

The build script passes `--cfg rfvp_switch` to the Rust backend crates, so the crate-wide cfgs are active without affecting normal host builds.

## Build host skeleton

From the repository root:

```sh
./platform/scripts/build_switch.sh
```

Output:

```text
dist/switch/rfvp.nro
```

You can also run:

```sh
make -C platform/switch
```

## Build with RFVP core link

The host crate has an optional `rfvp-core-link` feature. The build script enables it only when `RFVP_SWITCH_LINK_CORE=1` is set and links the external RFVP static library after the Switch host static library:

```sh
RFVP_SWITCH_LINK_CORE=1 \
RFVP_SWITCH_CORE_STATICLIB=/path/to/librfvp.a \
./platform/scripts/build_switch.sh
```

The root RFVP crate exposes the core entry only under:

```sh
--cfg rfvp_switch --no-default-features --features switch-core
```

The exported core ABI is:

```text
rfvp_switch_core_abi_version
rfvp_switch_core_create
rfvp_switch_core_tick
rfvp_switch_core_stats
rfvp_switch_core_destroy
```

## Runtime probe paths

The host currently checks:

```text
sdmc:/rfvp
sdmc:/switch/rfvp
```

The first readable path is passed to `rfvp_switch_host_global_load_game_root()`. Touch input maps to the RFVP left click path. A and B map to left and right click. D-pad maps to arrow keys.

## GPU renderer and audio decoding

The Switch host now consumes the RFVP render command buffer through an EGL/OpenGL ES 2.0 GPU backend in `platform/switch/source/main.c`.
It uploads RGBA8 textures from `UploadTextureRgba8` commands, caches them by RFVP texture id/generation, and draws textured/fill quads on the GPU.

This path requires the Switch Mesa/OpenGL portlibs in addition to libnx. The build script checks `${DEVKITPRO}/portlibs/switch` by default and links:

```text
-lEGL -lGLESv2 -lglapi -ldrm_nouveau -lnx -lm
```

Switch audio decoding is no longer limited to OGG/Vorbis. The Switch core enables Symphonia and decodes supported audio containers/codecs to interleaved PCM16 before feeding the Switch mixer and audout ring buffer. The direct WAV PCM16 parser and lewton OGG/Vorbis decoder remain as fallback paths.

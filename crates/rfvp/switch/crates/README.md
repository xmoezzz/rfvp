# RFVP Switch backend crates

This directory is the dedicated Switch backend crate workspace. It is separate from the desktop/mobile/web `rfvp` host chain.

- `rfvp_switch_core_abi`: shared no-std C ABI structs and constants used by the Switch host and the optional RFVP core entry.
- `rfvp_switch_render`: Switch render backend boundary. It owns Switch-side texture IDs and records render commands that will be lowered to the final deko3d implementation.
- `rfvp_switch_audio`: Switch audio backend boundary. It owns the Switch-side PCM ring buffer and the C ABI used by the libnx host.
- `rfvp_switch_host`: thin Switch host state crate that composes the render and audio crates, optionally links to the RFVP core ABI, and exposes the global C ABI used by `platform/switch/source/main.c`.

The Switch crates use crate-wide cfg gating:

```rust
#![cfg(any(target_os = "horizon", target_vendor = "nintendo", rfvp_switch))]
```

`build_switch.sh` passes `--cfg rfvp_switch` while building them. This keeps the backend isolated from ordinary desktop, Android, iOS, and wasm builds.

These crates intentionally do not depend on `winit`, `wgpu`, `cpal`, or GStreamer.

`rfvp_switch_render` remains the backend-neutral command boundary. The current libnx consumer lowers those commands to EGL/OpenGL ES 2.0 in `platform/switch/source/main.c`; the command ABI is still suitable for a later deko3d/NVN implementation.

`rfvp_switch_audio` receives already-decoded PCM from the RFVP Switch core. Decode is performed before the mixer so volume, pan, fade-in, fade-out, and tween ramps are applied at sample granularity.

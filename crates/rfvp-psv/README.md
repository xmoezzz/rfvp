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

## Implemented backends

The `vitasdk-backend` feature compiles both:

```text
c/rfvp_psv_c.c
c/rfvp_psv_vitasdk.c
```

`rfvp_psv_vitasdk.c` is the actual VitaSDK backend. It includes PSV headers and uses VitaSDK APIs for:

```text
<psp2/display.h>              sceDisplaySetFrameBuf / sceDisplayWaitVblankStart
<psp2/ctrl.h>                 sceCtrlSetSamplingMode / sceCtrlPeekBufferPositive
<psp2/touch.h>                sceTouchSetSamplingState / sceTouchPeek
<psp2/io/fcntl.h>             sceIoOpen / sceIoPread / sceIoLseek / sceIoClose
<psp2/io/stat.h>              sceIoGetstat
<psp2/io/dirent.h>            sceIoDopen / sceIoDread / sceIoDclose
<psp2/kernel/processmgr.h>    sceKernelExitProcess
```

## Display path

The current renderer is a Vita software-framebuffer backend:

```text
rfvp no_std core
  -> RfvpRenderer host API
  -> C RGBA8 CPU renderer
  -> VitaSDK present callback
  -> SceDisplayFrameBuf 960x544 A8B8G8R8 framebuffer
```

This does not use GXM yet.

## Input mapping

Current temporary input mapping:

```text
START          -> Quit
D-pad          -> cursor move
Left stick     -> cursor move
CROSS          -> left pointer button
CIRCLE         -> right pointer button
Front touch    -> touch event + left pointer event
```

## Asset root

Default asset root:

```text
app0:/
```

Override at build time:

```bash
RFVP_PSV_VITASDK_ASSET_ROOT='ux0:data/rfvp' \
  cargo build -Z build-std=core,alloc \
  --target armv7-sony-vita-newlibeabihf \
  -p rfvp-psv \
  --features entrypoint
```

## Build

You need VitaSDK installed and `VITASDK` pointing to the SDK root.

```bash
export VITASDK=/usr/local/vitasdk
export PATH="$VITASDK/bin:$PATH"

cargo +nightly vita build vpk \
  -p rfvp-psv \
  --release \
  --features entrypoint \
  --config 'profile.dev.panic="abort"' \
  --config 'profile.release.panic="abort"' \
  -Z build-std=core,alloc,compiler_builtins \
  -Z build-std-features=compiler-builtins-mem
```

This crate only builds the Rust/C platform object. Packaging into a VPK still needs the normal VitaSDK packaging step for your workspace layout.

## Not implemented yet

```text
1. GXM renderer
2. real audio output, current audio vtable is still a stub in c/rfvp_psv_c.c
3. final VPK packaging metadata
4. full game boot/resource-loading flow above rfvp no_std core
```

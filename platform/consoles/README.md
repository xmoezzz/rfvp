# Console executable CI

Build Artifacts calls `console-artifacts.yml` for PS1, PS2, PSP, 3DS, PS3,
PS Vita and Nintendo Switch on
default-branch pushes, version tags and pull requests. Each platform builds in
its own SDK image. Successful default-branch builds attach the executables
to the rolling `pre-release`; version tags attach them to the tagged release.
Pull requests upload Actions artifacts without replacing the rolling release.
The console workflow can also be dispatched independently for executable builds.

| Platform | Release asset |
| --- | --- |
| PS1 | `rfvp-ps1.exe` (PS-X EXE, stock 2 MiB RAM) |
| PS2 | `rfvp-ps2.elf` (ELF32 MIPS) |
| PSP | `EBOOT.PBP` |
| 3DS | `rfvp.3dsx` |
| PS3 | `rfvp-ps3.elf` (ELF64 PowerPC), `rfvp-ps3.self` |
| PS Vita | `rfvp-psv-eboot.bin` (SELF, rename to `eboot.bin` when packaging) |
| Nintendo Switch | `rfvp.nro` |

CI verifies executable signatures and fails if a runtime is missing. It does
not convert games, bundle demo content, create disc images or assemble release
ZIPs. Installing games and creating console disc images are separate processes;
see the platform-specific build scripts and installation documentation.

## Independent builder images

After a successful default-branch executable build, CI publishes
`ghcr.io/OWNER/rfvp-PLATFORM-builder:COMMIT` and `:latest`. Builds do not require
any pre-existing GHCR package: they build their Docker target first and use a
separate Actions layer cache for each platform.

`platform/docker/Dockerfile.console` provides seven independent targets:

- `console-ps1`: PSn00bSDK 0.24, its MIPS GCC, LLVM 23.
- `console-ps2`: ps2dev v2.0.0 with ps2sdk/gsKit, LLVM 23.
- `console-psp`: PSPDEV v20261001, Clang/LLVM 23, cargo-psp 0.2.8.
- `console-3ds`: official devkitPro/devkitARM 20260610 with libctru/citro3d.
- `console-ps3`: official ps3dev nightly-2026-07-26 with PSL1GHT.
- `console-psv`: official full VitaSDK image pinned by digest, cargo-vita 0.2.2.
- `console-switch`: official devkitPro/devkitA64 20260219 with libnx.

The main nightly is pinned in `toolchain.env`; PSP and PS2 use their existing
nightly-2025-03-19 as well. LLVM uses its official 23 branch repository. SDK
versions are pinned, but patch-level system dependencies can change on a clean
image rebuild; commit-tagged published images identify the validated toolchain.

## Local build

From the repository root, with Docker installed:

```sh
docker build --target console-ps1 -f platform/docker/Dockerfile.console \
  -t rfvp-console:ps1 platform
docker run --rm -v "$PWD:/work" -w /work rfvp-console:ps1 \
  bash platform/consoles/build-executable.sh ps1
```

Replace `ps1` with `ps2`, `psp`, `3ds`, `ps3`, `psv` or `switch`. Executables appear in
`dist/consoles/`; intermediate output is isolated under `target/console-PLATFORM`.
For PS1, `build_ps1.sh` without arguments emits only the executable; its existing
optional converted-game argument still builds that game's BIN/CUE locally.
For Vita, `build_vita.sh --executable-only` emits only `eboot.bin`; calling it
without arguments still builds a VPK locally. The manual Consoles workflow
delegates to the same seven-platform build. Wii workflows are unchanged.

# Platforms

The engine itself is `crates/rfvp`. Everything that is specific to one platform lives here, one
directory per platform.

| Directory | Platform | Status | Build |
| --- | --- | --- | --- |
| `switch/` | Nintendo Switch (homebrew `.nro`) | experimental | `switch/build_switch.sh` |
| `vita/` | PlayStation Vita (`.vpk`) | experimental | `vita/build_vita.sh` |
| `psp/` | PSP (`EBOOT.PBP`) | experimental | `psp/build_psp.sh` |
| `ps2/` | PlayStation 2 (`.elf`) | experimental | `make -C ps2/runtime` |
| `3ds/` | Nintendo 3DS (homebrew `.3dsx`) | experimental | `3ds/build_3ds.sh` |
| `ps3/` | PlayStation 3 (`.self`, PSL1GHT) | experimental | `ps3/build_ps3.sh` |
| `wii/` | Wii (homebrew `boot.dol`, libogc) | experimental | `wii/build_wii.sh` |
| `wiiu/` | Wii U (homebrew `.rpx`, wut) | experimental | `wiiu/build_wiiu.sh` |
| `experimental/` | the old PS2, 3DS and PS3 skeletons, superseded by `ps2/`, `3ds/` and `ps3/` | skeletons, untested | see `experimental/README.md` |
| `macos/`, `ios/`, `android/` | launcher apps | supported | `scripts/package_*.sh` |
| `uefi/` | bootable EFI | supported | `scripts/build_uefi.sh` |
| `wasm/` | browser bundle | supported | `scripts/build_wasm.sh` |

`scripts/` holds the build and packaging scripts of the desktop, mobile, UEFI and web builds;
`docker/` the image the console builds run in.

## A console port

Each console directory has the same layout:

```text
<platform>/
  README.md        installing a game, controls, saves, what is not done
  build_<name>.sh  produces the package under target/
  host/            the host crate: implements rfvp's host API (files, rendering,
                   audio, clock, input) on the platform SDK, and the entry point
  *.json           custom Rust target spec, when the built-in one does not fit
  patches/         patched dependencies, when one is needed
```

The console ports run the engine's `no_std` core (`rfvp::RfvpCore`) through the host API in
`crates/rfvp/src/host_api`, so a port only provides platform services: it does not touch engine
code. Rendering goes through `RfvpRenderer`; the shared software rasterizer
(`rfvp::host_api::SoftRaster`) is there for ports without a GPU path.

Every port but the PS1 reads an optional `config.toml` from the game directory (which script to
run and its encoding; see "config.toml" in the top-level `README.md`). The core reads it in
`RfvpCore::boot` (`crates/rfvp/src/game_config.rs`), so a port needs no code for it.

The consoles that need a nightly compiler all use the one pinned in `consoles/toolchain.env`.

The core can be run on a desktop through the same host API, with a real game, by the test
harness in `crates/rfvp/src/no_std_harness.rs` (see `switch/README.md` for how).

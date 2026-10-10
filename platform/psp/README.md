# PSP

The PSP host for the `rfvp` `no_std` core, in its `old_school` profile (paged script loading,
bitmap fonts), rendering with the PSP's GU.

**Status: experimental.** It builds an `EBOOT.PBP`; it has not been run on a PSP or in PPSSPP.

## Building

```bash
./platform/psp/build_psp.sh
```

You need `cargo-psp` and the `nightly-2025-03-19` toolchain with `rust-src` (the script tells you
what is missing).

## Installing a game

Put the EBOOT and the game next to each other (the script and packs are looked up relative to the
working directory). The game must be repacked for the PSP's 32 MiB (64 MiB on later models) and
480x272 screen, and needs a bitmap font, `defualt.tmap`, and an `rfvp.toml` with the `scale`.

## Saves

Saves go in `save/`, as on the other ports. There is no thumbnail: the GU frame is not read back.

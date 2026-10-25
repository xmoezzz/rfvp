# ADR-0007: Console platform support tiers: PS Vita and Switch as primary targets, PS1 as experimental prototype

- Status: Accepted
- Date: 2026-10-25

## Context

rfvp supports a broad range of target environments through its decoupled architecture: an abstract
host API (`crates/rfvp/src/host_api`) interfaces the platform-independent `no_std` core
(`rfvp::RfvpCore`) with platform-specific display, audio, file I/O, and input implementations.

The repository includes ports and host crates for a wide variety of dedicated game consoles, ranging
from modern systems (Nintendo Switch, PlayStation Vita) to legacy platforms (PSP, PS2, PS3, 3DS, Wii,
Wii U, and PlayStation 1).

These systems have radically different hardware architectures, capabilities, and resource profiles:
1. **Modern/Capable Handhelds (Switch and PS Vita):**
   - The Nintendo Switch features an ARM Cortex-A57 CPU, 4 GB of RAM, and a modern Horizon OS.
   - The PlayStation Vita features a quad-core ARM Cortex-A9 CPU, 512 MB of system RAM, and modern
     programmable graphics.
   - Both devices possess ample memory and compute power to run the standard `rfvp::RfvpCore` engine,
     parse full font glyph tables, stream audio and video, read standard `.bin` package archives, and
     honor dynamic configuration via `config.toml`.
2. **Extreme Legacy Hardware (PlayStation 1):**
   - The original PlayStation (1994) features a 33.8 MHz MIPS R3000A CPU without a floating-point unit
     (FPU), only 2 MB of main RAM, 1 MB of VRAM, and slow CD-ROM sector access.
   - Porting to the PS1 necessitated radical departures from standard engine architecture: an ahead-of-time
     (AOT) bytecode-to-C/Rust compiler (`platform/ps1/aot`), stripped hash-based entry lookups (`name_hash`
     replacing string maps in `VfsFile`), 4bpp/16x16 pre-baked glyphs, and bespoke memory layouts (`crates/rfvp-psx`).

Allowing the extreme memory, arithmetic, and storage constraints of the PS1 to dictate the core engine
design would severely degrade code ergonomics, readability, and performance across all other platforms.

## Decision

1. **Primary Mainstream Console Targets (Tier 1):**
   - **PlayStation Vita (`psv`)** and **Nintendo Switch (`switch`)** are officially designated as the
     primary mainstream console targets for rfvp.
   - Engine architectural evolution, host API abstractions, rendering paths, and performance profiling
     prioritize the Vita and Switch ports.
   - Mainstream console builds use the standard `rfvp::RfvpCore` pipeline, support standard game assets
     (`.bin` archives, standard `.hcb` bytecode), and support runtime configuration via `config.toml`.

2. **PlayStation 1 as an Experimental Prototype:**
   - The PS1 port (`rfvp-psx`, `platform/ps1`) is explicitly classified as an experimental prototype
     and research demonstration.
   - The core engine will **not** compromise its design, type system, or abstractions (such as forcing
     fixed-point math or discarding file string paths globally) to accommodate PS1 limitations.
   - Specialized PS1 work (AOT toolchains, stripped hash tables, aggressive asset downscaling) remains
     strictly isolated in PS1-specific crates and build scripts.

3. **Other Legacy Consoles (Skeleton Ports):**
   - Other console targets (PSP, PS2, PS3, Wii, Wii U, 3DS) remain secondary skeleton ports that plug
     into the core host API as hardware and community interest allow, without constraining the primary
     architecture.

## Consequences

- Engineering focus and optimization efforts remain aligned with the platforms where visual novels are
  most viable and enjoyable to play on actual hardware (Vita and Switch).
- The core engine codebase stays clean and expressive, unencumbered by workarounds for severe 1990s
  hardware limitations.
- The PS1 implementation can continue to serve as a fascinating proof-of-concept without imposing
  architectural baggage or technical debt on the wider project.

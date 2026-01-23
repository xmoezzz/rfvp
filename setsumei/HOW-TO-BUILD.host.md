# Host Release Builds (macOS/Linux)

It builds the `rfvp` crate in **release** mode for a requested architecture on the current host.

## Prerequisites

- Rust toolchain (`rustup`, `cargo`) installed and available in `PATH`.
- Project builds successfully on the host.

macOS only:

- Xcode Command Line Tools (for `lipo`).
- Rust targets:
  - `aarch64-apple-darwin`
  - `x86_64-apple-darwin`

Linux cross-builds only:

- `cross` installed (`cargo install cross`).
- A working container runtime (Docker or Podman), as required by `cross`.

## Build

From the repository root:

```bash
./platform/scripts/build_host_release.sh
```

Options:

- `--pkg <crate_pkg>`: cargo package name (default: `rfvp`).
- `--arch <x86_64|aarch64>`: Linux only; request a specific CPU architecture.
- `--target <triple>`: Linux only; request a specific target triple (takes precedence over `--arch`).
- `--out <dir>`: output directory root (default: `dist/host`).

Examples:

```bash
# macOS: produces a universal binary (arm64 + x86_64) via lipo
./platform/scripts/build_host_release.sh

# Linux: build for host arch
./platform/scripts/build_host_release.sh

# Linux: cross-build for aarch64 using cross
./platform/scripts/build_host_release.sh --arch aarch64
```

## Outputs

The script writes the resulting executable under:

- macOS: `dist/host/macos/universal/rfvp`
- Linux: `dist/host/linux/<arch>/rfvp`


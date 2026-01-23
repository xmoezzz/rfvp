# Windows (MSVC) Builds

This document covers `build_windows_msvc.cmd`, which builds the `rfvp` Windows executable using the MSVC toolchain.

## Prerequisites

- Windows 10/11.
- Rust installed via `rustup` (includes `cargo`).
- Visual Studio **Build Tools** or full Visual Studio, with:
  - MSVC toolchain
  - Windows 10/11 SDK
- Rust targets installed:
  - `x86_64-pc-windows-msvc`
  - `aarch64-pc-windows-msvc`

Recommended: run builds from the **Developer Command Prompt for VS** (or equivalent), so the MSVC environment variables are set.

## Build

From the repository root:

```cmd
platform\scripts\build_windows_msvc.cmd x86-64
```

Supported architecture arguments:

- `x86-64`
- `arm64`

Example:

```cmd
platform\scripts\build_windows_msvc.cmd arm64
```

## Outputs

The script produces a single `.exe` binary under the `dist\windows\` directory tree.  

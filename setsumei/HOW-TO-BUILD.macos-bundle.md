# macOS Bundle Build Guide

This repository provides a macOS application bundle (`RFVP.app`) that runs the release `rfvp` binary.

## Requirements

### Host
- macOS (Apple Silicon or Intel)

### Apple tooling
- **hdiutil** (required for DMG packaging; included with macOS)
- **codesign** (optional ad-hoc signing; included with macOS)

### Rust tooling
- **Rust toolchain** (required)
  - Install via rustup (recommended): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
  - Verify: `cargo --version`

## Build

### 1) Build the macOS app bundle
Run:
```bash
./platform/scripts/package_macos_app.sh
```

This script builds the release Rust binary and assembles:
- `RFVP.app` with `Contents/MacOS/RFVP` copied from `target/release/rfvp`.

### 2) Build the DMG
Run:
```bash
./platform/scripts/package_macos_dmg.sh
```

## Outputs
- App bundle: `dist/macos/RFVP.app`
- DMG image: `dist/macos/RFVP.dmg`

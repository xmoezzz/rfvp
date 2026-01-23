# macOS Bundle Build Guide

This repository provides a macOS **launcher application** (`RFVP.app`) that embeds the engine runtime.

## Requirements

### Host
- macOS (Apple Silicon or Intel)

### Apple tooling
- **Xcode** (required)
  - Install via the Mac App Store.
  - Verify: `xcodebuild -version`

- **XcodeGen** (required)
  - Install via Homebrew: `brew install xcodegen`
  - Verify: `xcodegen --version`

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

This script builds the Rust library and the macOS launcher app, then assembles:
- `RFVP.app` with `librfvp.dylib` embedded.

### 2) Build the DMG
Run:
```bash
./platform/scripts/package_macos_dmg.sh
```

## Outputs
- App bundle: `dist/macos/RFVP.app`
- DMG image: `dist/macos/RFVP.dmg`

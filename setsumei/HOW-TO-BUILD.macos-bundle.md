# macOS Bundle Build Guide

This repository provides a macOS SwiftUI launcher bundle (`RFVP.app`) linked to the release `librfvp.dylib`.

## Requirements

### Host
- macOS (Apple Silicon or Intel)

### Apple tooling
- **Xcode command line tools** with `xcodebuild`, `install_name_tool`, and `otool`
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

This script:
- builds the release Rust `cdylib`;
- builds the SwiftUI launcher in `platform/macos/RFVPLauncher`;
- embeds `librfvp.dylib` in `RFVP.app/Contents/Frameworks`;
- rewrites the launcher dependency to `@rpath/librfvp.dylib`;
- applies an ad hoc signature when `codesign` is available.

### 2) Build the DMG
Run:
```bash
./platform/scripts/package_macos_dmg.sh
```

## Outputs
- App bundle: `dist/macos/RFVP.app`
- DMG image: `dist/macos/RFVP.dmg`

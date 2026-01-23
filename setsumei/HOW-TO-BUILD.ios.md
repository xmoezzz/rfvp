# iOS Build Guide (AltStore IPA)

This repository provides an iOS **launcher application** that embeds the engine runtime as an **XCFramework**.

The current iOS packaging flow produces an **unsigned IPA** intended for **AltStore** sideloading.

## Requirements

### Host
- macOS

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

### 1) Build the iOS XCFramework
Run:
```bash
./platform/scripts/build_ios_xcframework.sh
```

This script produces the vendor artifact consumed by the iOS launcher:
- `platform/ios/RFVPLauncher/Vendor/RFVP.xcframework`

### 2) Build the IPA for AltStore
Run:
```bash
./platform/scripts/package_ios_altstore_ipa.sh
```

## Outputs
- XCFramework: `platform/ios/RFVPLauncher/Vendor/RFVP.xcframework`
- IPA (unsigned): `dist/ios/RFVPLauncher.ipa`

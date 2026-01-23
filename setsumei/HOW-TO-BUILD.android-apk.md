# Android Build Guide (Launcher APK)

This repository provides an Android **launcher application** that loads the engine runtime as JNI shared libraries.

The Android build packages `librfvp.so` for:
- `arm64-v8a`
- `x86_64`

## Requirements

### Host
- macOS / Linux / Windows

### Android tooling
- **Android Studio** (recommended) or command-line SDK installation
  - Install Android Studio.
  - In *SDK Manager*, install:
    - Android SDK Platform (matching the project’s `compileSdk`)
    - Android SDK Build-Tools
    - Android NDK
  - Configure environment variables (common setup):
    - `ANDROID_SDK_ROOT=<path to Android/sdk>`
    - `ANDROID_NDK_HOME=<path to Android/sdk/ndk/<version>>`

- **JDK 17**
  - Verify: `java -version`

### Rust tooling
- **Rust toolchain**
  - Verify: `cargo --version`
- Install Android targets:
  - `rustup target add aarch64-linux-android x86_64-linux-android`
- If the build script uses `cargo-ndk`, install it:
  - `cargo install cargo-ndk`

## Build

Run:
```bash
./platform/scripts/package_android_apk.sh
```

## Outputs
Gradle outputs APK(s) under:
- `platform/android/app/build/outputs/apk/`

Typical locations:
- Debug: `platform/android/app/build/outputs/apk/debug/app-debug.apk`
- Release (if built): `platform/android/app/build/outputs/apk/release/app-release.apk`

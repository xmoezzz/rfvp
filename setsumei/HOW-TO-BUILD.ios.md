# iOS Build Guide (AltStore IPA)

This repository provides an iOS **launcher application** that embeds the engine runtime as an **XCFramework**.

The iOS packaging flow intentionally produces an **unsigned IPA** intended for **AltStore sideloading**. We **do not** use Apple’s normal signing/export pipeline (Xcode Archive / provisioning profiles / Developer Program).

---

## What you get (and why)

* The build scripts generate:

  * a **vendor XCFramework** (`RFVP.xcframework`)
  * an **unsigned `.app` packaged into an unsigned `.ipa`**
* An unsigned IPA **cannot** be installed directly via Finder / Apple Configurator / AirDrop.
* AltStore/AltServer will **re-sign the IPA with your Apple ID** and install it onto an iOS device.

---

## Requirements

### Host

* macOS

### Apple tooling

* **Xcode** (required)

  * Install via the Mac App Store.
  * Verify:

    ```bash
    xcodebuild -version
    ```
  * Notes:

    * We use Xcode’s toolchain (SDK, `xcodebuild`, `xcrun`) to compile the launcher.
    * We do **not** require Apple Developer Program ($99/year).

* **XcodeGen** (required)

  * Install:

    ```bash
    brew install xcodegen
    ```
  * Verify:

    ```bash
    xcodegen --version
    ```

### Rust tooling

* **Rust toolchain** (required)

  * Install via rustup (recommended):

    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```
  * Verify:

    ```bash
    cargo --version
    ```

---

## Build

### 1) Build the iOS XCFramework

Run:

```bash
./platform/scripts/build_ios_xcframework.sh
```

This produces:

* `platform/ios/RFVPLauncher/Vendor/RFVP.xcframework`

**Pitfalls**

* If the XCFramework is missing, the launcher build will fail.
* If you change Rust code, rebuild the XCFramework before packaging the IPA.

---

### 2) Build the AltStore IPA (unsigned)

Run:

```bash
./platform/scripts/package_ios_altstore_ipa.sh
```

This produces an **unsigned** IPA:

* `dist/ios/RFVPLauncher.ipa`

**Important**

* This IPA is intentionally **not codesigned** and does **not** contain `embedded.mobileprovision`.
* Installing it directly on a device will fail with:

  * “Unable to install”
  * “Integrity could not be verified”
  * or similar

That is expected. Use **AltStore**.

---

## Install via AltStore

### 1) Install AltServer (macOS) + AltStore (iOS device)

* Install **AltServer** on macOS.
* Connect your iOS device via USB once.
* In Finder for the device, enable **Wi-Fi sync** (recommended for refresh).
* Use AltServer to install **AltStore** onto the device.
* On device: **Trust the developer** (Settings → General → VPN & Device Management).

### 2) Sideload the IPA

Use AltServer to sideload:

* `dist/ios/RFVPLauncher.ipa`

**Common gotchas**

* **Developer Mode (iOS 16+)** must be enabled on the device.
* For Wi-Fi refresh, keep the Mac and iOS device on the **same network**.

---

## AltStore limitations (free Apple ID)

These are Apple account restrictions, not project issues.

* **7-day expiration**
  Apps signed via a free Apple ID expire in ~7 days and must be refreshed via AltStore/AltServer.

* **3-app limit**
  Only a small number of sideloaded apps can be active at once (AltStore supports activate/deactivate workflows).

* **App ID / bundle identifier constraints**
  Apple enforces App ID registration limits. If AltStore fails with bundle identifier / App ID errors, use a different bundle id for local testing.

---

## Outputs

* XCFramework:

  * `platform/ios/RFVPLauncher/Vendor/RFVP.xcframework`
* IPA (unsigned, for AltStore):

  * `dist/ios/RFVPLauncher.ipa`

---

## Troubleshooting

* **Direct install fails (Configurator/Finder/AirDrop)**
  Expected. This IPA is unsigned. Use AltStore.

* **AltStore install fails**
  Usually Apple ID / device / network / App ID limitations. Try:

  * reconnect USB once
  * ensure Developer Mode is enabled (iOS 16+)
  * keep Mac + device on the same network
  * refresh AltStore and retry
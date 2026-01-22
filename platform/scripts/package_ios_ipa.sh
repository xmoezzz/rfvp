#!/usr/bin/env bash
set -euo pipefail

# Build + package RFVP into an iOS .ipa using:
#  - Rust staticlib compiled for device + simulator
#  - XCFramework produced via `xcodebuild -create-xcframework`
#  - XcodeGen to generate a tiny wrapper Xcode project
#  - xcodebuild archive/export to produce the .ipa

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLATFORM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$PLATFORM_DIR/.." && pwd)"

IOS_WRAP_DIR="$PLATFORM_DIR/ios/RFVPLauncher"
DIST_DIR="$ROOT_DIR/dist/ios"
VENDOR_DIR="$IOS_WRAP_DIR/Vendor"

CRATE_PKG="${CRATE_PKG:-rfvp}"
EXPORT_METHOD="${EXPORT_METHOD:-development}"  # development | ad-hoc | app-store | enterprise
ALLOW_PROV="${ALLOW_PROV:-0}"                 # 1 to pass -allowProvisioningUpdates

: "${BUNDLE_ID:?Missing env BUNDLE_ID (e.g. com.yourorg.rfvp)}"
: "${TEAM_ID:?Missing env TEAM_ID (Apple Developer Team ID)}"

mkdir -p "$DIST_DIR" "$VENDOR_DIR"

# Ensure rust targets exist (non-fatal if already installed)
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios-sim >/dev/null 2>&1 || true

# 1) Build Rust static libraries
pushd "$ROOT_DIR" >/dev/null
cargo build --release -p "$CRATE_PKG" --target aarch64-apple-ios
cargo build --release -p "$CRATE_PKG" --target aarch64-apple-ios-sim
# Intel simulator target is optional; build if toolchain is available.
cargo build --release -p "$CRATE_PKG" --target x86_64-apple-ios-sim || true
popd >/dev/null

LIB_DEVICE="$ROOT_DIR/target/aarch64-apple-ios/release/lib${CRATE_PKG}.a"
LIB_SIM_ARM64="$ROOT_DIR/target/aarch64-apple-ios-sim/release/lib${CRATE_PKG}.a"
LIB_SIM_X64="$ROOT_DIR/target/x86_64-apple-ios-sim/release/lib${CRATE_PKG}.a"

if [[ ! -f "$LIB_DEVICE" ]]; then
  echo "Missing device static library: $LIB_DEVICE" >&2
  exit 1
fi
if [[ ! -f "$LIB_SIM_ARM64" ]]; then
  echo "Missing simulator (arm64) static library: $LIB_SIM_ARM64" >&2
  exit 1
fi

# 2) Create XCFramework
HDR_DIR="$DIST_DIR/headers"
rm -rf "$HDR_DIR"
mkdir -p "$HDR_DIR"
cp "$IOS_WRAP_DIR/Headers/rfvp.h" "$HDR_DIR/"

OUT_XC="$VENDOR_DIR/RFVP.xcframework"
rm -rf "$OUT_XC"

XC_ARGS=(
  -create-xcframework
  -library "$LIB_DEVICE" -headers "$HDR_DIR"
  -library "$LIB_SIM_ARM64" -headers "$HDR_DIR"
)
if [[ -f "$LIB_SIM_X64" ]]; then
  XC_ARGS+=( -library "$LIB_SIM_X64" -headers "$HDR_DIR" )
fi
XC_ARGS+=( -output "$OUT_XC" )

xcodebuild "${XC_ARGS[@]}"

# 3) Generate .xcodeproj using XcodeGen
if ! command -v xcodegen >/dev/null 2>&1; then
  echo "xcodegen not found. Install with: brew install xcodegen" >&2
  exit 1
fi

pushd "$IOS_WRAP_DIR" >/dev/null
BUNDLE_ID="$BUNDLE_ID" TEAM_ID="$TEAM_ID" xcodegen generate --spec project.yml
popd >/dev/null

# 4) Archive + export ipa
ARCHIVE_PATH="$DIST_DIR/RFVPLauncher.xcarchive"
EXPORT_DIR="$DIST_DIR/export"
EXPORT_PLIST="$DIST_DIR/ExportOptions.plist"

rm -rf "$ARCHIVE_PATH" "$EXPORT_DIR"
mkdir -p "$EXPORT_DIR"

cat > "$EXPORT_PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>method</key>
  <string>${EXPORT_METHOD}</string>
  <key>teamID</key>
  <string>${TEAM_ID}</string>
</dict>
</plist>
EOF

ALLOW_FLAG=()
if [[ "$ALLOW_PROV" == "1" ]]; then
  ALLOW_FLAG=(-allowProvisioningUpdates)
fi

xcodebuild   -project "$IOS_WRAP_DIR/RFVPLauncher.xcodeproj"   -scheme RFVPLauncher   -configuration Release   -destination "generic/platform=iOS"   -archivePath "$ARCHIVE_PATH"   BUNDLE_ID="$BUNDLE_ID" TEAM_ID="$TEAM_ID"   "${ALLOW_FLAG[@]}"   archive

xcodebuild   -exportArchive   -archivePath "$ARCHIVE_PATH"   -exportOptionsPlist "$EXPORT_PLIST"   -exportPath "$EXPORT_DIR"   "${ALLOW_FLAG[@]}"

IPA_PATH="$(find "$EXPORT_DIR" -maxdepth 1 -name '*.ipa' -print -quit || true)"
if [[ -z "$IPA_PATH" ]]; then
  echo "Export succeeded but no .ipa found in: $EXPORT_DIR" >&2
  exit 1
fi

echo "OK: $IPA_PATH"

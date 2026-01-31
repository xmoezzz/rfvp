#!/usr/bin/env bash
set -euo pipefail


SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLATFORM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$PLATFORM_DIR/.." && pwd)"

IOS_DIR="$PLATFORM_DIR/ios/RFVPLauncher"
HDR_DIR="$IOS_DIR/Headers"
VENDOR_DIR="$IOS_DIR/Vendor"
OUT_XCF="$VENDOR_DIR/RFVP.xcframework"

RFVP_CARGO_PKG="${RFVP_CARGO_PKG:-rfvp}"
LIB_NAME="${LIB_NAME:-rfvp}" # produces lib${LIB_NAME}.a

TGT_IOS="aarch64-apple-ios"
TGT_SIM="aarch64-apple-ios-sim"

command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found" >&2; exit 1; }
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found" >&2; exit 1; }
command -v xcodebuild >/dev/null 2>&1 || { echo "ERROR: xcodebuild not found (install Xcode)" >&2; exit 1; }
command -v xcrun >/dev/null 2>&1 || { echo "ERROR: xcrun not found (install Xcode)" >&2; exit 1; }

[[ -d "$IOS_DIR" ]] || { echo "ERROR: Missing iOS launcher dir: $IOS_DIR" >&2; exit 1; }
[[ -d "$HDR_DIR" ]] || { echo "ERROR: Missing headers dir: $HDR_DIR" >&2; exit 1; }
[[ -f "$HDR_DIR/rfvp.h" ]] || { echo "ERROR: Missing header: $HDR_DIR/rfvp.h" >&2; exit 1; }

mkdir -p "$VENDOR_DIR"

# Ensure Rust targets
rustup target add "$TGT_IOS" >/dev/null 2>&1 || true
rustup target add "$TGT_SIM" >/dev/null 2>&1 || true

echo "[ios-xcf] Building Rust static libs..."
pushd "$ROOT_DIR" >/dev/null
cargo build --release -p "$RFVP_CARGO_PKG" --target "$TGT_IOS"
cargo build --release -p "$RFVP_CARGO_PKG" --target "$TGT_SIM"
popd >/dev/null

LIB_IOS_A="$ROOT_DIR/target/$TGT_IOS/release/lib${LIB_NAME}.a"
LIB_SIM_A="$ROOT_DIR/target/$TGT_SIM/release/lib${LIB_NAME}.a"

if [[ ! -f "$LIB_IOS_A" ]]; then
  echo "ERROR: Missing iOS static lib: $LIB_IOS_A" >&2
  echo "Hint: ensure your crate outputs staticlib for iOS (lib${LIB_NAME}.a)." >&2
  exit 1
fi
if [[ ! -f "$LIB_SIM_A" ]]; then
  echo "ERROR: Missing iOS simulator static lib: $LIB_SIM_A" >&2
  exit 1
fi

rm -rf "$OUT_XCF"

echo "[ios-xcf] Creating xcframework..."
xcodebuild -create-xcframework \
  -library "$LIB_IOS_A" -headers "$HDR_DIR" \
  -library "$LIB_SIM_A" -headers "$HDR_DIR" \
  -output "$OUT_XCF"

echo "[ios-xcf] OK: $OUT_XCF"



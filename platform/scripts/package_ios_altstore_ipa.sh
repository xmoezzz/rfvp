#!/usr/bin/env bash
set -euo pipefail


SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLATFORM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$PLATFORM_DIR/.." && pwd)"

IOS_DIR="$PLATFORM_DIR/ios/RFVPLauncher"
SPEC="$IOS_DIR/project.yml"
VENDOR="$IOS_DIR/Vendor/RFVP.xcframework"

DIST_DIR="$ROOT_DIR/dist/ios"
BUILD_DIR="$DIST_DIR/build"
DERIVED="$BUILD_DIR/DerivedData"
PAYLOAD="$BUILD_DIR/Payload"

SCHEME="${SCHEME:-RFVPLauncher}"
CONFIG="${CONFIG:-Release}"

mkdir -p "$DIST_DIR" "$BUILD_DIR"

command -v xcodegen >/dev/null 2>&1 || { echo "ERROR: xcodegen not found. Install: brew install xcodegen" >&2; exit 1; }
command -v xcodebuild >/dev/null 2>&1 || { echo "ERROR: xcodebuild not found (install Xcode)" >&2; exit 1; }

if [[ ! -f "$SPEC" ]]; then
  echo "ERROR: Missing XcodeGen spec: $SPEC" >&2
  exit 1
fi

if [[ ! -d "$VENDOR" ]]; then
  echo "[ios] Missing $VENDOR, building RFVP.xcframework..."
  "$SCRIPT_DIR/build_ios_xcframework.sh"
fi

[[ -d "$VENDOR" ]] || { echo "ERROR: RFVP.xcframework still missing after build: $VENDOR" >&2; exit 1; }


echo "[ios] Generating Xcode project..."
pushd "$IOS_DIR" >/dev/null
xcodegen generate --spec "$SPEC"
popd >/dev/null

XCODEPROJ="$IOS_DIR/$SCHEME.xcodeproj"
if [[ ! -d "$XCODEPROJ" ]]; then
  echo "ERROR: Missing generated Xcode project: $XCODEPROJ" >&2
  exit 1
fi

rm -rf "$DERIVED" "$PAYLOAD"
mkdir -p "$DERIVED" "$PAYLOAD"

echo "[ios] Building (unsigned)..."
xcodebuild   -project "$XCODEPROJ"   -scheme "$SCHEME"   -configuration "$CONFIG"   -sdk iphoneos   -derivedDataPath "$DERIVED"   CODE_SIGNING_ALLOWED=NO   CODE_SIGNING_REQUIRED=NO   CODE_SIGN_IDENTITY=""   build

APP_PATH="$DERIVED/Build/Products/$CONFIG-iphoneos/RFVP.app"
if [[ ! -d "$APP_PATH" ]]; then
  APP_PATH="$DERIVED/Build/Products/$CONFIG-iphoneos/$SCHEME.app"
fi
[[ -d "$APP_PATH" ]] || { echo "ERROR: Built app not found." >&2; exit 1; }

cp -R "$APP_PATH" "$PAYLOAD/"

IPA_OUT="$DIST_DIR/RFVPLauncher.ipa"
rm -f "$IPA_OUT"
pushd "$BUILD_DIR" >/dev/null
zip -r -q "$IPA_OUT" Payload
popd >/dev/null

echo "OK: $IPA_OUT"

#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLATFORM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$PLATFORM_DIR/.." && pwd)"

IOS_DIR="$PLATFORM_DIR/ios/RFVPLauncher"
SPEC="$IOS_DIR/project.yml"
VENDOR="$IOS_DIR/Vendor/RFVP.xcframework"

DIST_DIR="$ROOT_DIR/dist/ios"
BUILD_DIR="$DIST_DIR/build-sim"
DERIVED="$BUILD_DIR/DerivedData"

SCHEME="${SCHEME:-RFVPLauncher}"
CONFIG="${CONFIG:-Debug}"

# Simulator selection
SIM_NAME="${SIM_NAME:-iPhone 15}"
SIM_UDID="${SIM_UDID:-}"  # optional; if set, overrides SIM_NAME

mkdir -p "$DIST_DIR" "$BUILD_DIR"

command -v xcodegen >/dev/null 2>&1 || { echo "ERROR: xcodegen not found. Install: brew install xcodegen" >&2; exit 1; }
command -v xcodebuild >/dev/null 2>&1 || { echo "ERROR: xcodebuild not found" >&2; exit 1; }
command -v xcrun >/dev/null 2>&1 || { echo "ERROR: xcrun not found" >&2; exit 1; }

if [[ ! -f "$SPEC" ]]; then
  echo "ERROR: Missing XcodeGen spec: $SPEC" >&2
  exit 1
fi

if [[ ! -d "$VENDOR" ]]; then
  echo "ERROR: Missing $VENDOR. Build it first (needs simulator slice for sim builds)." >&2
  exit 1
fi

# Avoid apple metadata files
export COPYFILE_DISABLE=1

resolve_target_udid() {
  if [[ -n "${SIM_UDID:-}" ]]; then
    echo "$SIM_UDID"
    return 0
  fi

  # Pick the first available device line matching exact "SIM_NAME ("
  # Example line:
  #   iPhone 15 (E3D0....-....) (Shutdown)
  local line
  line="$(xcrun simctl list devices available | grep -F "  $SIM_NAME (" | head -n 1 || true)"
  if [[ -z "$line" ]]; then
    # Fallback: looser match (in case indentation differs)
    line="$(xcrun simctl list devices available | grep -F "$SIM_NAME (" | head -n 1 || true)"
  fi

  if [[ -z "$line" ]]; then
    echo "ERROR: Cannot find an available simulator device named: $SIM_NAME" >&2
    echo "[ios-sim] Available devices:" >&2
    xcrun simctl list devices available >&2
    return 1
  fi

  echo "$line" | sed -E 's/.*\(([0-9A-F-]+)\).*/\1/'
}

TARGET_UDID="$(resolve_target_udid)"
[[ -n "$TARGET_UDID" ]] || { echo "ERROR: Failed to resolve TARGET_UDID" >&2; exit 1; }

echo "[ios-sim] Target simulator name: $SIM_NAME"
echo "[ios-sim] Target simulator UDID: $TARGET_UDID"

# Bring Simulator UI to the target device (doesn't boot by itself)
open -a Simulator --args -CurrentDeviceUDID "$TARGET_UDID" >/dev/null 2>&1 || true

# Boot + wait for THIS UDID (prevents xcodebuild from booting a different one)
xcrun simctl boot "$TARGET_UDID" >/dev/null 2>&1 || true
echo "[ios-sim] Waiting for simulator boot to finish..."
xcrun simctl bootstatus "$TARGET_UDID" -b

BOOTED_INFO="$(xcrun simctl list devices | awk -v u="$TARGET_UDID" '
  $0 ~ /^--/ { rt=$0 }
  index($0, u) { print rt " " $0; exit }
')"
echo "[ios-sim] Target simulator info: $BOOTED_INFO"

echo "[ios-sim] Generating Xcode project..."
pushd "$IOS_DIR" >/dev/null
xcodegen generate --spec "$SPEC"
popd >/dev/null

XCODEPROJ="$IOS_DIR/$SCHEME.xcodeproj"
if [[ ! -d "$XCODEPROJ" ]]; then
  echo "ERROR: Missing generated Xcode project: $XCODEPROJ" >&2
  exit 1
fi

rm -rf "$DERIVED"
mkdir -p "$DERIVED"

DEST="id=$TARGET_UDID"
echo "[ios-sim] Building ($CONFIG, iphonesimulator) for destination: $DEST"
xcodebuild \
  -project "$XCODEPROJ" \
  -scheme "$SCHEME" \
  -configuration "$CONFIG" \
  -sdk iphonesimulator \
  -destination "$DEST" \
  -derivedDataPath "$DERIVED" \
  build

APP_PATH="$DERIVED/Build/Products/$CONFIG-iphonesimulator/RFVP.app"
if [[ ! -d "$APP_PATH" ]]; then
  APP_PATH="$DERIVED/Build/Products/$CONFIG-iphonesimulator/$SCHEME.app"
fi

if [[ ! -d "$APP_PATH" ]]; then
  echo "ERROR: Built app not found (iphonesimulator)." >&2
  echo "[ios-sim] Available .app under Products:" >&2
  find "$DERIVED/Build/Products" -maxdepth 3 -name "*.app" -print >&2 || true
  exit 1
fi

BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP_PATH/Info.plist")"

echo "[ios-sim] Installing $BUNDLE_ID to $TARGET_UDID ..."
xcrun simctl install "$TARGET_UDID" "$APP_PATH"

echo "[ios-sim] Verifying install..."
xcrun simctl listapps "$TARGET_UDID" | grep -i "$BUNDLE_ID" >/dev/null || {
  echo "ERROR: App not found after install." >&2
  echo "[ios-sim] Installed apps snapshot:" >&2
  xcrun simctl listapps "$TARGET_UDID" >&2 || true
  exit 1
}

echo "[ios-sim] Launching: $BUNDLE_ID"
xcrun simctl launch "$TARGET_UDID" "$BUNDLE_ID" >/dev/null

echo "OK: installed and launched on simulator: $BUNDLE_ID"
echo "APP: $APP_PATH"
echo "UDID: $TARGET_UDID"

#!/usr/bin/env bash
set -euo pipefail

# Resolve repo root: script is at platform/scripts/
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLATFORM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$PLATFORM_DIR/.." && pwd)"

APP_NAME="${APP_NAME:-RFVP}"
CRATE_PKG="${CRATE_PKG:-rfvp}"

MAC_TEMPLATE_APP="$PLATFORM_DIR/macos/${APP_NAME}.app"
DIST_DIR="$ROOT_DIR/dist/macos"
BUILD_DIR="$DIST_DIR/build"
DMG_STAGE="$DIST_DIR/dmg_stage"
DMG_OUT="$DIST_DIR/${APP_NAME}.dmg"

# Optional: set VERSION to patch DMG filename or Info.plist externally
VERSION="${VERSION:-}"

# Targets
TGT_ARM64="aarch64-apple-darwin"
TGT_X64="x86_64-apple-darwin"

mkdir -p "$BUILD_DIR" "$DIST_DIR"

if [[ ! -d "$MAC_TEMPLATE_APP" ]]; then
  echo "Missing macOS bundle template: $MAC_TEMPLATE_APP" >&2
  exit 1
fi

if [[ ! -f "$MAC_TEMPLATE_APP/Contents/Info.plist" ]]; then
  echo "Missing Info.plist: $MAC_TEMPLATE_APP/Contents/Info.plist" >&2
  exit 1
fi

if [[ ! -s "$MAC_TEMPLATE_APP/Contents/Resources/${APP_NAME}.icns" ]]; then
  echo "Missing or empty icon: $MAC_TEMPLATE_APP/Contents/Resources/${APP_NAME}.icns" >&2
  exit 1
fi

# 1) Build two arch binaries
pushd "$ROOT_DIR" >/dev/null
cargo build --release -p "$CRATE_PKG" --target "$TGT_ARM64"
cargo build --release -p "$CRATE_PKG" --target "$TGT_X64"
popd >/dev/null

BIN_ARM64="$ROOT_DIR/target/$TGT_ARM64/release/$CRATE_PKG"
BIN_X64="$ROOT_DIR/target/$TGT_X64/release/$CRATE_PKG"

if [[ ! -f "$BIN_ARM64" || ! -f "$BIN_X64" ]]; then
  echo "Build output missing. Expected:" >&2
  echo "  $BIN_ARM64" >&2
  echo "  $BIN_X64" >&2
  exit 1
fi

# 2) Create universal binary
UNIVERSAL_BIN="$BUILD_DIR/$CRATE_PKG"
lipo -create "$BIN_ARM64" "$BIN_X64" -output "$UNIVERSAL_BIN"
chmod +x "$UNIVERSAL_BIN"

# 3) Assemble .app (copy template -> build area)
APP_OUT="$BUILD_DIR/${APP_NAME}.app"
rm -rf "$APP_OUT"
cp -R "$MAC_TEMPLATE_APP" "$APP_OUT"

# Put executable in Contents/MacOS/rfvp (or your chosen name)
DEST_BIN="$APP_OUT/Contents/MacOS/$CRATE_PKG"
cp "$UNIVERSAL_BIN" "$DEST_BIN"
chmod +x "$DEST_BIN"

# Ensure config.toml exists next to executable
if [[ ! -f "$APP_OUT/Contents/MacOS/config.toml" ]]; then
  echo "RFVP_LAUNCHER = true" > "$APP_OUT/Contents/MacOS/config.toml"
fi

# 4) (Optional) strip quarantine attributes
xattr -cr "$APP_OUT" || true

# 5) Create DMG staging: app + /Applications symlink
rm -rf "$DMG_STAGE"
mkdir -p "$DMG_STAGE"
cp -R "$APP_OUT" "$DMG_STAGE/"
ln -s /Applications "$DMG_STAGE/Applications"

# 6) Build DMG (read-only compressed)
rm -f "$DMG_OUT"
VOLNAME="${APP_NAME}"
if [[ -n "$VERSION" ]]; then
  DMG_OUT="$DIST_DIR/${APP_NAME}-${VERSION}.dmg"
fi

hdiutil create -volname "$VOLNAME" -srcfolder "$DMG_STAGE" -ov -format UDZO "$DMG_OUT"

echo "OK: $DMG_OUT"

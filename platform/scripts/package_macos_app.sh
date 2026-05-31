#!/usr/bin/env bash
set -euo pipefail


ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MACOS_DIR="${ROOT_DIR}/platform/macos/RFVPLauncher"
DIST_DIR="${ROOT_DIR}/dist/macos"
OUT_APP="${DIST_DIR}/RFVP.app"

RFVP_CARGO_PKG="${RFVP_CARGO_PKG:-rfvp}"
RFVP_BIN_NAME="${RFVP_BIN_NAME:-rfvp}"
APP_EXECUTABLE_NAME="${APP_EXECUTABLE_NAME:-RFVP}"
APP_BUNDLE_ID="${APP_BUNDLE_ID:-org.rfvp.RFVP}"
APP_VERSION="${APP_VERSION:-}"

if [[ -z "${APP_VERSION}" ]]; then
  APP_VERSION="$(
    awk '
      /^\[package\]$/ { in_package = 1; next }
      /^\[/ { in_package = 0 }
      in_package && $1 == "version" {
        gsub(/"/, "", $3)
        print $3
        exit
      }
    ' "${ROOT_DIR}/crates/rfvp/Cargo.toml"
  )"
fi
[[ -n "${APP_VERSION}" ]] || { echo "ERROR: Missing package version in ${ROOT_DIR}/crates/rfvp/Cargo.toml"; exit 1; }

mkdir -p "${DIST_DIR}"

command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found"; exit 1; }

echo "[macos] Building ${RFVP_BIN_NAME} ..."
pushd "${ROOT_DIR}" >/dev/null
cargo build --release -p "${RFVP_CARGO_PKG}"
popd >/dev/null

BIN_PATH="${ROOT_DIR}/target/release/${RFVP_BIN_NAME}"
if [[ ! -f "${BIN_PATH}" ]]; then
  echo "ERROR: Missing ${BIN_PATH}"
  echo "Hint: ensure macOS build produces a release binary named ${RFVP_BIN_NAME}."
  exit 1
fi

rm -rf "${OUT_APP}"
mkdir -p "${OUT_APP}/Contents/MacOS" "${OUT_APP}/Contents/Resources"

cp -f "${BIN_PATH}" "${OUT_APP}/Contents/MacOS/${APP_EXECUTABLE_NAME}"
chmod +x "${OUT_APP}/Contents/MacOS/${APP_EXECUTABLE_NAME}"

ICON_PATH="${MACOS_DIR}/RFVPLauncher/Resources/RFVP.icns"
ICON_PLIST_ENTRY=""
if [[ -f "${ICON_PATH}" ]]; then
  cp -f "${ICON_PATH}" "${OUT_APP}/Contents/Resources/RFVP.icns"
  ICON_PLIST_ENTRY=$'  <key>CFBundleIconFile</key>\n  <string>RFVP</string>\n'
fi

cat > "${OUT_APP}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>RFVP</string>
  <key>CFBundleExecutable</key>
  <string>${APP_EXECUTABLE_NAME}</string>
  <key>CFBundleIdentifier</key>
  <string>${APP_BUNDLE_ID}</string>
${ICON_PLIST_ENTRY}  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>RFVP</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${APP_VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${APP_VERSION}</string>
  <key>LSMinimumSystemVersion</key>
  <string>10.13</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

if command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - --timestamp=none --deep "${OUT_APP}"
fi

echo "[macos] OK: ${OUT_APP}"

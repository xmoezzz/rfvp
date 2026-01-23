#!/usr/bin/env bash
set -euo pipefail


ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MACOS_DIR="${ROOT_DIR}/platform/macos/RFVPLauncher"
VENDOR_DIR="${MACOS_DIR}/Vendor"
DIST_DIR="${ROOT_DIR}/dist/macos"
DERIVED_DIR="${DIST_DIR}/DerivedData"

RFVP_CARGO_PKG="${RFVP_CARGO_PKG:-rfvp}"
SCHEME="${SCHEME:-RFVPLauncher}"
CONFIG="${CONFIG:-Release}"

mkdir -p "${VENDOR_DIR}" "${DIST_DIR}"

command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found"; exit 1; }
command -v xcodebuild >/dev/null 2>&1 || { echo "ERROR: xcodebuild not found (install Xcode)"; exit 1; }
command -v xcodegen >/dev/null 2>&1 || { echo "ERROR: xcodegen not found. Install: brew install xcodegen"; exit 1; }

echo "[macos] Building librfvp.dylib ..."
pushd "${ROOT_DIR}" >/dev/null
cargo build --release -p "${RFVP_CARGO_PKG}"
popd >/dev/null

DYLIB_PATH="${ROOT_DIR}/target/release/librfvp.dylib"
if [[ ! -f "${DYLIB_PATH}" ]]; then
  echo "ERROR: Missing ${DYLIB_PATH}"
  echo "Hint: ensure macOS build produces a cdylib named librfvp.dylib."
  exit 1
fi

cp -f "${DYLIB_PATH}" "${VENDOR_DIR}/librfvp.dylib"

# Ensure a relocatable install name for rpath loading.
install_name_tool -id "@rpath/librfvp.dylib" "${VENDOR_DIR}/librfvp.dylib" || true

echo "[macos] Generating Xcode project ..."
pushd "${MACOS_DIR}" >/dev/null
xcodegen generate --spec project.yml
popd >/dev/null

XCODEPROJ="${MACOS_DIR}/${SCHEME}.xcodeproj"
[[ -d "${XCODEPROJ}" ]] || { echo "ERROR: Missing generated ${XCODEPROJ}"; exit 1; }

rm -rf "${DERIVED_DIR}"
mkdir -p "${DERIVED_DIR}"

echo "[macos] Building .app ..."
xcodebuild   -project "${XCODEPROJ}"   -scheme "${SCHEME}"   -configuration "${CONFIG}"   -derivedDataPath "${DERIVED_DIR}"   build

APP_PATH="${DERIVED_DIR}/Build/Products/${CONFIG}/RFVP.app"
if [[ ! -d "${APP_PATH}" ]]; then
  # Fallback to scheme-based naming
  APP_PATH="${DERIVED_DIR}/Build/Products/${CONFIG}/${SCHEME}.app"
fi
[[ -d "${APP_PATH}" ]] || { echo "ERROR: Built app not found"; exit 1; }

FW_DIR="${APP_PATH}/Contents/Frameworks"
mkdir -p "${FW_DIR}"
cp -f "${VENDOR_DIR}/librfvp.dylib" "${FW_DIR}/librfvp.dylib"

# Ad-hoc sign for local testing. Replace '-' with a real identity for distribution.
codesign --force --sign - --timestamp=none "${FW_DIR}/librfvp.dylib" || true
codesign --force --sign - --timestamp=none --deep "${APP_PATH}" || true

OUT_APP="${DIST_DIR}/RFVP.app"
rm -rf "${OUT_APP}"
cp -R "${APP_PATH}" "${OUT_APP}"

echo "[macos] OK: ${OUT_APP}"

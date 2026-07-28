#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MACOS_PROJECT_DIR="${ROOT_DIR}/platform/macos/RFVPLauncher"
XCODE_PROJECT="${MACOS_PROJECT_DIR}/RFVPLauncher.xcodeproj"
VENDOR_DIR="${MACOS_PROJECT_DIR}/Vendor"
DIST_DIR="${ROOT_DIR}/dist/macos"
OUT_APP="${DIST_DIR}/RFVP.app"
BUILD_ROOT="${ROOT_DIR}/target/macos-launcher-build"
RUST_TARGET_ARM64="aarch64-apple-darwin"
RUST_TARGET_X86_64="x86_64-apple-darwin"
RUST_DYLIB_ARM64="${ROOT_DIR}/target/${RUST_TARGET_ARM64}/release/librfvp.dylib"
RUST_DYLIB_X86_64="${ROOT_DIR}/target/${RUST_TARGET_X86_64}/release/librfvp.dylib"
UNIVERSAL_DYLIB="${VENDOR_DIR}/librfvp.dylib"

RFVP_CARGO_PKG="${RFVP_CARGO_PKG:-rfvp}"
APP_TARGET="${APP_TARGET:-RFVPLauncher}"

command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found"; exit 1; }
command -v xcodebuild >/dev/null 2>&1 || { echo "ERROR: xcodebuild not found"; exit 1; }
command -v install_name_tool >/dev/null 2>&1 || { echo "ERROR: install_name_tool not found"; exit 1; }
NM_BIN="${NM_BIN:-/usr/bin/nm}"
LIPO_BIN="${LIPO_BIN:-/usr/bin/lipo}"
[[ -x "${NM_BIN}" ]] || { echo "ERROR: Apple nm not found at ${NM_BIN}"; exit 1; }
[[ -x "${LIPO_BIN}" ]] || { echo "ERROR: Apple lipo not found at ${LIPO_BIN}"; exit 1; }
[[ -d "${XCODE_PROJECT}" ]] || { echo "ERROR: Missing ${XCODE_PROJECT}"; exit 1; }

mkdir -p "${DIST_DIR}" "${VENDOR_DIR}"

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"

verify_export() {
  local dylib="$1"
  local arch="$2"
  local symbol="$3"

  if ! "${NM_BIN}" -gU -arch "${arch}" "${dylib}" | awk '{ print $NF }' | grep -Fx "_${symbol}" >/dev/null; then
    echo "ERROR: ${dylib} (${arch}) does not export ${symbol}" >&2
    exit 1
  fi
}

# Remove the final cdylib files so Cargo must relink both target slices from the current source.
rm -f "${RUST_DYLIB_ARM64}" "${RUST_DYLIB_X86_64}" "${UNIVERSAL_DYLIB}"

echo "[macos] Building Rust cdylib for ${RUST_TARGET_ARM64} ..."
pushd "${ROOT_DIR}" >/dev/null
cargo build --release -p "${RFVP_CARGO_PKG}" --lib --target "${RUST_TARGET_ARM64}"
echo "[macos] Building Rust cdylib for ${RUST_TARGET_X86_64} ..."
cargo build --release -p "${RFVP_CARGO_PKG}" --lib --target "${RUST_TARGET_X86_64}"
popd >/dev/null

[[ -f "${RUST_DYLIB_ARM64}" ]] || { echo "ERROR: Missing ${RUST_DYLIB_ARM64}"; exit 1; }
[[ -f "${RUST_DYLIB_X86_64}" ]] || { echo "ERROR: Missing ${RUST_DYLIB_X86_64}"; exit 1; }

verify_export "${RUST_DYLIB_ARM64}" arm64 rfvp_run_entry
verify_export "${RUST_DYLIB_ARM64}" arm64 rfvp_set_text_hidpi_enabled
verify_export "${RUST_DYLIB_X86_64}" x86_64 rfvp_run_entry
verify_export "${RUST_DYLIB_X86_64}" x86_64 rfvp_set_text_hidpi_enabled

echo "[macos] Creating universal librfvp.dylib ..."
"${LIPO_BIN}" -create \
  "${RUST_DYLIB_ARM64}" \
  "${RUST_DYLIB_X86_64}" \
  -output "${UNIVERSAL_DYLIB}"
install_name_tool -id "@rpath/librfvp.dylib" "${UNIVERSAL_DYLIB}"

UNIVERSAL_DYLIB_ARCHS="$("${LIPO_BIN}" -archs "${UNIVERSAL_DYLIB}")"
[[ " ${UNIVERSAL_DYLIB_ARCHS} " == *" arm64 "* && " ${UNIVERSAL_DYLIB_ARCHS} " == *" x86_64 "* ]] || {
  echo "ERROR: Universal dylib does not contain arm64 and x86_64: ${UNIVERSAL_DYLIB_ARCHS}"
  exit 1
}
verify_export "${UNIVERSAL_DYLIB}" arm64 rfvp_run_entry
verify_export "${UNIVERSAL_DYLIB}" arm64 rfvp_set_text_hidpi_enabled
verify_export "${UNIVERSAL_DYLIB}" x86_64 rfvp_run_entry
verify_export "${UNIVERSAL_DYLIB}" x86_64 rfvp_set_text_hidpi_enabled

echo "[macos] Building universal SwiftUI launcher ..."
rm -rf "${BUILD_ROOT}"
xcodebuild \
  -project "${XCODE_PROJECT}" \
  -target "${APP_TARGET}" \
  -configuration Release \
  SYMROOT="${BUILD_ROOT}" \
  OBJROOT="${BUILD_ROOT}/obj" \
  CODE_SIGNING_ALLOWED=NO \
  ARCHS="arm64 x86_64" \
  ONLY_ACTIVE_ARCH=NO \
  build

BUILT_APP="$(find "${BUILD_ROOT}/Release" -maxdepth 1 -type d -name '*.app' -print -quit)"
[[ -n "${BUILT_APP}" && -d "${BUILT_APP}" ]] || {
  echo "ERROR: SwiftUI launcher app was not produced under ${BUILD_ROOT}/Release"
  exit 1
}

rm -rf "${OUT_APP}"
cp -R "${BUILT_APP}" "${OUT_APP}"
mkdir -p "${OUT_APP}/Contents/Frameworks"
cp -f "${UNIVERSAL_DYLIB}" "${OUT_APP}/Contents/Frameworks/librfvp.dylib"
install_name_tool -id "@rpath/librfvp.dylib" "${OUT_APP}/Contents/Frameworks/librfvp.dylib"

APP_EXECUTABLE="$(find "${OUT_APP}/Contents/MacOS" -maxdepth 1 -type f -perm -111 -print -quit)"
[[ -n "${APP_EXECUTABLE}" && -f "${APP_EXECUTABLE}" ]] || {
  echo "ERROR: Missing launcher executable in ${OUT_APP}/Contents/MacOS"
  exit 1
}

APP_ARCHS="$("${LIPO_BIN}" -archs "${APP_EXECUTABLE}")"
[[ " ${APP_ARCHS} " == *" arm64 "* && " ${APP_ARCHS} " == *" x86_64 "* ]] || {
  echo "ERROR: Launcher is not universal: ${APP_ARCHS}"
  exit 1
}

EMBEDDED_DYLIB="${OUT_APP}/Contents/Frameworks/librfvp.dylib"
EMBEDDED_DYLIB_ARCHS="$("${LIPO_BIN}" -archs "${EMBEDDED_DYLIB}")"
[[ " ${EMBEDDED_DYLIB_ARCHS} " == *" arm64 "* && " ${EMBEDDED_DYLIB_ARCHS} " == *" x86_64 "* ]] || {
  echo "ERROR: Embedded librfvp.dylib is not universal: ${EMBEDDED_DYLIB_ARCHS}"
  exit 1
}
verify_export "${EMBEDDED_DYLIB}" arm64 rfvp_run_entry
verify_export "${EMBEDDED_DYLIB}" arm64 rfvp_set_text_hidpi_enabled
verify_export "${EMBEDDED_DYLIB}" x86_64 rfvp_run_entry
verify_export "${EMBEDDED_DYLIB}" x86_64 rfvp_set_text_hidpi_enabled

if command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - --timestamp=none "${EMBEDDED_DYLIB}"
  codesign --force --sign - --timestamp=none --deep "${OUT_APP}"
fi

echo "[macos] OK: ${OUT_APP}"

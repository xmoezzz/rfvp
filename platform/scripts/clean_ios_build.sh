#!/usr/bin/env bash
set -euo pipefail

# Repo root: scripts/... -> repo_root
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PLATFORM_DIR="${ROOT_DIR}/platform"
IOS_DIR="${PLATFORM_DIR}/ios/RFVPLauncher"
DIST_IOS_DIR="${ROOT_DIR}/dist/ios"

SCHEME="${SCHEME:-RFVPLauncher}"

echo "[clean-ios] ROOT_DIR=${ROOT_DIR}"

# Build artifacts produced by our iOS packaging scripts:
# - dist/ios/build (device build)
# - dist/ios/build-sim (sim build)
# - DerivedData/ Payload/ intermediates inside those
# - Generated Xcode project by xcodegen
paths=(
  "${DIST_IOS_DIR}/build"
  "${DIST_IOS_DIR}/build-sim"
  "${IOS_DIR}/${SCHEME}.xcodeproj"
  "${IOS_DIR}/${SCHEME}.xcworkspace"
  "${IOS_DIR}/DerivedData"
)

# Common macOS metadata that may appear after zips/unzips/copies
# (safe to remove, does not affect sources)
# We only delete under platform/ios and dist/ios to avoid touching the whole repo.
meta_roots=(
  "${IOS_DIR}"
  "${DIST_IOS_DIR}"
)

echo "[clean-ios] Removing build artifacts..."
for p in "${paths[@]}"; do
  if [[ -e "${p}" ]]; then
    echo "  rm -rf ${p}"
    rm -rf "${p}"
  fi
done

echo "[clean-ios] Removing macOS metadata under ios/dist..."
for r in "${meta_roots[@]}"; do
  if [[ -d "${r}" ]]; then
    find "${r}" -name ".DS_Store" -type f -print -delete || true
    find "${r}" -name "__MACOSX" -type d -print -prune -exec rm -rf {} + || true
    find "${r}" -name "._*" -type f -print -delete || true
  fi
done

# Optional: delete built RFVP.xcframework (compiled artifact)
# Default is KEEP (because builds depend on it).
if [[ "${CLEAN_VENDOR_XCFRAMEWORK:-0}" == "1" ]]; then
  VENDOR_XC="${IOS_DIR}/Vendor/RFVP.xcframework"
  if [[ -d "${VENDOR_XC}" ]]; then
    echo "[clean-ios] CLEAN_VENDOR_XCFRAMEWORK=1 -> rm -rf ${VENDOR_XC}"
    rm -rf "${VENDOR_XC}"
  fi
fi

echo "[clean-ios] Done."

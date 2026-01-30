#!/usr/bin/env bash
set -euo pipefail

# -----------------------------------------
# clean_platform.sh
# Clean platform build artifacts for iOS/macOS/Android.
#
# Usage:
#   ./clean_platform.sh            # real delete
#   ./clean_platform.sh --dry-run  # print only
# -----------------------------------------

DRY_RUN=0
if [[ "${1-}" == "--dry-run" ]]; then
  DRY_RUN=1
fi

# Resolve repo root as the parent directory of this script.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}" && pwd)"

PLATFORM_DIR="${ROOT}/platform"
if [[ ! -d "${PLATFORM_DIR}" ]]; then
  echo "[clean] platform dir not found: ${PLATFORM_DIR}"
  exit 1
fi

say() { echo "[clean] $*"; }

rm_path() {
  local p="$1"
  if [[ -e "$p" ]]; then
    if [[ "${DRY_RUN}" -eq 1 ]]; then
      say "would remove: $p"
    else
      say "remove: $p"
      rm -rf -- "$p"
    fi
  fi
}

# Remove by glob (safe wrapper)
rm_glob() {
  local base="$1"
  local pat="$2"
  shopt -s nullglob
  local matches=("${base}"/${pat})
  shopt -u nullglob

  for p in "${matches[@]}"; do
    rm_path "$p"
  done
}

say "repo root: ${ROOT}"
say "platform dir: ${PLATFORM_DIR}"
if [[ "${DRY_RUN}" -eq 1 ]]; then
  say "DRY RUN enabled (no files will be deleted)"
fi

# ------------------------
# iOS / macOS (Swift / Xcode)
# ------------------------
# vendor directories (you explicitly mentioned these)
rm_path "${PLATFORM_DIR}/ios/vendor"
rm_path "${PLATFORM_DIR}/macos/vendor"

# Xcode derived/build output (common)
rm_glob "${PLATFORM_DIR}/ios" "DerivedData"
rm_glob "${PLATFORM_DIR}/macos" "DerivedData"
rm_glob "${PLATFORM_DIR}/ios" "build"
rm_glob "${PLATFORM_DIR}/macos" "build"
rm_glob "${PLATFORM_DIR}/ios" "*.xcarchive"
rm_glob "${PLATFORM_DIR}/macos" "*.xcarchive"

# SwiftPM caches (if you use SwiftPM in these subprojects)
rm_glob "${PLATFORM_DIR}/ios" ".build"
rm_glob "${PLATFORM_DIR}/macos" ".build"

# ------------------------
# Android (Gradle)
# ------------------------
# Gradle caches inside the project
rm_path "${PLATFORM_DIR}/android/.gradle"
rm_path "${PLATFORM_DIR}/android/build"
rm_glob "${PLATFORM_DIR}/android" "**/build"
rm_glob "${PLATFORM_DIR}/android" "**/.cxx"
rm_glob "${PLATFORM_DIR}/android" "**/.externalNativeBuild"
rm_glob "${PLATFORM_DIR}/android" "**/intermediates"
rm_glob "${PLATFORM_DIR}/android" "**/outputs"
rm_glob "${PLATFORM_DIR}/android" "**/generated"
rm_glob "${PLATFORM_DIR}/android" "**/tmp"

# JNI libs / packaged .so outputs (you mentioned "two so files")
# These are typical locations; the glob is broad but still restricted under platform/android.
rm_glob "${PLATFORM_DIR}/android" "**/*.so"

# If you produce standalone .so in a known folder, add it here too (optional examples):
rm_path "${PLATFORM_DIR}/android/app/src/main/jniLibs"
rm_path "${PLATFORM_DIR}/android/app/build"

say "done"

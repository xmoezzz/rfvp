#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ANDROID_DIR="${ROOT_DIR}/platform/android"
APP_DIR="${ANDROID_DIR}/app"
JNI_LIBS_DIR="${APP_DIR}/src/main/jniLibs"

echo "[clean-android] ROOT_DIR=${ROOT_DIR}"

# Build artifacts produced by our Android build pipeline:
# - cargo-ndk outputs to app/src/main/jniLibs/<abi>/lib*.so
# - Gradle outputs to app/build/...
# - CMake/NDK intermediates (.cxx)
# - Gradle caches under platform/android/.gradle
paths=(
  "${JNI_LIBS_DIR}"
  "${APP_DIR}/build"
  "${APP_DIR}/.cxx"
  "${ANDROID_DIR}/.cxx"
  "${ANDROID_DIR}/.gradle"
  "${ANDROID_DIR}/build"
)

echo "[clean-android] Removing build artifacts..."
for p in "${paths[@]}"; do
  if [[ -e "${p}" ]]; then
    echo "  rm -rf ${p}"
    rm -rf "${p}"
  fi
done

# Optional: delete Gradle user-home caches for THIS project only (kept by default).
# Note: global ~/.gradle is not touched.
if [[ "${CLEAN_GRADLE_WRAPPER_CACHE:-0}" == "1" ]]; then
  # Gradle wrapper cache may appear under platform/android/.gradle already; global cache not removed.
  echo "[clean-android] CLEAN_GRADLE_WRAPPER_CACHE=1 -> (project-local only) done via .gradle removal above"
fi

echo "[clean-android] Done."

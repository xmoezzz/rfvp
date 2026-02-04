#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ANDROID_DIR="${ROOT_DIR}/platform/android"
APP_DIR="${ANDROID_DIR}/app"
JNI_LIBS_DIR="${APP_DIR}/src/main/jniLibs"

RFVP_CARGO_PKG="${RFVP_CARGO_PKG:-rfvp}"
RFVP_SO_NAME="${RFVP_SO_NAME:-rfvp}"
ANDROID_PLATFORM="${ANDROID_PLATFORM:-28}"
VARIANT="${VARIANT:-debug}"
ABIS="${ABIS:-arm64-v8a x86_64}"

command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found in PATH" >&2; exit 1; }
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found in PATH" >&2; exit 1; }
command -v java >/dev/null 2>&1 || { echo "ERROR: java not found in PATH (JDK required for Gradle)" >&2; exit 1; }

ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
[[ -n "${ANDROID_SDK_ROOT}" ]] || { echo "ERROR: ANDROID_SDK_ROOT (or ANDROID_HOME) is not set" >&2; exit 1; }

ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [[ -z "${ANDROID_NDK_HOME}" ]]; then
  NDK_PARENT="${ANDROID_SDK_ROOT}/ndk"
  if [[ -d "${NDK_PARENT}" ]]; then
    ANDROID_NDK_HOME="$(ls -1 "${NDK_PARENT}" | sort -V | tail -n 1 | awk -v p="${NDK_PARENT}" '{print p "/" $0}')"
  fi
fi
[[ -n "${ANDROID_NDK_HOME}" && -d "${ANDROID_NDK_HOME}" ]] || {
  echo "ERROR: ANDROID_NDK_HOME is not set and no NDK found under ${ANDROID_SDK_ROOT}/ndk/" >&2
  exit 1
}

export ANDROID_NDK_HOME
export CARGO_NDK_PLATFORM="${ANDROID_PLATFORM}"

echo "[android] ANDROID_SDK_ROOT=${ANDROID_SDK_ROOT}"
echo "[android] ANDROID_NDK_HOME=${ANDROID_NDK_HOME}"
echo "[android] CARGO_NDK_PLATFORM=${CARGO_NDK_PLATFORM}"
echo "[android] ABIS=${ABIS}"
echo "[android] JNI_LIBS_DIR=${JNI_LIBS_DIR}"
echo "[android] VARIANT=${VARIANT}"

if ! command -v cargo-ndk >/dev/null 2>&1; then
  echo "[android] Installing cargo-ndk ..."
  cargo install cargo-ndk
fi

need_targets=()
for abi in ${ABIS}; do
  case "${abi}" in
    arm64-v8a) need_targets+=("aarch64-linux-android") ;;
    armeabi-v7a) need_targets+=("armv7-linux-androideabi") ;;
    x86) need_targets+=("i686-linux-android") ;;
    x86_64) need_targets+=("x86_64-linux-android") ;;
    *) echo "ERROR: Unknown ABI: ${abi}" >&2; exit 1 ;;
  esac
done
for t in "${need_targets[@]}"; do
  if ! rustup target list --installed | grep -q "^${t}$"; then
    echo "[android] rustup target add ${t}"
    rustup target add "${t}"
  fi
done

rm -rf "${JNI_LIBS_DIR}"
mkdir -p "${JNI_LIBS_DIR}"

CARGO_PROFILE_ARGS=()
if [[ "${VARIANT}" == "release" ]]; then
  CARGO_PROFILE_ARGS+=(--release)
fi

pushd "${ROOT_DIR}" >/dev/null
echo "[android] Building Rust shared library via cargo ndk ..."
cargo ndk $(for abi in ${ABIS}; do printf -- "-t %s " "${abi}"; done) \
  -o "${JNI_LIBS_DIR}" \
  build ${CARGO_PROFILE_ARGS[@]+"${CARGO_PROFILE_ARGS[@]}"} -p "${RFVP_CARGO_PKG}"
popd >/dev/null

missing=0
for abi in ${ABIS}; do
  so_path="${JNI_LIBS_DIR}/${abi}/lib${RFVP_SO_NAME}.so"
  if [[ ! -f "${so_path}" ]]; then
    echo "ERROR: Missing ${so_path}" >&2
    missing=1
  fi
done
[[ "${missing}" -eq 0 ]] || exit 1

pushd "${ANDROID_DIR}" >/dev/null
if [[ -x "./gradlew" ]]; then
  GRADLE_CMD="./gradlew"
elif command -v gradle >/dev/null 2>&1; then
  GRADLE_CMD="gradle"
else
  echo "ERROR: Neither ./gradlew nor gradle found." >&2
  exit 1
fi

GRADLE_TASK=":app:assembleDebug"
if [[ "${VARIANT}" == "release" ]]; then
  GRADLE_TASK=":app:assembleRelease"
fi

echo "[android] Running Gradle: ${GRADLE_CMD} ${GRADLE_TASK}"
${GRADLE_CMD} ${GRADLE_TASK}

APK_PATH="$(ls -1 "${APP_DIR}/build/outputs/apk/${VARIANT}/"*.apk 2>/dev/null | head -n 1 || true)"
if [[ -n "${APK_PATH}" ]]; then
  echo "[android] APK: ${APK_PATH}"
else
  echo "ERROR: APK not found under app/build/outputs/apk/${VARIANT}/" >&2
  exit 1
fi
popd >/dev/null

echo "[android] Done."

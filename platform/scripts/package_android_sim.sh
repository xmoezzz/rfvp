#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ANDROID_DIR="${ROOT_DIR}/platform/android"
APP_DIR="${ANDROID_DIR}/app"

ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
[[ -n "${ANDROID_SDK_ROOT}" ]] || { echo "ERROR: ANDROID_SDK_ROOT (or ANDROID_HOME) is not set" >&2; exit 1; }

EMULATOR_BIN="${ANDROID_SDK_ROOT}/emulator/emulator"
ADB_BIN="${ANDROID_SDK_ROOT}/platform-tools/adb"
[[ -x "${EMULATOR_BIN}" ]] || { echo "ERROR: emulator not found: ${EMULATOR_BIN}" >&2; exit 1; }
[[ -x "${ADB_BIN}" ]] || { echo "ERROR: adb not found: ${ADB_BIN}" >&2; exit 1; }

# Always debug for simulator unless explicitly overridden
VARIANT="${VARIANT:-debug}"
AVD_NAME="${AVD_NAME:-}"           # optional override
SHOW_LOGCAT="${SHOW_LOGCAT:-1}"    # 1 to tail logs
CONFIG_ABI="${ABI:-}"              # optional override

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APK_SCRIPT="${SCRIPT_DIR}/package_android_apk.sh"
[[ -f "${APK_SCRIPT}" ]] || { echo "ERROR: missing ${APK_SCRIPT}" >&2; exit 1; }

# Resolve package name from manifest (no aapt dependency)
MANIFEST="${APP_DIR}/src/main/AndroidManifest.xml"
PKG="$(grep -o 'package="[^"]*"' -m1 "${MANIFEST}" | sed 's/package="//;s/"//')"
[[ -n "${PKG}" ]] || { echo "ERROR: Failed to parse package name from ${MANIFEST}" >&2; exit 1; }
echo "[emu] package=${PKG}"

# Pick AVD
if [[ -z "${AVD_NAME}" ]]; then
  AVDS=()
  while IFS= read -r line; do
    [[ -n "${line}" ]] && AVDS+=("${line}")
  done < <("${EMULATOR_BIN}" -list-avds 2>/dev/null || true)

  if [[ "${#AVDS[@]}" -eq 0 ]]; then
    echo "ERROR: No AVD found. Create one in Android Studio." >&2
    exit 1
  elif [[ "${#AVDS[@]}" -eq 1 ]]; then
    AVD_NAME="${AVDS[0]}"
  else
    echo "ERROR: Multiple AVDs found. Set AVD_NAME explicitly via AVD_NAME=..." >&2
    printf '  %s\n' "${AVDS[@]}" >&2
    exit 1
  fi
fi
echo "[emu] AVD_NAME=${AVD_NAME}"

# Start emulator if none booted
if ! "${ADB_BIN}" devices | awk 'NR>1 && $1 ~ /^emulator-[0-9]+$/ && $2=="device"{found=1} END{exit(found?0:1)}'; then
  echo "[emu] Starting emulator..."
  nohup "${EMULATOR_BIN}" -avd "${AVD_NAME}" -netdelay none -netspeed full >/tmp/rfvp_emulator.log 2>&1 &
fi

echo "[emu] Waiting for emulator device..."
"${ADB_BIN}" wait-for-device

# Select emulator serial explicitly
SERIAL="$("${ADB_BIN}" devices | awk 'NR>1 && $1 ~ /^emulator-[0-9]+$/ && $2=="device" {print $1; exit}')"
if [[ -z "${SERIAL}" ]]; then
  echo "ERROR: No booted emulator found in 'adb devices'." >&2
  echo "Hint: check /tmp/rfvp_emulator.log" >&2
  exit 1
fi
echo "[emu] SERIAL=${SERIAL}"

echo "[emu] Waiting for boot completed..."
for _ in $(seq 1 180); do
  BOOT="$("${ADB_BIN}" -s "${SERIAL}" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')"
  [[ "${BOOT}" == "1" ]] && break
  sleep 1
done
BOOT="$("${ADB_BIN}" -s "${SERIAL}" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')"
[[ "${BOOT}" == "1" ]] || { echo "ERROR: Emulator did not boot in time" >&2; exit 1; }

# Detect ABI
ABI_DEV="$("${ADB_BIN}" -s "${SERIAL}" shell getprop ro.product.cpu.abi 2>/dev/null | tr -d '\r')"
echo "[emu] device ABI=${ABI_DEV}"

ABI_BUILD="${CONFIG_ABI:-}"
if [[ -z "${ABI_BUILD}" ]]; then
  case "${ABI_DEV}" in
    x86_64) ABI_BUILD="x86_64" ;;
    arm64-v8a|arm64*) ABI_BUILD="arm64-v8a" ;;
    *) echo "ERROR: Unsupported emulator ABI: ${ABI_DEV}" >&2; exit 1 ;;
  esac
fi
echo "[emu] build ABI=${ABI_BUILD}"

# IMPORTANT: Do NOT depend on package_android_apk.sh internals.
# Force debug build by default for emulator testing.
export ABIS="${ABI_BUILD}"
export VARIANT="${VARIANT}"

echo "[emu] Building APK via ${APK_SCRIPT} ..."
bash "${APK_SCRIPT}"

APK_PATH="$(ls -1 "${APP_DIR}/build/outputs/apk/${VARIANT}/"*.apk 2>/dev/null | head -n 1 || true)"
[[ -n "${APK_PATH}" ]] || { echo "ERROR: APK not found after build" >&2; exit 1; }
echo "[emu] APK=${APK_PATH}"

echo "[emu] Installing to ${SERIAL} ..."
"${ADB_BIN}" -s "${SERIAL}" install -r "${APK_PATH}"

echo "[emu] Launching on ${SERIAL} ..."
"${ADB_BIN}" -s "${SERIAL}" shell monkey -p "${PKG}" -c android.intent.category.LAUNCHER 1 >/dev/null

echo "[emu] OK: installed and launched. serial=${SERIAL}"
if [[ "${SHOW_LOGCAT}" == "1" ]]; then
  echo "[emu] Tailing logcat (Ctrl+C to stop)..."
  "${ADB_BIN}" -s "${SERIAL}" logcat -v brief | grep -E "(${PKG}|rfvp|RFVP)" || true
fi

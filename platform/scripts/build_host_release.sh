#!/usr/bin/env bash
set -euo pipefail

# build_host_release.sh
#
# macOS:
#   - build x86_64-apple-darwin + aarch64-apple-darwin
#   - lipo into one universal binary (default behavior; no arch prompt)
#
# Linux:
#   - if target == rustc host: cargo build
#   - else: cross build (requires `cross` installed and Docker available)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

CRATE_PKG="${CRATE_PKG:-rfvp}"   # cargo package name (-p)
BIN_NAME="${BIN_NAME:-rfvp}"     # output binary name
OUT_DIR="${OUT_DIR:-${ROOT_DIR}/dist/host}"

TARGETS=""
PROFILE="release"

usage() {
  cat <<EOF
Usage:
  $0 [--package <pkg>] [--bin <bin>] [--targets <t1,t2,...>] [--out-dir <dir>]

Defaults:
  --package  rfvp
  --bin      rfvp
  --out-dir  <repo>/dist/host

Notes:
  - On macOS, if --targets is omitted, builds universal (x86_64 + aarch64) and lipo.
  - On Linux, if --targets is omitted, builds for host (no --target).
  - On Linux, cross is used when target != rustc host.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --package)
      CRATE_PKG="$2"; shift 2;;
    --bin)
      BIN_NAME="$2"; shift 2;;
    --targets)
      TARGETS="$2"; shift 2;;
    --out-dir)
      OUT_DIR="$2"; shift 2;;
    -h|--help)
      usage; exit 0;;
    *)
      echo "ERROR: Unknown argument: $1" >&2
      usage
      exit 1;;
  esac
done

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "ERROR: Missing command: $1" >&2; exit 1; }
}

need_cmd uname
need_cmd cargo
need_cmd rustc

OS="$(uname -s)"
ARCH="$(uname -m)"
RUST_HOST="$(rustc -vV | awk -F': ' '/^host:/ {print $2}')"

mkdir -p "${OUT_DIR}"

log() { echo "[host-build] $*"; }

build_with_cargo_target() {
  local tgt="$1"
  log "cargo build --${PROFILE} -p ${CRATE_PKG} --target ${tgt}"
  (cd "${ROOT_DIR}" && cargo build --"${PROFILE}" -p "${CRATE_PKG}" --target "${tgt}")
}

build_with_cargo_host() {
  log "cargo build --${PROFILE} -p ${CRATE_PKG}"
  (cd "${ROOT_DIR}" && cargo build --"${PROFILE}" -p "${CRATE_PKG}")
}

build_with_cross_target() {
  local tgt="$1"
  need_cmd cross
  log "cross build --${PROFILE} -p ${CRATE_PKG} --target ${tgt}"
  (cd "${ROOT_DIR}" && cross build --"${PROFILE}" -p "${CRATE_PKG}" --target "${tgt}")
}

copy_bin_from_target_dir() {
  local tgt="$1"
  local src="${ROOT_DIR}/target/${tgt}/${PROFILE}/${BIN_NAME}"
  local dst_dir="${OUT_DIR}/${tgt}"
  local dst="${dst_dir}/${BIN_NAME}"
  [[ -f "${src}" ]] || { echo "ERROR: Build output not found: ${src}" >&2; exit 1; }
  mkdir -p "${dst_dir}"
  cp -f "${src}" "${dst}"
  chmod +x "${dst}" || true
  log "OK: ${dst}"
}

copy_bin_from_host_dir() {
  local src="${ROOT_DIR}/target/${PROFILE}/${BIN_NAME}"
  local dst_dir="${OUT_DIR}/${RUST_HOST}"
  local dst="${dst_dir}/${BIN_NAME}"
  [[ -f "${src}" ]] || { echo "ERROR: Build output not found: ${src}" >&2; exit 1; }
  mkdir -p "${dst_dir}"
  cp -f "${src}" "${dst}"
  chmod +x "${dst}" || true
  log "OK: ${dst}"
}

if [[ "${OS}" == "Darwin" ]]; then
  need_cmd lipo

  # Default: universal build (no arch prompt)
  if [[ -z "${TARGETS}" ]]; then
    TARGETS="aarch64-apple-darwin,x86_64-apple-darwin"
  fi

  IFS=',' read -r -a TGT_ARR <<< "${TARGETS}"

  # If exactly two Apple targets are provided, build both and lipo into universal.
  if [[ "${#TGT_ARR[@]}" -eq 2 ]]; then
    T1="${TGT_ARR[0]}"
    T2="${TGT_ARR[1]}"

    build_with_cargo_target "${T1}"
    build_with_cargo_target "${T2}"

    BIN1="${ROOT_DIR}/target/${T1}/${PROFILE}/${BIN_NAME}"
    BIN2="${ROOT_DIR}/target/${T2}/${PROFILE}/${BIN_NAME}"
    [[ -f "${BIN1}" && -f "${BIN2}" ]] || { echo "ERROR: Missing built binaries for lipo." >&2; exit 1; }

    UNIVERSAL_DIR="${OUT_DIR}/universal-macos"
    mkdir -p "${UNIVERSAL_DIR}"
    UNIVERSAL_BIN="${UNIVERSAL_DIR}/${BIN_NAME}"

    log "lipo -create ${BIN1} ${BIN2} -output ${UNIVERSAL_BIN}"
    lipo -create "${BIN1}" "${BIN2}" -output "${UNIVERSAL_BIN}"
    chmod +x "${UNIVERSAL_BIN}" || true
    log "OK: ${UNIVERSAL_BIN}"
    exit 0
  fi

  # Otherwise: build each target and copy separately.
  for tgt in "${TGT_ARR[@]}"; do
    build_with_cargo_target "${tgt}"
    copy_bin_from_target_dir "${tgt}"
  done
  exit 0
fi

if [[ "${OS}" == "Linux" ]]; then
  # Default: host build
  if [[ -z "${TARGETS}" ]]; then
    build_with_cargo_host
    copy_bin_from_host_dir
    exit 0
  fi

  IFS=',' read -r -a TGT_ARR <<< "${TARGETS}"
  for tgt in "${TGT_ARR[@]}"; do
    if [[ "${tgt}" == "${RUST_HOST}" ]]; then
      build_with_cargo_host
      copy_bin_from_host_dir
    else
      build_with_cross_target "${tgt}"
      copy_bin_from_target_dir "${tgt}"
    fi
  done
  exit 0
fi

echo "ERROR: Unsupported OS: ${OS} (only macOS and Linux are supported)" >&2
exit 1

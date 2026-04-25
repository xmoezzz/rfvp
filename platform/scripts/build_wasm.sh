#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
WASM_DIR="${PROJECT_ROOT}/platform/wasm"
PKG_DIR="${WASM_DIR}/pkg"
DIST_DIR="${PROJECT_ROOT}/dist/wasm"
OUT_ZIP="${DIST_DIR}/rfvp-wasm.zip"
WASM_TARGET="wasm32-unknown-unknown"
RFVP_WASM="${PROJECT_ROOT}/target/${WASM_TARGET}/release/rfvp.wasm"

cd "${PROJECT_ROOT}"

if [[ ! -f "${WASM_DIR}/index.html" ]]; then
  echo "ERROR: missing platform/wasm/index.html" >&2
  exit 1
fi

if [[ ! -f "${WASM_DIR}/main.js" ]]; then
  echo "ERROR: missing platform/wasm/main.js" >&2
  exit 1
fi

if [[ ! -f "${WASM_DIR}/icon/favicon.ico" ]]; then
  echo "WARNING: missing platform/wasm/icon/favicon.ico; package will be built without a favicon file" >&2
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "ERROR: cargo is not available" >&2
  exit 1
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "ERROR: rustup is not available" >&2
  exit 1
fi

if ! rustup target list --installed | grep -qx "${WASM_TARGET}"; then
  echo "ERROR: Rust target ${WASM_TARGET} is not installed" >&2
  echo "Run: rustup target add ${WASM_TARGET}" >&2
  exit 1
fi

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "ERROR: wasm-bindgen is not available" >&2
  echo "Run: cargo install wasm-bindgen-cli" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "ERROR: python3 is not available; it is required to create ${OUT_ZIP}" >&2
  exit 1
fi

echo "==> Building rfvp wasm library"
cargo build -p rfvp \
  --lib \
  --release \
  --target "${WASM_TARGET}" \
  --no-default-features \
  --features wasm

test -f "${RFVP_WASM}"

echo "==> Generating wasm-bindgen web package"
rm -rf "${PKG_DIR}"
mkdir -p "${PKG_DIR}"

wasm-bindgen "${RFVP_WASM}" \
  --target web \
  --out-dir "${PKG_DIR}" \
  --out-name rfvp

if [[ ! -f "${PKG_DIR}/rfvp.js" ]]; then
  echo "ERROR: wasm-bindgen did not generate ${PKG_DIR}/rfvp.js" >&2
  exit 1
fi

if [[ ! -f "${PKG_DIR}/rfvp_bg.wasm" ]]; then
  echo "ERROR: wasm-bindgen did not generate ${PKG_DIR}/rfvp_bg.wasm" >&2
  exit 1
fi

if ! grep -q "start_rfvp_from_directory" "${PKG_DIR}/rfvp.js"; then
  echo "ERROR: generated rfvp.js does not export start_rfvp_from_directory" >&2
  exit 1
fi

echo "==> Packaging web files"
mkdir -p "${DIST_DIR}"
rm -f "${OUT_ZIP}"

python3 - <<'PY' "${WASM_DIR}" "${OUT_ZIP}"
import os
import sys
import zipfile
from pathlib import Path

wasm_dir = Path(sys.argv[1]).resolve()
out_zip = Path(sys.argv[2]).resolve()

exclude_dirs = {".git", "node_modules"}
exclude_files = {".DS_Store"}

with zipfile.ZipFile(out_zip, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    for root, dirs, files in os.walk(wasm_dir):
        dirs[:] = [d for d in dirs if d not in exclude_dirs]
        root_path = Path(root)
        for name in files:
            if name in exclude_files:
                continue
            path = root_path / name
            rel = path.relative_to(wasm_dir)
            zf.write(path, rel.as_posix())
PY

echo "==> Built ${OUT_ZIP}"
echo "==> Local test:"
echo "    cd platform/wasm"
echo "    python3 -m http.server 8000"
echo "    open http://127.0.0.1:8000/"

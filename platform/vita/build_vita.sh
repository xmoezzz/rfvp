#!/usr/bin/env bash
# Build the PlayStation Vita port (rfvp-psv) into a VPK.
#
# Needs VitaSDK (VITASDK, default /usr/local/vitasdk), cargo-vita and rustup.
# Output: target/rfvp-psv/psv/rfvp.vpk
#
# The game goes in ux0:data/rfvp/game/ on the memory card; see platform/vita/README.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../consoles/toolchain.env
source "$ROOT_DIR/platform/consoles/toolchain.env"

export VITASDK="${VITASDK:-/usr/local/vitasdk}"
export PATH="$VITASDK/bin:$PATH"
TOOLCHAIN="${RFVP_CONSOLE_NIGHTLY}"
OUT_DIR="$ROOT_DIR/target/rfvp-psv/psv"

command -v arm-vita-eabi-gcc >/dev/null 2>&1 || { echo "ERROR: VitaSDK not found at $VITASDK" >&2; exit 1; }
command -v cargo-vita >/dev/null 2>&1 || { echo "ERROR: cargo-vita not found (cargo install cargo-vita)" >&2; exit 1; }
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found" >&2; exit 1; }
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rust-src --no-self-update

cd "$ROOT_DIR"
cargo "+$TOOLCHAIN" vita build vpk -p rfvp-psv --release --features entrypoint \
  --config 'profile.dev.panic="abort"' \
  --config 'profile.release.panic="abort"' \
  -Z build-std=core,alloc,compiler_builtins \
  -Z build-std-features=compiler-builtins-mem

VPK="${CARGO_TARGET_DIR:-$ROOT_DIR/target}/armv7-sony-vita-newlibeabihf/release/rfvp-psv.vpk"
[[ -f "$VPK" ]] || { echo "ERROR: build produced no $VPK" >&2; exit 1; }

mkdir -p "$OUT_DIR"
cp "$VPK" "$OUT_DIR/rfvp.vpk"
echo "[psv] $OUT_DIR/rfvp.vpk"

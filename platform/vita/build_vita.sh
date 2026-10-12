#!/usr/bin/env bash
# Build the PlayStation Vita port (rfvp-psv) into a VPK, or only its executable.
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
BUILD_KIND=vpk
EXTENSION=vpk
OUTPUT_NAME=rfvp.vpk
case "${1:-}" in
  "") ;;
  --executable-only) BUILD_KIND=eboot; EXTENSION=self; OUTPUT_NAME=eboot.bin ;;
  *) echo "usage: build_vita.sh [--executable-only]" >&2; exit 1 ;;
esac

command -v arm-vita-eabi-gcc >/dev/null 2>&1 || { echo "ERROR: VitaSDK not found at $VITASDK" >&2; exit 1; }
command -v cargo-vita >/dev/null 2>&1 || { echo "ERROR: cargo-vita not found (cargo install cargo-vita)" >&2; exit 1; }
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found" >&2; exit 1; }
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rust-src --no-self-update

cd "$ROOT_DIR"
cargo "+$TOOLCHAIN" vita build "$BUILD_KIND" -p rfvp-psv --release --features entrypoint \
  --config 'profile.dev.panic="abort"' \
  --config 'profile.release.panic="abort"' \
  -Z build-std=core,alloc,compiler_builtins \
  -Z build-std-features=compiler-builtins-mem

ARTIFACT="${CARGO_TARGET_DIR:-$ROOT_DIR/target}/armv7-sony-vita-newlibeabihf/release/rfvp-psv.$EXTENSION"
[[ -f "$ARTIFACT" ]] || { echo "ERROR: build produced no $ARTIFACT" >&2; exit 1; }

mkdir -p "$OUT_DIR"
cp "$ARTIFACT" "$OUT_DIR/$OUTPUT_NAME"
echo "[psv] $OUT_DIR/$OUTPUT_NAME"

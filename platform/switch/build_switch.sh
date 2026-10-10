#!/usr/bin/env bash
# Build the Nintendo Switch port (rfvp-horizon) into an NRO.
#
# Needs devkitPro (elf2nro, nacptool; DEVKITPRO, default /opt/devkitpro) with devkitA64, which
# also compiles the C Vorbis decoder, and rustup. Output: target/rfvp-switch/switch/rfvp.nro
#
# The game goes in sdmc:/switch/rfvp/game/ on the SD card; see platform/switch/README.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../consoles/toolchain.env
source "$ROOT_DIR/platform/consoles/toolchain.env"

export DEVKITPRO="${DEVKITPRO:-/opt/devkitpro}"
TOOLCHAIN="${RFVP_CONSOLE_NIGHTLY}"
TARGET_NAME="aarch64-nintendo-switch-rfvp"
# The stock aarch64-nintendo-switch-freestanding target allows dynamic libraries, so rustc also
# tries to build rfvp's `cdylib` (used by the desktop and mobile packages) and fails for lack of
# an allocator. The same target with `dynamic-linking` off makes rustc skip it.
TARGET_JSON="$ROOT_DIR/platform/switch/$TARGET_NAME.json"
OUT_DIR="$ROOT_DIR/target/rfvp-switch/switch"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT_DIR/platform/switch/host/Cargo.toml" | head -1)"

for tool in "$DEVKITPRO/tools/bin/elf2nro" "$DEVKITPRO/tools/bin/nacptool" \
            "$DEVKITPRO/devkitA64/bin/aarch64-none-elf-gcc"; do
  [[ -x "$tool" ]] || { echo "ERROR: missing $tool (install devkitPro with devkitA64)" >&2; exit 1; }
done
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found" >&2; exit 1; }
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rust-src --no-self-update

cd "$ROOT_DIR"
cargo "+$TOOLCHAIN" build -p rfvp-horizon --release --features entrypoint \
  --target "$TARGET_JSON" \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins \
  -Zbuild-std-features=compiler-builtins-mem

ELF="${CARGO_TARGET_DIR:-$ROOT_DIR/target}/$TARGET_NAME/release/rfvp-horizon"
[[ -f "$ELF" ]] || { echo "ERROR: build produced no $ELF" >&2; exit 1; }

mkdir -p "$OUT_DIR"
"$DEVKITPRO/tools/bin/nacptool" --create "rfvp" "rfvp" "${VERSION:-0.1.0}" "$OUT_DIR/rfvp.nacp"
"$DEVKITPRO/tools/bin/elf2nro" "$ELF" "$OUT_DIR/rfvp.nro" --nacp="$OUT_DIR/rfvp.nacp"
echo "[switch] $OUT_DIR/rfvp.nro"

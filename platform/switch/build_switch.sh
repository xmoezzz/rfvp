#!/usr/bin/env bash
# Build the Nintendo Switch port into an NRO: the engine (platform/switch/host) as a static
# library, linked into the libnx runtime (platform/switch/runtime/main.c).
#
# Needs devkitPro with devkitA64, libnx and switch-tools (DEVKITPRO, default /opt/devkitpro),
# and rustup. Output: target/rfvp-switch/switch/rfvp.nro
#
# The game goes in sdmc:/switch/rfvp/game/ on the SD card; see platform/switch/README.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../consoles/toolchain.env
source "$ROOT_DIR/platform/consoles/toolchain.env"

export DEVKITPRO="${DEVKITPRO:-/opt/devkitpro}"
DEVKITA64="$DEVKITPRO/devkitA64"
TOOLCHAIN="${RFVP_CONSOLE_NIGHTLY}"
TARGET_NAME="aarch64-nintendo-switch-rfvp"
# The stock aarch64-nintendo-switch-freestanding target allows dynamic libraries, so rustc also
# tries to build rfvp's `cdylib` (used by the desktop and mobile packages) and fails for lack of
# an allocator. The same target with `dynamic-linking` off makes rustc skip it.
TARGET_JSON="$ROOT_DIR/platform/switch/$TARGET_NAME.json"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
OUT_DIR="$ROOT_DIR/target/rfvp-switch/switch"
BUILD_DIR="$TARGET_DIR/rfvp-switch-runtime"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT_DIR/platform/switch/host/Cargo.toml" | head -1)"

CC="$DEVKITA64/bin/aarch64-none-elf-gcc"
for tool in "$DEVKITPRO/tools/bin/elf2nro" "$DEVKITPRO/tools/bin/nacptool" "$CC" \
            "$DEVKITPRO/libnx/lib/libnx.a"; do
  [[ -e "$tool" ]] || { echo "ERROR: missing $tool (install devkitPro with devkitA64 and libnx)" >&2; exit 1; }
done
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found" >&2; exit 1; }
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rust-src --no-self-update

cd "$ROOT_DIR"
# newlib provides memcpy and friends, so compiler_builtins does not.
cargo "+$TOOLCHAIN" build -p rfvp-horizon --release \
  --target "$TARGET_JSON" \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins
LIB="$TARGET_DIR/$TARGET_NAME/release/librfvp_horizon.a"
[[ -f "$LIB" ]] || { echo "ERROR: build produced no $LIB" >&2; exit 1; }

ARCH_FLAGS=(-march=armv8-a+crc+crypto -mtune=cortex-a57 -mtp=soft -fPIE)
mkdir -p "$BUILD_DIR" "$OUT_DIR"
"$CC" "${ARCH_FLAGS[@]}" -std=gnu17 -O2 -g -Wall -Wextra -ffunction-sections -fdata-sections \
  -I"$DEVKITPRO/libnx/include" -c "$ROOT_DIR/platform/switch/runtime/main.c" \
  -o "$BUILD_DIR/main.o"
"$CC" "${ARCH_FLAGS[@]}" -specs="$DEVKITPRO/libnx/switch.specs" -g -Wl,--gc-sections \
  -Wl,-Map,"$BUILD_DIR/rfvp.map" "$BUILD_DIR/main.o" "$LIB" \
  -L"$DEVKITPRO/libnx/lib" -lnx -lm -o "$BUILD_DIR/rfvp.elf"

"$DEVKITPRO/tools/bin/nacptool" --create "rfvp" "rfvp" "${VERSION:-0.1.0}" "$BUILD_DIR/rfvp.nacp"
"$DEVKITPRO/tools/bin/elf2nro" "$BUILD_DIR/rfvp.elf" "$OUT_DIR/rfvp.nro" --nacp="$BUILD_DIR/rfvp.nacp"
echo "[switch] $OUT_DIR/rfvp.nro"

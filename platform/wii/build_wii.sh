#!/usr/bin/env bash
# Build the Wii port into a DOL: the engine (platform/wii/host) as a static library, linked into
# the libogc runtime (platform/wii/runtime/main.c), drawing with GX.
#
# Needs devkitPro with devkitPPC and libogc (DEVKITPRO, default /opt/devkitpro), and rustup.
# Output: target/rfvp-wii/apps/rfvp/boot.dol
#
# The game, repacked with `rfvp-rebuilder --platform wii`, goes in sd:/apps/rfvp/game/; see
# platform/wii/README.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../consoles/toolchain.env
source "$ROOT_DIR/platform/consoles/toolchain.env"

export DEVKITPRO="${DEVKITPRO:-/opt/devkitpro}"
DEVKITPPC="$DEVKITPRO/devkitPPC"
LIBOGC="$DEVKITPRO/libogc"
TOOLCHAIN="${RFVP_CONSOLE_NIGHTLY}"
TARGET_NAME="powerpc-nintendo-wii-rfvp"
TARGET_JSON="$SCRIPT_DIR/$TARGET_NAME.json"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
OUT_DIR="$ROOT_DIR/target/rfvp-wii/apps/rfvp"
BUILD_DIR="$TARGET_DIR/rfvp-wii-runtime"

CC="$DEVKITPPC/bin/powerpc-eabi-gcc"
for tool in "$DEVKITPRO/tools/bin/elf2dol" "$CC" "$LIBOGC/lib/wii/libogc.a"; do
  [[ -e "$tool" ]] || { echo "ERROR: missing $tool (install devkitPro with devkitPPC and libogc)" >&2; exit 1; }
done
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found" >&2; exit 1; }
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rust-src --no-self-update

cd "$ROOT_DIR"
# newlib provides memcpy and friends, so compiler_builtins does not.
cargo "+$TOOLCHAIN" build -p rfvp-wii --release \
  --target "$TARGET_JSON" \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins
LIB="$TARGET_DIR/$TARGET_NAME/release/librfvp_wii.a"
[[ -f "$LIB" ]] || { echo "ERROR: build produced no $LIB" >&2; exit 1; }

ARCH_FLAGS=(-DGEKKO -mrvl -mcpu=750 -meabi -mhard-float)
CFLAGS=("${ARCH_FLAGS[@]}" -O2 -g -Wall -ffunction-sections -fdata-sections
  -I"$LIBOGC/include" -I"$ROOT_DIR/crates/rfvp/vendor")
mkdir -p "$BUILD_DIR" "$OUT_DIR"
"$CC" "${CFLAGS[@]}" -std=gnu11 -Wextra -c "$SCRIPT_DIR/runtime/main.c" -o "$BUILD_DIR/main.o"
"$CC" "${CFLAGS[@]}" -w -c "$ROOT_DIR/crates/rfvp/vendor/rfvp_ogg_vorbis.c" -o "$BUILD_DIR/rfvp_ogg_vorbis.o"
"$CC" "${ARCH_FLAGS[@]}" -g -Wl,--gc-sections -Wl,-Map,"$BUILD_DIR/rfvp.map" \
  "$BUILD_DIR/main.o" "$BUILD_DIR/rfvp_ogg_vorbis.o" "$LIB" \
  -L"$LIBOGC/lib/wii" -lwiiuse -lbte -lfat -lasnd -logc -lm -o "$BUILD_DIR/rfvp.elf"
"$DEVKITPRO/tools/bin/elf2dol" "$BUILD_DIR/rfvp.elf" "$OUT_DIR/boot.dol"
echo "[wii] $OUT_DIR/boot.dol"

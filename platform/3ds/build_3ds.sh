#!/usr/bin/env bash
# Build the Nintendo 3DS port into a 3DSX: the engine (platform/3ds/host) as a static library,
# linked into the libctru runtime (platform/3ds/runtime/main.c), drawing with citro3d.
#
# Needs devkitPro with devkitARM, libctru and citro3d (DEVKITPRO, default /opt/devkitpro), and
# rustup. Output: target/rfvp-3ds/3ds/rfvp.3dsx
#
# The game goes in sdmc:/3ds/rfvp/game/ on the SD card; see platform/3ds/README.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../consoles/toolchain.env
source "$ROOT_DIR/platform/consoles/toolchain.env"

export DEVKITPRO="${DEVKITPRO:-/opt/devkitpro}"
DEVKITARM="$DEVKITPRO/devkitARM"
TOOLCHAIN="${RFVP_CONSOLE_NIGHTLY}"
TARGET_NAME="armv6k-nintendo-3ds-rfvp"
# The built-in armv6k-nintendo-3ds target unwinds on panic, which needs std; this is the same
# target with panics aborting.
TARGET_JSON="$ROOT_DIR/platform/3ds/$TARGET_NAME.json"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
OUT_DIR="$ROOT_DIR/target/rfvp-3ds/3ds"
BUILD_DIR="$TARGET_DIR/rfvp-3ds-runtime"
TOOLS="$DEVKITPRO/tools/bin"

CC="$DEVKITARM/bin/arm-none-eabi-gcc"
for tool in "$TOOLS/3dsxtool" "$TOOLS/smdhtool" "$TOOLS/picasso" "$TOOLS/bin2s" "$CC" \
            "$DEVKITPRO/libctru/lib/libctru.a" "$DEVKITPRO/libctru/lib/libcitro3d.a"; do
  [[ -e "$tool" ]] || { echo "ERROR: missing $tool (install devkitPro with devkitARM, libctru and citro3d)" >&2; exit 1; }
done
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found" >&2; exit 1; }
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rust-src --no-self-update

cd "$ROOT_DIR"
# newlib provides memcpy and friends, so compiler_builtins does not.
cargo "+$TOOLCHAIN" build -p rfvp-3ds --release \
  --target "$TARGET_JSON" \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins
LIB="$TARGET_DIR/$TARGET_NAME/release/librfvp_3ds.a"
[[ -f "$LIB" ]] || { echo "ERROR: build produced no $LIB" >&2; exit 1; }

mkdir -p "$BUILD_DIR" "$OUT_DIR"
# The sprite shader, assembled and embedded.
"$TOOLS/picasso" -o "$BUILD_DIR/sprite.shbin" "$SCRIPT_DIR/runtime/sprite.v.pica"
(cd "$BUILD_DIR" && "$TOOLS/bin2s" sprite.shbin > sprite_shbin.s)
cat > "$BUILD_DIR/sprite_shbin.h" <<'H'
#include <stdint.h>
extern const uint8_t sprite_shbin[];
extern const uint8_t sprite_shbin_end[];
extern const uint32_t sprite_shbin_size;
H

ARCH_FLAGS=(-march=armv6k -mtune=mpcore -mfloat-abi=hard -mtp=soft)
CFLAGS=("${ARCH_FLAGS[@]}" -D__3DS__ -O2 -g -Wall -ffunction-sections -fdata-sections
  -I"$DEVKITPRO/libctru/include" -I"$BUILD_DIR" -I"$ROOT_DIR/crates/rfvp/vendor")
"$CC" "${CFLAGS[@]}" -std=gnu11 -Wextra -c "$SCRIPT_DIR/runtime/main.c" -o "$BUILD_DIR/main.o"
"$CC" "${CFLAGS[@]}" -w -c "$ROOT_DIR/crates/rfvp/vendor/rfvp_ogg_vorbis.c" -o "$BUILD_DIR/rfvp_ogg_vorbis.o"
"$CC" "${ARCH_FLAGS[@]}" -x assembler-with-cpp -c "$BUILD_DIR/sprite_shbin.s" -o "$BUILD_DIR/sprite_shbin.o"
"$CC" "${ARCH_FLAGS[@]}" -specs=3dsx.specs -g -Wl,--gc-sections -Wl,-Map,"$BUILD_DIR/rfvp.map" \
  "$BUILD_DIR/main.o" "$BUILD_DIR/rfvp_ogg_vorbis.o" "$BUILD_DIR/sprite_shbin.o" "$LIB" \
  -L"$DEVKITPRO/libctru/lib" -lcitro3d -lctru -lm -o "$BUILD_DIR/rfvp.elf"

"$TOOLS/smdhtool" --create "rfvp" "rfvp visual novel engine" "rfvp" \
  "$DEVKITPRO/libctru/default_icon.png" "$BUILD_DIR/rfvp.smdh"
"$TOOLS/3dsxtool" "$BUILD_DIR/rfvp.elf" "$OUT_DIR/rfvp.3dsx" --smdh="$BUILD_DIR/rfvp.smdh"
echo "[3ds] $OUT_DIR/rfvp.3dsx"

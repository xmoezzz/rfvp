#!/usr/bin/env bash
# Build the Wii U port into an RPX: the engine (platform/wiiu/host) as a static library, linked
# into the wut runtime (platform/wiiu/runtime/main.c).
#
# Needs devkitPro with devkitPPC and wut (DEVKITPRO, default /opt/devkitpro), and rustup.
# Output: target/rfvp-wiiu/wiiu/apps/rfvp/rfvp.rpx
#
# The game goes in sd:/wiiu/apps/rfvp/game/ on the SD card; see platform/wiiu/README.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../consoles/toolchain.env
source "$ROOT_DIR/platform/consoles/toolchain.env"

export DEVKITPRO="${DEVKITPRO:-/opt/devkitpro}"
DEVKITPPC="$DEVKITPRO/devkitPPC"
WUT="$DEVKITPRO/wut"
TOOLCHAIN="${RFVP_CONSOLE_NIGHTLY}"
TARGET_NAME="powerpc-nintendo-wiiu-rfvp"
TARGET_JSON="$SCRIPT_DIR/$TARGET_NAME.json"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
OUT_DIR="$ROOT_DIR/target/rfvp-wiiu/wiiu/apps/rfvp"
BUILD_DIR="$TARGET_DIR/rfvp-wiiu-runtime"

CC="$DEVKITPPC/bin/powerpc-eabi-gcc"
for tool in "$DEVKITPRO/tools/bin/elf2rpl" "$CC" "$WUT/lib/libwut.a"; do
  [[ -e "$tool" ]] || { echo "ERROR: missing $tool (install devkitPro with devkitPPC and wut)" >&2; exit 1; }
done
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found" >&2; exit 1; }
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rust-src --no-self-update

cd "$ROOT_DIR"
# newlib provides memcpy and friends, so compiler_builtins does not.
cargo "+$TOOLCHAIN" build -p rfvp-wiiu --release \
  --target "$TARGET_JSON" \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins
LIB="$TARGET_DIR/$TARGET_NAME/release/librfvp_wiiu.a"
[[ -f "$LIB" ]] || { echo "ERROR: build produced no $LIB" >&2; exit 1; }

ARCH_FLAGS=(-DESPRESSO -mcpu=750 -meabi -mhard-float)
CFLAGS=("${ARCH_FLAGS[@]}" -D__WIIU__ -D__WUT__ -O2 -g -Wall -ffunction-sections -fdata-sections
  -I"$WUT/include" -I"$ROOT_DIR/crates/rfvp/vendor")
mkdir -p "$BUILD_DIR" "$OUT_DIR"
"$CC" "${CFLAGS[@]}" -std=gnu11 -Wextra -c "$SCRIPT_DIR/runtime/main.c" -o "$BUILD_DIR/main.o"
"$CC" "${CFLAGS[@]}" -w -c "$ROOT_DIR/crates/rfvp/vendor/rfvp_ogg_vorbis.c" -o "$BUILD_DIR/rfvp_ogg_vorbis.o"
"$CC" "${ARCH_FLAGS[@]}" -specs="$WUT/share/wut.specs" -g -Wl,--gc-sections \
  -Wl,-Map,"$BUILD_DIR/rfvp.map" \
  "$BUILD_DIR/main.o" "$BUILD_DIR/rfvp_ogg_vorbis.o" "$LIB" \
  -L"$WUT/lib" -lwut -lm -o "$BUILD_DIR/rfvp.elf"
# rustc's objects carry .note.GNU-stack as a section with contents, which elf2rpl cannot place.
"$DEVKITPPC/bin/powerpc-eabi-objcopy" -R .note.GNU-stack "$BUILD_DIR/rfvp.elf" "$BUILD_DIR/rfvp.rpl.elf"
"$DEVKITPRO/tools/bin/elf2rpl" "$BUILD_DIR/rfvp.rpl.elf" "$OUT_DIR/rfvp.rpx"
echo "[wiiu] $OUT_DIR/rfvp.rpx"

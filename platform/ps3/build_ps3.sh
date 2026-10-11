#!/usr/bin/env bash
# Build the PS3 port: the engine (platform/ps3/host) as a static library, linked into the PSL1GHT
# runtime (platform/ps3/runtime/main.c), drawing on the RSX.
#
# Needs ps3toolchain with PSL1GHT (PS3DEV, default /usr/local/ps3dev; PSL1GHT, default $PS3DEV)
# and rustup. Output: target/rfvp-ps3/ps3/rfvp.elf (RPCS3 boots it as it is) and rfvp.self.
#
# See platform/ps3/README.md for where the game goes.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../consoles/toolchain.env
source "$ROOT_DIR/platform/consoles/toolchain.env"

export PS3DEV="${PS3DEV:-/usr/local/ps3dev}"
export PSL1GHT="${PSL1GHT:-$PS3DEV}"
TOOLCHAIN="${RFVP_CONSOLE_NIGHTLY}"
TARGET_NAME="powerpc64-sony-ps3-rfvp"
TARGET_JSON="$SCRIPT_DIR/$TARGET_NAME.json"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
OUT_DIR="$ROOT_DIR/target/rfvp-ps3/ps3"
BUILD_DIR="$TARGET_DIR/rfvp-ps3-runtime"

CC="$PS3DEV/ppu/bin/powerpc64-ps3-elf-gcc"
for tool in "$CC" "$PSL1GHT/ppu/lib/liblv2.a"; do
  [[ -e "$tool" ]] || { echo "ERROR: missing $tool (install ps3toolchain with PSL1GHT)" >&2; exit 1; }
done
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found" >&2; exit 1; }
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rust-src --no-self-update

cd "$ROOT_DIR"
# newlib provides memcpy and friends, so compiler_builtins does not. The PPU linker (binutils
# 2.22) reads DWARF 4 at most.
RUSTFLAGS="${RUSTFLAGS:-} -Zdwarf-version=4" cargo "+$TOOLCHAIN" build -p rfvp-ps3 --release \
  --target "$TARGET_JSON" \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins
LIB="$TARGET_DIR/$TARGET_NAME/release/librfvp_ps3.a"
[[ -f "$LIB" ]] || { echo "ERROR: build produced no $LIB" >&2; exit 1; }

mkdir -p "$BUILD_DIR" "$OUT_DIR"
# The sprite shaders: hand-written assembly, so cgcomp needs no Cg toolkit. Embedded with bin2s.
SHADER_OBJS=()
for shader in vp fp; do
  flag="-${shader:0:1}"
  "$PS3DEV/bin/cgcomp" -a "$flag" "$SCRIPT_DIR/runtime/sprite.$shader" "$BUILD_DIR/sprite.${shader}o"
  (cd "$BUILD_DIR" && "$PS3DEV/bin/bin2s" -a 64 "sprite.${shader}o" > "sprite_${shader}o.s")
  "$PS3DEV/ppu/bin/powerpc64-ps3-elf-as" -o "$BUILD_DIR/sprite_${shader}o.o" "$BUILD_DIR/sprite_${shader}o.s"
  printf 'extern const unsigned char sprite_%so[];\nextern const unsigned char sprite_%so_end[];\nextern const unsigned int sprite_%so_size;\n' \
    "$shader" "$shader" "$shader" > "$BUILD_DIR/sprite_${shader}o.h"
  SHADER_OBJS+=("$BUILD_DIR/sprite_${shader}o.o")
done
CFLAGS=(-mcpu=cell -O2 -g -Wall -ffunction-sections -fdata-sections -I"$BUILD_DIR"
  -I"$PSL1GHT/ppu/include" -I"$PS3DEV/portlibs/ppu/include" -I"$ROOT_DIR/crates/rfvp/vendor")
"$CC" "${CFLAGS[@]}" -std=gnu11 -Wextra -c "$SCRIPT_DIR/runtime/main.c" -o "$BUILD_DIR/main.o"
"$CC" "${CFLAGS[@]}" -w -c "$ROOT_DIR/crates/rfvp/vendor/rfvp_ogg_vorbis.c" -o "$BUILD_DIR/rfvp_ogg_vorbis.o"
"$CC" -mcpu=cell -g -Wl,--gc-sections -Wl,-Map,"$BUILD_DIR/rfvp.map" \
  "$BUILD_DIR/main.o" "$BUILD_DIR/rfvp_ogg_vorbis.o" "${SHADER_OBJS[@]}" "$LIB" \
  -L"$PSL1GHT/ppu/lib" -L"$PS3DEV/portlibs/ppu/lib" \
  -lrsx -lgcm_sys -lio -lsysutil -laudio -lsysfs -lrt -llv2 -lm -o "$OUT_DIR/rfvp.elf"
"$PS3DEV/ppu/bin/powerpc64-ps3-elf-strip" -o "$BUILD_DIR/rfvp.stripped.elf" "$OUT_DIR/rfvp.elf"
if command -v "$PS3DEV/bin/sprxlinker" >/dev/null 2>&1; then
  "$PS3DEV/bin/sprxlinker" "$BUILD_DIR/rfvp.stripped.elf"
fi
if [[ -x "$PS3DEV/bin/make_self" ]]; then
  "$PS3DEV/bin/make_self" "$BUILD_DIR/rfvp.stripped.elf" "$OUT_DIR/rfvp.self"
fi
echo "[ps3] $OUT_DIR/rfvp.elf"

#!/usr/bin/env bash

if [ -z "${BASH_VERSION:-}" ]; then
    exec bash "$0" "$@"
fi

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$WORKSPACE_DIR/target/rfvp-3ds/3ds"
ABI_DIR="$BUILD_DIR/abi-check"
TARGET_JSON="$SCRIPT_DIR/armv6k-nintendo-3ds-rfvp.json"
TARGET_DIR="$WORKSPACE_DIR/target/armv6k-nintendo-3ds-rfvp"
RUST_LIB="$TARGET_DIR/debug/librfvp_3ds.a"
ELF_OUT="$BUILD_DIR/rfvp-3ds.elf"
THREEDSX_OUT="$BUILD_DIR/rfvp-3ds.3dsx"

fail() {
    echo "error: $*" >&2
    exit 1
}

DEVKITPRO="${DEVKITPRO:-/opt/devkitpro}"
DEVKITARM="${DEVKITARM:-$DEVKITPRO/devkitARM}"
LIBCTRU="${LIBCTRU:-$DEVKITPRO/libctru}"

[[ -d "$DEVKITPRO" ]] || fail "DEVKITPRO not found: $DEVKITPRO"
[[ -d "$DEVKITARM" ]] || fail "DEVKITARM not found: $DEVKITARM"
[[ -d "$LIBCTRU" ]] || fail "libctru not found: $LIBCTRU"

find_tool() {
    local env_value="$1"
    shift
    if [[ -n "$env_value" ]]; then
        printf '%s\n' "$env_value"
        return 0
    fi
    local candidate
    for candidate in "$@"; do
        if [[ -x "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

ARM_GCC="$(find_tool "${ARM_GCC:-}" \
    "$DEVKITARM/bin/arm-none-eabi-gcc")" || fail "arm-none-eabi-gcc not found"
ARM_AR="$(find_tool "${ARM_AR:-}" \
    "$DEVKITARM/bin/arm-none-eabi-ar")" || fail "arm-none-eabi-ar not found"
ARM_READELF="$(find_tool "${ARM_READELF:-}" \
    "$DEVKITARM/bin/arm-none-eabi-readelf")" || fail "arm-none-eabi-readelf not found"
ARM_OBJCOPY="$(find_tool "${ARM_OBJCOPY:-}" \
    "$DEVKITARM/bin/arm-none-eabi-objcopy")" || fail "arm-none-eabi-objcopy not found"
THREEDSXTOOL="$(find_tool "${THREEDSXTOOL:-}" \
    "$DEVKITPRO/tools/bin/3dsxtool")" || fail "3dsxtool not found"

[[ -f "$TARGET_JSON" ]] || fail "target JSON not found: $TARGET_JSON"

mkdir -p "$BUILD_DIR"
rm -f "$BUILD_DIR"/entrypoint.o "$BUILD_DIR"/libctru_backend.o "$ELF_OUT" "$THREEDSX_OUT"

export CARGO_TARGET_ARMV6K_NINTENDO_3DS_RFVP_LINKER="$ARM_GCC"
export CARGO_TARGET_ARMV6K_NINTENDO_3DS_RFVP_RUSTFLAGS="-C panic=abort"

cargo +nightly build \
    -p rfvp-3ds \
    --lib \
    --features entrypoint \
    --target "$TARGET_JSON" \
    -Z build-std=core,alloc,compiler_builtins \
    -Z build-std-features=compiler-builtins-mem \
    -Z json-target-spec

[[ -f "$RUST_LIB" ]] || fail "Rust staticlib not found: $RUST_LIB"

COMMON_CFLAGS=(
    -march=armv6k
    -mtune=mpcore
    -mfloat-abi=hard
    -mtp=soft
    -O2
    -Wall
    -Wextra
    -I"$LIBCTRU/include"
    -I"$SCRIPT_DIR/c"
)

"$ARM_GCC" "${COMMON_CFLAGS[@]}" -c "$SCRIPT_DIR/c/entrypoint.c" -o "$BUILD_DIR/entrypoint.o"
"$ARM_GCC" "${COMMON_CFLAGS[@]}" -c "$SCRIPT_DIR/c/libctru_backend.c" -o "$BUILD_DIR/libctru_backend.o"

rm -rf "$ABI_DIR"
mkdir -p "$ABI_DIR"
(
    cd "$ABI_DIR"
    "$ARM_AR" x "$RUST_LIB"
)

RUST_OBJECT="$(find "$ABI_DIR" -name '*.o' -print -quit)"
[[ -n "$RUST_OBJECT" ]] || fail "no object file found inside Rust staticlib"

rust_flags="$("$ARM_READELF" -h "$RUST_OBJECT" | awk -F: '/Flags:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"
c_flags="$("$ARM_READELF" -h "$BUILD_DIR/entrypoint.o" | awk -F: '/Flags:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"

if [[ -z "$rust_flags" || -z "$c_flags" || "$rust_flags" != "$c_flags" ]]; then
    echo "Rust object path: $RUST_OBJECT" >&2
    echo "Rust ELF flags: ${rust_flags:-<unavailable>}" >&2
    echo "devkitARM C object path: $BUILD_DIR/entrypoint.o" >&2
    echo "devkitARM C ELF flags: ${c_flags:-<unavailable>}" >&2
    fail "Rust target/codegen ABI issue, not rfvp runtime issue"
fi

"$ARM_GCC" \
    -specs=3dsx.specs \
    -march=armv6k \
    -mtune=mpcore \
    -mfloat-abi=hard \
    -mtp=soft \
    -L"$LIBCTRU/lib" \
    "$BUILD_DIR/entrypoint.o" \
    "$BUILD_DIR/libctru_backend.o" \
    "$RUST_LIB" \
    -Wl,--start-group \
    -lctru -lm -lc \
    -Wl,--end-group \
    -o "$ELF_OUT"

"$ARM_OBJCOPY" --remove-section .note.GNU-stack "$ELF_OUT"
"$THREEDSXTOOL" "$ELF_OUT" "$THREEDSX_OUT"

echo "3DS ELF: $ELF_OUT"
echo "3DSX: $THREEDSX_OUT"

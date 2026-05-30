#!/usr/bin/env bash

if [ -z "${BASH_VERSION:-}" ]; then
    exec bash "$0" "$@"
fi

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$WORKSPACE_DIR/target/rfvp-wiiu/wiiu"
ABI_DIR="$BUILD_DIR/abi-check"
TARGET_JSON="$SCRIPT_DIR/powerpc-nintendo-wiiu-rfvp.json"
TARGET_DIR="$WORKSPACE_DIR/target/powerpc-nintendo-wiiu-rfvp"
RUST_LIB="$TARGET_DIR/debug/librfvp_wiiu.a"
ELF_OUT="$BUILD_DIR/rfvp-wiiu.elf"
RPX_OUT="$BUILD_DIR/rfvp-wiiu.rpx"

fail() {
    echo "error: $*" >&2
    exit 1
}

DEVKITPRO="${DEVKITPRO:-/opt/devkitpro}"
DEVKITPPC="${DEVKITPPC:-$DEVKITPRO/devkitPPC}"
WUT="${WUT:-$DEVKITPRO/wut}"

[[ -d "$DEVKITPRO" ]] || fail "DEVKITPRO not found: $DEVKITPRO"
[[ -d "$DEVKITPPC" ]] || fail "DEVKITPPC not found: $DEVKITPPC"
[[ -d "$WUT" ]] || fail "wut not found: $WUT"

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

PPC_GCC="$(find_tool "${PPC_GCC:-}" \
    "$DEVKITPPC/bin/powerpc-eabi-gcc" \
    "$DEVKITPPC/bin/powerpc-eabi-gcc-15.2.0")" || fail "powerpc-eabi-gcc not found"
PPC_AR="$(find_tool "${PPC_AR:-}" \
    "$DEVKITPPC/bin/powerpc-eabi-ar")" || fail "powerpc-eabi-ar not found"
PPC_READELF="$(find_tool "${PPC_READELF:-}" \
    "$DEVKITPPC/bin/powerpc-eabi-readelf")" || fail "powerpc-eabi-readelf not found"
PPC_OBJCOPY="$(find_tool "${PPC_OBJCOPY:-}" \
    "$DEVKITPPC/bin/powerpc-eabi-objcopy")" || fail "powerpc-eabi-objcopy not found"
ELF2RPL="$(find_tool "${ELF2RPL:-}" \
    "$DEVKITPRO/tools/bin/elf2rpl")" || fail "elf2rpl not found"

[[ -x "$PPC_GCC" ]] || fail "powerpc-eabi-gcc not found or not executable: $PPC_GCC"
[[ -x "$PPC_AR" ]] || fail "powerpc-eabi-ar not found or not executable: $PPC_AR"
[[ -x "$PPC_READELF" ]] || fail "powerpc-eabi-readelf not found or not executable: $PPC_READELF"
[[ -x "$PPC_OBJCOPY" ]] || fail "powerpc-eabi-objcopy not found or not executable: $PPC_OBJCOPY"
[[ -x "$ELF2RPL" ]] || fail "elf2rpl not found or not executable: $ELF2RPL"
[[ -f "$TARGET_JSON" ]] || fail "target JSON not found: $TARGET_JSON"

mkdir -p "$BUILD_DIR"
rm -f "$BUILD_DIR"/entrypoint.o "$BUILD_DIR"/wut_backend.o "$ELF_OUT" "$RPX_OUT"

export CARGO_TARGET_POWERPC_NINTENDO_WIIU_RFVP_LINKER="$PPC_GCC"
export CARGO_TARGET_POWERPC_NINTENDO_WIIU_RFVP_RUSTFLAGS="-C panic=abort"

cargo +nightly build \
    -p rfvp-wiiu \
    --lib \
    --features entrypoint \
    --target "$TARGET_JSON" \
    -Z build-std=core,alloc,compiler_builtins \
    -Z build-std-features=compiler-builtins-mem \
    -Z json-target-spec

[[ -f "$RUST_LIB" ]] || fail "Rust staticlib not found: $RUST_LIB"

COMMON_CFLAGS=(
    -mcpu=750
    -meabi
    -mhard-float
    -O2
    -Wall
    -Wextra
    -I"$WUT/include"
    -I"$SCRIPT_DIR/c"
)

"$PPC_GCC" "${COMMON_CFLAGS[@]}" -c "$SCRIPT_DIR/c/entrypoint.c" -o "$BUILD_DIR/entrypoint.o"
"$PPC_GCC" "${COMMON_CFLAGS[@]}" -c "$SCRIPT_DIR/c/wut_backend.c" -o "$BUILD_DIR/wut_backend.o"

rm -rf "$ABI_DIR"
mkdir -p "$ABI_DIR"
(
    cd "$ABI_DIR"
    "$PPC_AR" x "$RUST_LIB"
)

RUST_OBJECT="$(find "$ABI_DIR" -name '*.o' -print -quit)"
[[ -n "$RUST_OBJECT" ]] || fail "no object file found inside Rust staticlib"

rust_flags="$("$PPC_READELF" -h "$RUST_OBJECT" | awk -F: '/Flags:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"
c_flags="$("$PPC_READELF" -h "$BUILD_DIR/entrypoint.o" | awk -F: '/Flags:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"

if [[ -z "$rust_flags" || -z "$c_flags" || "$rust_flags" != "$c_flags" ]]; then
    echo "Rust object path: $RUST_OBJECT" >&2
    echo "Rust ELF flags: ${rust_flags:-<unavailable>}" >&2
    echo "devkitPPC C object path: $BUILD_DIR/entrypoint.o" >&2
    echo "devkitPPC C ELF flags: ${c_flags:-<unavailable>}" >&2
    fail "Rust target/codegen ABI issue, not rfvp runtime issue"
fi

"$PPC_GCC" \
    -specs="$WUT/share/wut.specs" \
    -mcpu=750 \
    -meabi \
    -mhard-float \
    -L"$WUT/lib" \
    "$BUILD_DIR/entrypoint.o" \
    "$BUILD_DIR/wut_backend.o" \
    "$RUST_LIB" \
    -Wl,--start-group \
    -lwut -lm -lc \
    -Wl,--end-group \
    -o "$ELF_OUT"

"$PPC_OBJCOPY" --remove-section .note.GNU-stack "$ELF_OUT"
"$ELF2RPL" "$ELF_OUT" "$RPX_OUT"

echo "Wii U ELF: $ELF_OUT"
echo "Wii U RPX: $RPX_OUT"

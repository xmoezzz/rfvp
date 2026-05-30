#!/usr/bin/env bash

if [ -z "${BASH_VERSION:-}" ]; then
    exec bash "$0" "$@"
fi

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$WORKSPACE_DIR/target/rfvp-wii/wii"
ABI_DIR="$BUILD_DIR/abi-check"
TARGET_JSON="$SCRIPT_DIR/powerpc-nintendo-wii-rfvp.json"
TARGET_DIR="$WORKSPACE_DIR/target/powerpc-nintendo-wii-rfvp"
RUST_LIB="$TARGET_DIR/debug/librfvp_wii.a"
ELF_OUT="$BUILD_DIR/rfvp-wii.elf"
DOL_OUT="$BUILD_DIR/rfvp-wii.dol"

fail() {
    echo "error: $*" >&2
    exit 1
}

DEVKITPRO="${DEVKITPRO:-/opt/devkitpro}"
DEVKITPPC="${DEVKITPPC:-$DEVKITPRO/devkitPPC}"
LIBOGC="${LIBOGC:-$DEVKITPRO/libogc}"

[[ -d "$DEVKITPRO" ]] || fail "DEVKITPRO not found: $DEVKITPRO"
[[ -d "$DEVKITPPC" ]] || fail "DEVKITPPC not found: $DEVKITPPC"
[[ -d "$LIBOGC" ]] || fail "libogc not found: $LIBOGC"

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
ELF2DOL="$(find_tool "${ELF2DOL:-}" \
    "$DEVKITPRO/tools/bin/elf2dol")" || fail "elf2dol not found"

[[ -x "$PPC_GCC" ]] || fail "powerpc-eabi-gcc not found or not executable: $PPC_GCC"
[[ -x "$PPC_AR" ]] || fail "powerpc-eabi-ar not found or not executable: $PPC_AR"
[[ -x "$PPC_READELF" ]] || fail "powerpc-eabi-readelf not found or not executable: $PPC_READELF"
[[ -x "$ELF2DOL" ]] || fail "elf2dol not found or not executable: $ELF2DOL"
[[ -f "$TARGET_JSON" ]] || fail "target JSON not found: $TARGET_JSON"

mkdir -p "$BUILD_DIR"
rm -f "$BUILD_DIR"/entrypoint.o "$BUILD_DIR"/libogc_backend.o "$ELF_OUT" "$DOL_OUT"

export CARGO_TARGET_POWERPC_NINTENDO_WII_RFVP_LINKER="$PPC_GCC"
export CARGO_TARGET_POWERPC_NINTENDO_WII_RFVP_RUSTFLAGS="-C panic=abort"

cargo +nightly build \
    -p rfvp-wii \
    --lib \
    --features entrypoint \
    --target "$TARGET_JSON" \
    -Z build-std=core,alloc,compiler_builtins \
    -Z build-std-features=compiler-builtins-mem \
    -Z json-target-spec

[[ -f "$RUST_LIB" ]] || fail "Rust staticlib not found: $RUST_LIB"

COMMON_CFLAGS=(
    -DGEKKO
    -DHW_RVL
    -mrvl
    -mcpu=750
    -meabi
    -mhard-float
    -O2
    -Wall
    -Wextra
    -I"$LIBOGC/include"
    -I"$SCRIPT_DIR/c"
)

"$PPC_GCC" "${COMMON_CFLAGS[@]}" -c "$SCRIPT_DIR/c/entrypoint.c" -o "$BUILD_DIR/entrypoint.o"
"$PPC_GCC" "${COMMON_CFLAGS[@]}" -c "$SCRIPT_DIR/c/libogc_backend.c" -o "$BUILD_DIR/libogc_backend.o"

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
    -mrvl \
    -mcpu=750 \
    -meabi \
    -mhard-float \
    -L"$LIBOGC/lib/wii" \
    "$BUILD_DIR/entrypoint.o" \
    "$BUILD_DIR/libogc_backend.o" \
    "$RUST_LIB" \
    -Wl,--start-group \
    -lwiiuse -lbte -lfat -logc -lm -lc \
    -Wl,--end-group \
    -o "$ELF_OUT"

"$ELF2DOL" "$ELF_OUT" "$DOL_OUT"

echo "Wii ELF: $ELF_OUT"
echo "Wii DOL: $DOL_OUT"

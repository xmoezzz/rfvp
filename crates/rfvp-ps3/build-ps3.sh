#!/usr/bin/env bash

if [ -z "${BASH_VERSION:-}" ]; then
    exec bash "$0" "$@"
fi

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$WORKSPACE_DIR/target/rfvp-ps3/ps3"
ABI_DIR="$BUILD_DIR/abi-check"
TARGET_JSON="$SCRIPT_DIR/powerpc64-sony-ps3-rfvp.json"
TARGET_DIR="$WORKSPACE_DIR/target/powerpc64-sony-ps3-rfvp"
RUST_LIB="$TARGET_DIR/debug/librfvp_ps3.a"
ELF_OUT="$BUILD_DIR/rfvp-ps3.elf"
SELF_OUT="$BUILD_DIR/rfvp-ps3.self"

fail() {
    echo "error: $*" >&2
    exit 1
}

PS3DEV="${PS3DEV:-}"
PSL1GHT="${PSL1GHT:-${PS3DEV:+$PS3DEV/psl1ght}}"
PS3_PORTLIBS="${PS3_PORTLIBS:-${PS3DEV:+$PS3DEV/portlibs/ppu}}"

[[ -n "$PS3DEV" ]] || fail "PS3DEV is not set"
[[ -d "$PS3DEV" ]] || fail "PS3DEV not found: $PS3DEV"
[[ -n "$PSL1GHT" ]] || fail "PSL1GHT is not set"
[[ -d "$PSL1GHT" ]] || fail "PSL1GHT not found: $PSL1GHT"

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

PPU_GCC="$(find_tool "${PPU_GCC:-}" \
    "$PS3DEV/ppu/bin/ppu-gcc" \
    "$PS3DEV/bin/ppu-gcc")" || fail "ppu-gcc not found"
PPU_AR="$(find_tool "${PPU_AR:-}" \
    "$PS3DEV/ppu/bin/ppu-ar" \
    "$PS3DEV/bin/ppu-ar")" || fail "ppu-ar not found"
PPU_READELF="$(find_tool "${PPU_READELF:-}" \
    "$PS3DEV/ppu/bin/ppu-readelf" \
    "$PS3DEV/bin/ppu-readelf")" || fail "ppu-readelf not found"
PPU_OBJCOPY="$(find_tool "${PPU_OBJCOPY:-}" \
    "$PS3DEV/ppu/bin/ppu-objcopy" \
    "$PS3DEV/bin/ppu-objcopy")" || fail "ppu-objcopy not found"
MAKE_SELF="$(find_tool "${MAKE_SELF:-}" \
    "$PS3DEV/bin/make_self" \
    "$PS3DEV/bin/make_fself" \
    "$PS3DEV/bin/fself.py")" || fail "make_self/make_fself/fself.py not found"

[[ -f "$TARGET_JSON" ]] || fail "target JSON not found: $TARGET_JSON"

mkdir -p "$BUILD_DIR"
rm -f "$BUILD_DIR"/entrypoint.o "$BUILD_DIR"/psl1ght_backend.o "$ELF_OUT" "$SELF_OUT"

export CARGO_TARGET_POWERPC64_SONY_PS3_RFVP_LINKER="$PPU_GCC"
export CARGO_TARGET_POWERPC64_SONY_PS3_RFVP_RUSTFLAGS="-C panic=abort"

cargo +nightly build \
    -p rfvp-ps3 \
    --lib \
    --features entrypoint \
    --target "$TARGET_JSON" \
    -Z build-std=core,alloc,compiler_builtins \
    -Z build-std-features=compiler-builtins-mem \
    -Z json-target-spec

[[ -f "$RUST_LIB" ]] || fail "Rust staticlib not found: $RUST_LIB"

COMMON_CFLAGS=(
    -m64
    -O2
    -Wall
    -Wextra
    -I"$PSL1GHT/include"
)

if [[ -n "$PS3_PORTLIBS" && -d "$PS3_PORTLIBS/include" ]]; then
    COMMON_CFLAGS+=(-I"$PS3_PORTLIBS/include")
fi

"$PPU_GCC" "${COMMON_CFLAGS[@]}" -c "$SCRIPT_DIR/c/entrypoint.c" -o "$BUILD_DIR/entrypoint.o"
"$PPU_GCC" "${COMMON_CFLAGS[@]}" -c "$SCRIPT_DIR/c/psl1ght_backend.c" -o "$BUILD_DIR/psl1ght_backend.o"

rm -rf "$ABI_DIR"
mkdir -p "$ABI_DIR"
(
    cd "$ABI_DIR"
    "$PPU_AR" x "$RUST_LIB"
)

RUST_OBJECT="$(find "$ABI_DIR" -name '*.o' -print -quit)"
[[ -n "$RUST_OBJECT" ]] || fail "no object file found inside Rust staticlib"

rust_flags="$("$PPU_READELF" -h "$RUST_OBJECT" | awk -F: '/Flags:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"
c_flags="$("$PPU_READELF" -h "$BUILD_DIR/entrypoint.o" | awk -F: '/Flags:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"

if [[ -z "$rust_flags" || -z "$c_flags" || "$rust_flags" != "$c_flags" ]]; then
    echo "Rust object path: $RUST_OBJECT" >&2
    echo "Rust ELF flags: ${rust_flags:-<unavailable>}" >&2
    echo "PS3 PPU C object path: $BUILD_DIR/entrypoint.o" >&2
    echo "PS3 PPU C ELF flags: ${c_flags:-<unavailable>}" >&2
    fail "Rust target/codegen ABI issue, not rfvp runtime issue"
fi

LINK_FLAGS=(
    -m64
    -L"$PSL1GHT/lib"
)

if [[ -n "$PS3_PORTLIBS" && -d "$PS3_PORTLIBS/lib" ]]; then
    LINK_FLAGS+=(-L"$PS3_PORTLIBS/lib")
fi

"$PPU_GCC" \
    "${LINK_FLAGS[@]}" \
    "$BUILD_DIR/entrypoint.o" \
    "$BUILD_DIR/psl1ght_backend.o" \
    "$RUST_LIB" \
    -Wl,--start-group \
    -lrsx -lgcm_sys -lsysutil -lio -lpsl1ght -lrt -llv2 -lm -lc \
    -Wl,--end-group \
    -o "$ELF_OUT"

"$PPU_OBJCOPY" --remove-section .note.GNU-stack "$ELF_OUT"
"$MAKE_SELF" "$ELF_OUT" "$SELF_OUT"

echo "PS3 ELF: $ELF_OUT"
echo "PS3 SELF: $SELF_OUT"

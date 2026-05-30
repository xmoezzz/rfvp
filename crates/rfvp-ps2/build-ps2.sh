#!/usr/bin/env bash

if [ -z "${BASH_VERSION:-}" ]; then
    exec bash "$0" "$@"
fi

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$WORKSPACE_DIR/target/rfvp-ps2/ps2"
ABI_DIR="$BUILD_DIR/abi-check"
TARGET_JSON="$SCRIPT_DIR/mipsel-sony-ps2-rfvp.json"
TARGET_DIR="$WORKSPACE_DIR/target/mipsel-sony-ps2-rfvp"
RUST_LIB="$TARGET_DIR/debug/librfvp_ps2.a"
ELF_OUT="$BUILD_DIR/rfvp-ps2.elf"

fail() {
    echo "error: $*" >&2
    exit 1
}

PS2DEV="${PS2DEV:-$HOME/ps2dev}"
PS2SDK="${PS2SDK:-$PS2DEV/ps2sdk}"

[[ -d "$PS2DEV" ]] || fail "PS2DEV not found: $PS2DEV"
[[ -d "$PS2SDK" ]] || fail "PS2SDK not found: $PS2SDK"

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

EE_GCC="$(find_tool "${EE_GCC:-}" \
    "$PS2DEV/ee/bin/ee-gcc" \
    "$PS2DEV/ee/bin/mips64r5900el-ps2-elf-gcc" \
    "$PS2DEV/ee/bin/mips64r5900el-ps2-elf-gcc-15.2.0")" || fail "ee-gcc not found"
EE_AR="$(find_tool "${EE_AR:-}" \
    "$PS2DEV/ee/bin/ee-ar" \
    "$PS2DEV/ee/bin/mips64r5900el-ps2-elf-ar")" || fail "ee-ar not found"
EE_READELF="$(find_tool "${EE_READELF:-}" \
    "$PS2DEV/ee/bin/ee-readelf" \
    "$PS2DEV/ee/bin/mips64r5900el-ps2-elf-readelf")" || fail "ee-readelf not found"

[[ -x "$EE_GCC" ]] || fail "ee-gcc not found or not executable: $EE_GCC"
[[ -x "$EE_AR" ]] || fail "ee-ar not found or not executable: $EE_AR"
[[ -x "$EE_READELF" ]] || fail "ee-readelf not found or not executable: $EE_READELF"
[[ -f "$TARGET_JSON" ]] || fail "target JSON not found: $TARGET_JSON"

mkdir -p "$BUILD_DIR"
rm -f "$BUILD_DIR"/entrypoint.o "$BUILD_DIR"/ps2sdk_backend.o "$ELF_OUT"

export CARGO_TARGET_MIPSEL_SONY_PS2_RFVP_LINKER="$EE_GCC"
export CARGO_TARGET_MIPSEL_SONY_PS2_RFVP_RUSTFLAGS="-C panic=abort"

cargo +nightly build \
    -p rfvp-ps2 \
    --lib \
    --features entrypoint \
    --target "$TARGET_JSON" \
    -Z build-std=core,alloc,compiler_builtins \
    -Z build-std-features=compiler-builtins-mem \
    -Z json-target-spec

[[ -f "$RUST_LIB" ]] || fail "Rust staticlib not found: $RUST_LIB"

find_crt_object() {
    local candidate
    for candidate in \
        "$PS2SDK/ee/startup/crt0.o" \
        "$PS2SDK/ee/lib/crt0.o" \
        "$PS2DEV/ee/mips64r5900el-ps2-elf/lib/crt0.o" \
        "$PS2DEV/ee/ee/lib/crt0.o" \
        "$PS2DEV/ee/lib/crt0.o"; do
        if [[ -f "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

CRT_OBJECT="$(find_crt_object)" || fail "PS2SDK crt/start object not found"

rm -rf "$ABI_DIR"
mkdir -p "$ABI_DIR"
(
    cd "$ABI_DIR"
    "$EE_AR" x "$RUST_LIB"
)

RUST_OBJECT="$(find "$ABI_DIR" -name '*.o' -print -quit)"
[[ -n "$RUST_OBJECT" ]] || fail "no object file found inside Rust staticlib"

rust_flags="$("$EE_READELF" -h "$RUST_OBJECT" | awk -F: '/Flags:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"
crt_flags="$("$EE_READELF" -h "$CRT_OBJECT" | awk -F: '/Flags:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"

if [[ -z "$rust_flags" || -z "$crt_flags" || "$rust_flags" != "$crt_flags" ]]; then
    echo "Rust object path: $RUST_OBJECT" >&2
    echo "Rust ELF flags: ${rust_flags:-<unavailable>}" >&2
    echo "PS2SDK object path: $CRT_OBJECT" >&2
    echo "PS2SDK ELF flags: ${crt_flags:-<unavailable>}" >&2
    fail "Rust target/codegen ABI issue, not rfvp runtime issue"
fi

COMMON_CFLAGS=(
    -D_EE
    -G0
    -O2
    -Wall
    -Wextra
    -I"$PS2SDK/ee/include"
    -I"$PS2SDK/common/include"
    -I"$SCRIPT_DIR/c"
)

"$EE_GCC" "${COMMON_CFLAGS[@]}" -c "$SCRIPT_DIR/c/entrypoint.c" -o "$BUILD_DIR/entrypoint.o"
"$EE_GCC" "${COMMON_CFLAGS[@]}" -c "$SCRIPT_DIR/c/ps2sdk_backend.c" -o "$BUILD_DIR/ps2sdk_backend.o"

LINKFILE="$PS2SDK/ee/startup/linkfile"
[[ -f "$LINKFILE" ]] || fail "PS2SDK EE linkfile not found: $LINKFILE"

"$EE_GCC" \
    -T"$LINKFILE" \
    -L"$PS2SDK/ee/lib" \
    "$BUILD_DIR/entrypoint.o" \
    "$BUILD_DIR/ps2sdk_backend.o" \
    "$RUST_LIB" \
    -Wl,--start-group \
    -ldraw -lgraph -lpacket -ldma -lpad -lfileXio -lkernel -lc \
    -Wl,--end-group \
    -o "$ELF_OUT"

echo "PS2 ELF: $ELF_OUT"

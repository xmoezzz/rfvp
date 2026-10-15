#!/usr/bin/env bash
# Build the Rust part of the PS1 port as one R3000 object.
#
# rustc's MIPS backend cannot emit atomics for MIPS I (no sync, no ll/sc), and the engine's
# crates use them. So, as for the PS2, rustc only emits LLVM bitcode (for MIPS II, which has
# them); the atomics are lowered to plain loads and stores (the engine runs on one thread), and
# Homebrew's LLVM makes R3000 code from the result. Floats are soft-float: compiler_builtins is
# compiled from its embedded bitcode too, so the object needs nothing from libgcc.
#
# Usage: build_ps1_rust.sh <crate dir> <staticlib name>; writes $CARGO_TARGET_DIR/ps1/<name>.o
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
# shellcheck source=../consoles/toolchain.env
source "$ROOT/platform/consoles/toolchain.env"
CRATE="$1"
NAME="$2"
LLVM="${LLVM:-/opt/homebrew/opt/llvm/bin}"
TOOLCHAIN="${RFVP_CONSOLE_NIGHTLY}"
TARGET_JSON="$HERE/mipsel-sony-psx-rfvp.json"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
OUT="$TARGET_DIR/ps1"
mkdir -p "$OUT"

BUILD_ARGS=()
PANIC_FLAGS="-Cpanic=immediate-abort"
if [[ "${RFVP_PS1_DEBUG:-0}" == 1 ]]; then
    BUILD_ARGS+=(--no-default-features --features diagnostics)
    PANIC_FLAGS="-Cpanic=abort"
fi

(
    cd "$CRATE"
    # The smallest build: code and data share 2 MiB. No debug information, no panic or
    # location messages, no `Debug` formatting (the log is compiled out: see host/Cargo.toml).
    RUSTFLAGS="-C linker-plugin-lto -C embed-bitcode=yes -C debuginfo=0 -Zlocation-detail=none -Zfmt-debug=none -Zunstable-options $PANIC_FLAGS" \
    CARGO_PROFILE_RELEASE_OPT_LEVEL="${RFVP_PS1_OPT_LEVEL:-z}" CARGO_PROFILE_RELEASE_LTO=thin \
    CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
        cargo "+$TOOLCHAIN" build --release \
        -Zjson-target-spec -Z build-std=core,alloc,compiler_builtins \
        --target "$TARGET_JSON" --target-dir "$TARGET_DIR" "${BUILD_ARGS[@]}"
)

LIB="$TARGET_DIR/mipsel-sony-psx-rfvp/release/lib$NAME.a"
WORK="$OUT/$NAME.members"
rm -rf "$WORK" && mkdir -p "$WORK"
(cd "$WORK" && "$LLVM/llvm-ar" x "$LIB")

BITCODE=()
for member in "$WORK"/*; do
    if file -b "$member" | grep -q LLVM; then
        BITCODE+=("$member")
    elif [[ "$("$LLVM/llvm-readelf" -S "$member" 2>/dev/null)" == *.llvmbc* ]]; then
        # compiler_builtins: machine code for rustc's CPU, with its bitcode embedded.
        "$LLVM/llvm-objcopy" --dump-section ".llvmbc=$member.bc" "$member"
        BITCODE+=("$member.bc")
    fi
done
"$LLVM/llvm-link" "${BITCODE[@]}" -o "$OUT/$NAME.bc"
"$LLVM/llvm-dis" "$OUT/$NAME.bc" -o "$OUT/$NAME.ll"
# Per-function attributes name rustc's CPU; retarget them to the R3000.
python3 - "$OUT/$NAME.ll" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
text = path.read_text().replace('"target-cpu"="mips2"', '"target-cpu"="mips1"')
text = re.sub(r'"target-features"="[^"]*"', '"target-features"="+soft-float,+noabicalls"', text)
path.write_text(text)
PY
"$LLVM/opt" -passes=lower-atomic "$OUT/$NAME.ll" -o "$OUT/$NAME.lowered.bc"
# Whole program: everything but the entry points (rfvp_ps1_*) and the runtime routines code
# generation may call (__*, mem*) and the C heap (malloc...) becomes internal, so what nothing reaches is dropped; the PS1
# has 2 MiB for code and data together. Functions retain rustc's size attributes.
"$LLVM/llvm-nm" --defined-only --format=just-symbols "$OUT/$NAME.lowered.bc" \
    | grep -E '^(rfvp_ps1_|aot_|__|mem|rust_|malloc$|free$|calloc$|realloc$)' > "$OUT/$NAME.exports"
# Spend code bytes only on the small VM stack/operand routines. Cold resource,
# decoder and serialization code keeps its size attributes.
"$LLVM/llvm-nm" --defined-only --format=just-symbols "$OUT/$NAME.lowered.bc" > "$OUT/$NAME.symbols"
"$LLVM/llvm-cxxfilt" < "$OUT/$NAME.symbols" > "$OUT/$NAME.demangled"
python3 - "$OUT/$NAME.symbols" "$OUT/$NAME.demangled" "$OUT/$NAME.hot" <<'PY'
from pathlib import Path
import sys
symbols, names, output = map(Path, sys.argv[1:])
hot = []
for symbol, name in zip(symbols.read_text().splitlines(), names.read_text().splitlines()):
    if name.startswith('<rfvp::script::context::Context>::') and not any(
        word in name for word in ('snapshot', 'backtrace', '::new', 'debug')
    ):
        hot.append(symbol)
    elif name.startswith('<rfvp::script::parser::Parser>::fetch'):
        hot.append(symbol)
output.write_text('\n'.join(hot) + '\n')
PY
HOT_ARGS=()
while IFS= read -r symbol; do
    [[ -n "$symbol" ]] || continue
    HOT_ARGS+=(-force-remove-attribute="$symbol:minsize" -force-remove-attribute="$symbol:optsize")
done < "$OUT/$NAME.hot"
"$LLVM/opt" -passes='forceattrs,internalize,globaldce,default<O2>' "${HOT_ARGS[@]}" \
    -internalize-public-api-file="$OUT/$NAME.exports" \
    "$OUT/$NAME.lowered.bc" -o "$OUT/$NAME.opt.bc"
"$LLVM/llc" -mtriple=mipsel-unknown-none-elf -target-abi=o32 -mcpu=mips1 \
    -mattr=+soft-float,+noabicalls -relocation-model=static -mno-check-zero-division \
    -mips-ssection-threshold=0 \
    -function-sections -data-sections -filetype=obj "$OUT/$NAME.opt.bc" -o "$OUT/$NAME.o"

if "$LLVM/llvm-objdump" -d "$OUT/$NAME.o" | grep -qE $'\t(ll|sc|sync|lwc1|swc1|mtc1|mfc1)\t'; then
    echo "error: $OUT/$NAME.o has instructions the R3000 lacks" >&2
    exit 1
fi
echo "$OUT/$NAME.o"

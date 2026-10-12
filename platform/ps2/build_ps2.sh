#!/usr/bin/env bash
# Build the Rust part of the PS2 port as one R5900 object.
#
# rustc's LLVM has no R5900 CPU, so rustc only emits LLVM bitcode (target mips3, n32 ABI,
# single-float). LLVM 23 (Homebrew) then generates
# R5900 code from it: no 64-bit multiply/divide, no ll/sc, with the short-loop erratum fix.
# LLVM treats the R5900 as soft-float, so float arithmetic becomes calls to __addsf3 and friends
# with the soft-float convention (arguments in integer registers). EE GCC's libgcc has those
# routines built for its hard-float convention, so they must not be used: compiler_builtins is
# compiled here too (from the bitcode rustc embeds in it), and the object is checked to need
# nothing of the kind from outside. The memory functions come from newlib.
#
# Usage: build_ps2.sh <crate dir> <staticlib name>; writes target/ps2/<name>.o
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE="$1"
NAME="$2"
LLVM="${LLVM:-/opt/homebrew/opt/llvm/bin}"
TOOLCHAIN="${TOOLCHAIN:-nightly-2025-03-19}"
TARGET_JSON="$HERE/mips64r5900el-sony-ps2.json"
TARGET_DIR="${CARGO_TARGET_DIR:-$HERE/../../target}"
OUT="$TARGET_DIR/ps2"
mkdir -p "$OUT"

(
    cd "$CRATE"
    RUSTFLAGS="-C linker-plugin-lto -C embed-bitcode=yes" \
        cargo "+$TOOLCHAIN" build --release \
        -Z build-std=core,alloc \
        --target "$TARGET_JSON" --target-dir "$TARGET_DIR"
)

LIB="$TARGET_DIR/mips64r5900el-sony-ps2/release/lib$NAME.a"
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
# Per-function attributes name rustc's CPU; retarget them to the R5900.
python3 - "$OUT/$NAME.ll" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
text = path.read_text().replace('"target-cpu"="mips3"', '"target-cpu"="r5900"')
text = re.sub(r'"target-features"="[^"]*"', '"target-features"="+single-float,+noabicalls,+fix-r5900"', text)
path.write_text(text)
PY
# LLVM 23 expands Rust's saturating float-to-int casts wrongly for the (soft-float) R5900.
python3 "$HERE/fix_float_casts.py" "$OUT/$NAME.ll"
# rustc's single-threaded setting does not survive in bitcode: lower the atomics to plain loads
# and stores here (the R5900 has no ll/sc, and the port runs the engine on one EE thread).
"$LLVM/opt" -passes=lower-atomic "$OUT/$NAME.ll" -o "$OUT/$NAME.lowered.bc"
"$LLVM/llc" -O2 -mtriple=mips64el-unknown-none-gnuabin32 -target-abi=n32 -mcpu=r5900 \
    -mattr=+single-float,+noabicalls,+fix-r5900 -relocation-model=static \
    -function-sections -data-sections -filetype=asm "$OUT/$NAME.lowered.bc" -o "$OUT/$NAME.s"
# The R5900's FPU compares only with c.f, c.eq, c.olt and c.ole; LLVM also emits the unordered
# forms, which it lacks. Its FPU makes no NaN, so they mean the ordered ones (c.un: never).
perl -pi -e 's/\tc\.un\.s\t/\tc.f.s\t/; s/\tc\.ueq\.s\t/\tc.eq.s\t/; s/\tc\.ult\.s\t/\tc.olt.s\t/; s/\tc\.ule\.s\t/\tc.ole.s\t/' \
    "$OUT/$NAME.s"
# LLVM leaves single-precision arithmetic to library calls on the R5900 (its FPU is not IEEE:
# no infinities, NaN or denormals), which is far too slow for the engine's motions and the
# scripts' arithmetic. The calls already pass floats in FPU registers, so point them at the
# FPU's own instructions, as GCC's PS2 code uses them.
perl -pi -e 's/\tjal\t(__addsf3|__subsf3|__mulsf3|__divsf3|sqrtf)$/\tjal\t__rfvp_fpu_$1/' "$OUT/$NAME.s"
cat >> "$OUT/$NAME.s" <<'FPU'
	.text
	.set	noreorder
	.globl	__rfvp_fpu___addsf3
__rfvp_fpu___addsf3:
	jr	$ra
	add.s	$f0, $f12, $f13
	.globl	__rfvp_fpu___subsf3
__rfvp_fpu___subsf3:
	jr	$ra
	sub.s	$f0, $f12, $f13
	.globl	__rfvp_fpu___mulsf3
__rfvp_fpu___mulsf3:
	jr	$ra
	mul.s	$f0, $f12, $f13
	.globl	__rfvp_fpu___divsf3
__rfvp_fpu___divsf3:
	jr	$ra
	div.s	$f0, $f12, $f13
	.globl	__rfvp_fpu_sqrtf
__rfvp_fpu_sqrtf:
	jr	$ra
	sqrt.s	$f0, $f12
	.set	reorder
FPU
"$LLVM/llvm-mc" -triple=mips64el-unknown-none-gnuabin32 -target-abi=n32 -mcpu=r5900 \
    -mattr=+single-float,+noabicalls -filetype=obj "$OUT/$NAME.s" -o "$OUT/$NAME.o"

if "$LLVM/llvm-objdump" -d "$OUT/$NAME.o" | grep -qE $'\t(dmult|dmultu|ddiv|ddivu|ll|sc|lld|scd|c\\.(un|ueq|ult|ule|sf|ngle|seq|ngl|lt|nge|le|ngt)\\.[sd])\t'; then
    echo "error: $OUT/$NAME.o has instructions the R5900 lacks" >&2
    exit 1
fi
SOFT_FLOAT='^(__(add|sub|mul|div|neg|eq|ne|lt|le|gt|ge|unord|cmp)[sd]f[23]|__(extend|trunc)[sd]f[sd]f2|__float(un)?[sdt]i[sd]f|__fix(uns)?[sd]f[sdt]i)$'
if "$LLVM/llvm-nm" -u "$OUT/$NAME.o" | awk '{print $NF}' | grep -qE "$SOFT_FLOAT"; then
    echo "error: $OUT/$NAME.o needs soft-float routines from outside (EE libgcc's are hard-float):" >&2
    "$LLVM/llvm-nm" -u "$OUT/$NAME.o" | awk '{print $NF}' | grep -E "$SOFT_FLOAT" >&2
    exit 1
fi
echo "$OUT/$NAME.o"

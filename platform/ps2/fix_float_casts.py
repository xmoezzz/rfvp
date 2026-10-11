#!/usr/bin/env python3
"""Replace LLVM's saturating float-to-int intrinsics in a textual IR module.

Rust's `as` from a float to an integer is `llvm.fpto[su]i.sat`. LLVM 23 expands those wrongly
for the R5900, which it treats as soft-float (`68.0f32 as i32` gave i32::MAX, `as u32` 0); its
soft-float `fcmp oge` is wrong too. Plain `fptosi`/`fptoui` become calls into compiler_builtins,
which already saturate as Rust does, so each intrinsic becomes such a conversion (to 64 bits for
the narrower types, then clamped with integer compares).

Usage: fix_float_casts.py <module.ll>   (rewritten in place)
"""

import re
import sys

INTRINSIC = re.compile(r"@llvm\.fpto([su])i\.sat\.i(\d+)\.(f32|f64)\b")
FLOAT_TYPE = {"f32": "float", "f64": "double"}


def definition(signed, bits, src):
    ty = FLOAT_TYPE[src]
    it = f"i{bits}"
    name = f"@__rfvp_fpto{signed}i_sat_{it}_{src}"
    head = f"define internal {it} {name}({ty} %x) alwaysinline nounwind {{\n"
    if bits >= 64:
        # compiler_builtins' __fix*di / __fix*ti already saturate as Rust does (NaN gives 0).
        convert = "fptosi" if signed == "s" else "fptoui"
        return head + f"  %r = {convert} {ty} %x to {it}\n  ret {it} %r\n}}\n"
    # Narrower: convert to i64 (saturating, as above), then clamp with integer compares.
    if signed == "s":
        min_value, max_value = -(2 ** (bits - 1)), 2 ** (bits - 1) - 1
    else:
        min_value, max_value = 0, 2 ** bits - 1
    return head + (
        f"  %w = fptosi {ty} %x to i64\n"
        f"  %below = icmp slt i64 %w, {min_value}\n"
        f"  %above = icmp sgt i64 %w, {max_value}\n"
        f"  %lo = select i1 %below, i64 {min_value}, i64 %w\n"
        f"  %hi = select i1 %above, i64 {max_value}, i64 %lo\n"
        f"  %r = trunc i64 %hi to {it}\n"
        f"  ret {it} %r\n"
        f"}}\n"
    )


def main():
    path = sys.argv[1]
    with open(path) as f:
        text = f.read()
    used = sorted(set(INTRINSIC.findall(text)))
    if not used:
        return
    # Drop the intrinsics' declarations, then point the calls at the replacements.
    text = re.sub(r"^declare [^\n]*@llvm\.fpto[su]i\.sat\.i\d+\.f(32|64)\b[^\n]*\n", "", text,
                  flags=re.M)
    text = INTRINSIC.sub(lambda m: f"@__rfvp_fpto{m.group(1)}i_sat_i{m.group(2)}_{m.group(3)}",
                         text)
    text += "\n" + "".join(definition(s, int(b), src) for s, b, src in used)
    with open(path, "w") as f:
        f.write(text)


if __name__ == "__main__":
    main()

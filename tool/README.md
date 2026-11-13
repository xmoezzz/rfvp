# Offline HCB translation merge

`hcb_patch.py` merges the binary `patch.dat` format originally proposed in
[#57](https://github.com/xmoezzz/rfvp/pull/57) into a new, standard HCB. It needs
Python 3.8 or newer and the standard library only.

```sh
python3 tool/hcb_patch.py /path/to/original.hcb /path/to/patch.dat \
  --input-encoding sjis --patch-encoding gbk --output-encoding utf8 \
  -o /path/to/Translated.hcb
```

Keep the original HCB used to create the patch: record offsets refer to its
`PushString` payloads. The output must be a new file. No runtime patch loader,
DLL scanning, image conversion, archive repacking, or changes to game assets
are involved. This tool does not download anything.

On console ports, select the resulting script through the existing game
directory's `config.toml`:

```toml
[game]
hcb = "Translated.hcb"
nls = "utf8"
```

Other hosts can select the HCB and encoding through their normal launch options.
All script literals, the game title, and syscall names are encoded consistently
in the selected output encoding, including strings not listed in the patch.
`sjis` uses Windows CP932; `gbk` uses GBK; `utf8` uses UTF-8. Invalid or
unrepresentable text fails instead of being replaced with question marks.

## Bytecode layout

The default `--layout preserve` keeps original instruction slots, entry points,
branch operands, and integer operands at their original addresses. A shorter
string is NUL-padded in its slot. A growing string is emitted before the system
description and reached through ordinary HCB `Jmp` instructions, with a jump
back after the literal. This avoids guessing which integers contain thread
entry addresses. It requires at least five bytes in each growing string's
original instruction slot; a smaller slot produces an error with its offset.

For these short slots, `--layout relocate` reassembles the bytecode, updating
the entry point and every direct `Call`, `Jmp`, and `Jz` operand. HCB has no
relocation table for integers used as callback addresses. When an integer
coincides with a moved instruction/string address, the tool stops and lists
the original integer-push opcode offsets requiring classification. Repeat
`--address-push 0xOFFSET` for address operands and `--literal-push 0xOFFSET`
for ordinary numbers. Address operands are widened to `PushI32` if necessary.
These offsets identify the **original push instruction**, not its value or
the translation record. Classifications must come from the script's semantics;
the tool does not infer them from numerical coincidence. Computed addresses
also require source review before using the relocating layout.

Neither layout makes old saves or read-text flags interchangeable with the
rewritten script: moving a string changes its native constant-string identity.
Keep separate saves for the new HCB.

## Patch format and limits

Raw patches contain a little-endian `u32` record count, then records of
`u32 original_payload_offset`, `u16 byte_length`, and that many encoded bytes.
Nonempty records include a terminating NUL and no interior NUL. A zero length
retains the original literal; a single NUL replaces it with an empty string.

Compressed patches have a 16-byte header: a zero first word, a reserved word,
the uncompressed size at byte 8, and the compressed size at byte 12, followed
by one zlib stream. Sizes, stream termination, duplicate offsets, instruction
boundaries, and string encodings are checked before writing anything.

The native HCB format allows at most 254 encoded text bytes per literal/title/
syscall name. Overlong strings fail; the tool does not split them into runtime
string additions, which would change constant-string semantics. Unknown
opcodes, custom syscall tables, and unrecognized metadata tails also fail.
Patch input is limited to 16 MiB and HCB input/output to 256 MiB. Inputs and
existing output files are never overwritten.

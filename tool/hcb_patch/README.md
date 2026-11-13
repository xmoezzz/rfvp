# HCB translation merge

`rfvp-hcb-patch` merges the binary `patch.dat` format originally proposed in
[#57](https://github.com/xmoezzz/rfvp/pull/57) into a new standard HCB using
RFVP's existing `disassembler` and `assembler` crates.

```sh
cargo build --release -p rfvp-hcb-patch
./target/release/rfvp-hcb-patch /path/to/original.hcb /path/to/patch.dat \
  --input-encoding sjis --patch-encoding gbk --output-encoding utf8 \
  -o /path/to/Translated.hcb
```

The tool disassembles the original script, replaces `push_string` operands by
their original payload offsets, and sends that project to the existing
assembler. The assembler owns instruction sizing, entry-point relocation,
`Call`/`Jmp`/`Jz` linking, `ThreadStart` pointer relocation, and metadata emission.
There is no separate bytecode encoder, jump-island layout, or runtime patch
loader. Intermediate YAML/TOML projects are temporary and removed on exit.
Images and archives are unchanged.

Use the exact original HCB used to create the patch. The output must be a new
file. On console hosts, select it in the game directory's existing configuration:

```toml
[game]
hcb = "Translated.hcb"
nls = "utf8"
```

Other hosts select the HCB and encoding through their normal launch options.
All strings and metadata use the chosen output encoding. Keep separate saves
and read-text flags for the newly assembled script because its addresses change.

Raw patches contain a little-endian `u32` count followed by records of
`u32 original_payload_offset`, `u16 byte_length`, and that many encoded bytes.
Nonempty records include a terminating NUL and no interior NUL. A zero-length
record retains the original literal; a single NUL replaces it with an empty one.
Compressed patches use the original 16-byte wrapper: zero first word, reserved
word, uncompressed size at byte 8, compressed size at byte 12, then zlib data.

The tool checks record bounds, duplicate/invalid offsets, and patch encoding
before assembly. The assembler rejects unrepresentable or overlong strings
(native maximum: 254 encoded text bytes), and retains its existing explicit
failures for unsupported custom syscalls and ambiguous/unresolved `ThreadStart`
addresses. Arbitrary computed code pointers are not inferred by the linker.
Patch input is limited to 16 MiB and HCB input/output to 256 MiB. Source files
and existing outputs are never overwritten. The tool does not download data.

## Substitute-character translations

The same package also builds `rfvp-hcb-restore`, the offline character-table
conversion separated from #58:

```sh
./target/release/rfvp-hcb-restore extract /path/to/game.dll -o /path/to/jis_map.bin
./target/release/rfvp-hcb-restore rebuild /path/to/original.hcb \
  --table /path/to/jis_map.bin --all-strings -o /path/to/Chinese.hcb
```

Extraction reads only the named file and proposes its longest aligned run of
UTF-16LE character pairs. This is a heuristic candidate, not a verified format
signature. Use `--offset 0xOFFSET --count PAIRS` to extract a known table range.
The resulting table is a plain sequence of `(u16 source, u16 destination)` pairs;
invalid Unicode and duplicate keys are rejected.

Rebuilding uses the existing disassembler and assembler, with Shift-JIS input
and UTF-8 output by default. `--all-strings` explicitly includes resource-path
literals. To restrict restoration to dialogue/UI strings, instead pass
`--offsets file.txt`, containing original payload offsets (one decimal or
`0xHEX` offset per line). Titles and syscall names are not character-remapped.
The engine never scans an EXE/DLL, installs a mapping, or changes font priority.

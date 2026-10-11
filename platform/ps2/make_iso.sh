#!/usr/bin/env bash
# Make a PS2 disc image of a game.
#
#   make_iso.sh <repacked game> <out.iso> [title]
#
# <repacked game> is the output of `rfvp-rebuilder --platform psp` (loose folders). A disc
# cannot hold that as it is: ISO 9660, as the PS2 reads it, allows upper-case Latin names only,
# and a game's files have Japanese names. So each asset folder goes on the disc as a pack in the
# original engine's format (FOLDER.BIN, with the names inside), and the files at the top keep
# their names in upper case. The ELF (target/ps2/rfvp-ps2.elf, `make -C platform/ps2/runtime`)
# boots through SYSTEM.CNF; the game saves to the memory card in slot 1 (mc0:/RFVP).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GAME="$1"
OUT="$2"
TITLE="${3:-RFVP}"
ELF="${ELF:-$HERE/../../target/ps2/rfvp-ps2.elf}"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

[[ -f "$ELF" ]] || { echo "error: $ELF not found (make -C platform/ps2/runtime)" >&2; exit 1; }

python3 - "$GAME" "$STAGE" <<'PY'
import os, struct, sys, shutil

game, stage = sys.argv[1], sys.argv[2]
# Files of a repack that are not the game's.
SKIP_FILES = {"debug.txt", "debug_report.txt", "debug_screen.bmp", "last_error.txt",
              "profile.json", "readme.txt"}
SKIP_DIRS = {"save", "saves"}

def pack(folder, out_path):
    """Write `folder` as an engine pack: u32 count, u32 name table size, (name offset,
    data offset, size) per file, NUL-terminated Shift-JIS names, then the data."""
    names = sorted(n for n in os.listdir(folder) if os.path.isfile(os.path.join(folder, n)))
    table = bytearray()
    offsets = []
    for name in names:
        offsets.append(len(table))
        table += name.encode("shift_jis") + b"\0"
    header_size = 8 + 12 * len(names) + len(table)
    entries = []
    data_offset = header_size
    for name, name_offset in zip(names, offsets):
        size = os.path.getsize(os.path.join(folder, name))
        entries.append((name_offset, data_offset, size))
        data_offset += size
    if data_offset >= 1 << 32:
        sys.exit(f"error: {folder} is over 4 GiB")
    with open(out_path, "wb") as out:
        out.write(struct.pack("<II", len(names), len(table)))
        for entry in entries:
            out.write(struct.pack("<III", *entry))
        out.write(table)
        for name in names:
            with open(os.path.join(folder, name), "rb") as src:
                shutil.copyfileobj(src, out, 1 << 20)
    print(f"  {os.path.basename(out_path)}: {len(names)} files, {data_offset >> 20} MiB")

for name in sorted(os.listdir(game)):
    path = os.path.join(game, name)
    if os.path.isdir(path):
        if name.lower() not in SKIP_DIRS:
            pack(path, os.path.join(stage, name.upper() + ".BIN"))
    elif name.lower() not in SKIP_FILES and not name.lower().endswith(".exe"):
        shutil.copyfile(path, os.path.join(stage, name.upper()))
PY

cp "$ELF" "$STAGE/RFVP_PS2.ELF"
printf 'BOOT2 = cdrom0:\\RFVP_PS2.ELF;1\r\nVER = 1.00\r\nVMODE = NTSC\r\n' > "$STAGE/SYSTEM.CNF"

# The boot files and the small ones first: the BIOS reads the ELF from the start of the disc.
SORT="$(mktemp)"
trap 'rm -rf "$STAGE" "$SORT"' EXIT
for f in "$STAGE"/*; do
    case "$f" in
        *.BIN) echo "$f 0" ;;
        *) echo "$f 100" ;;
    esac
done > "$SORT"
# ISO level 2: names up to 31 characters (RFVP.TOML, DEFUALT.TMAP), upper case, ";1" versions.
mkisofs -quiet -iso-level 2 -udf -sort "$SORT" -V "$TITLE" -sysid PLAYSTATION -A PLAYSTATION \
    -o "$OUT" "$STAGE"
ls -l "$OUT"

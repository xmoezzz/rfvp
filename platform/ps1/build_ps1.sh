#!/usr/bin/env bash
# Build the PlayStation executable, and optionally a converted game's disc image.
#
#   build_ps1.sh [converted game]
#
# <converted game> is the output of `rfvp-psx-convert` (the disc's files, iso.xml). Needs
# PSn00bSDK (PSN00BSDK, default ~/psn00bsdk), a mipsel-none-elf GCC (PSN00BSDK_TC, default
# ~/ps2dev/iop), LLVM and the nightly in consoles/toolchain.env. Output: the EXE in
# target/rfvp-ps1/, and the disc (rfvp.bin, rfvp.cue) in the converted game's folder.
#
# The build is for a stock console's 2 MiB; RFVP_PS1_RAM=8 builds for the 8 MiB of a
# development console instead.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GAME=""
if [[ $# -gt 0 ]]; then GAME="$(cd "$1" && pwd)"; fi
PSN00BSDK="${PSN00BSDK:-$HOME/psn00bsdk}"
PSN00BSDK_TC="${PSN00BSDK_TC:-$HOME/ps2dev/iop}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
OUT="${RFVP_PS1_OUT_DIR:-$ROOT/target/rfvp-ps1}"
case "${RFVP_PS1_RAM:-2}" in
  8) RAM_TOP=0x80800000; STACK=807FFFF0 ;;
  *) RAM_TOP=0x80200000; STACK=801FFFF0 ;;
esac

CARGO_TARGET_DIR="$TARGET_DIR" bash "$HERE/build_ps1_rust.sh" "$HERE/host" rfvp_ps1
cmake -S "$HERE/runtime" -B "$TARGET_DIR/rfvp-ps1-runtime" -G Ninja \
  -DCMAKE_TOOLCHAIN_FILE="$PSN00BSDK/lib/libpsn00b/cmake/sdk.cmake" \
  -DPSN00BSDK_TC="$PSN00BSDK_TC" -DPSN00BSDK_TARGET=mipsel-none-elf \
  -DRFVP_RUST_OBJECT="$TARGET_DIR/ps1/rfvp_ps1.o" -DRFVP_RAM_TOP="$RAM_TOP" >/dev/null
# Quiet unless it fails (ninja reports errors on stdout).
if ! cmake --build "$TARGET_DIR/rfvp-ps1-runtime" > "$TARGET_DIR/rfvp-ps1-runtime.log"; then
  cat "$TARGET_DIR/rfvp-ps1-runtime.log" >&2
  exit 1
fi
mkdir -p "$OUT"
# The PS-X EXE, from the ELF's loaded sections. (PSn00bSDK's elf2x also counts the GNU_STACK
# header the linker adds for objects without .note.GNU-stack, and makes a 2 GiB file.)
ELF="$TARGET_DIR/rfvp-ps1-runtime/rfvp.elf"
"$PSN00BSDK_TC/bin/mipsel-none-elf-objcopy" -O binary -j .text -j .rodata -j .ctors -j .dtors -j .data \
  "$ELF" "$OUT/rfvp.text"
python3 - "$ELF" "$OUT/rfvp.text" "$OUT/rfvp.exe" <<'PY'
import struct, sys
elf, text, out = sys.argv[1], sys.argv[2], sys.argv[3]
e = open(elf, "rb").read()
entry = struct.unpack_from("<I", e, 24)[0]
shoff, = struct.unpack_from("<I", e, 32)
shentsize, shnum, shstrndx = struct.unpack_from("<HHH", e, 46)
sections = [struct.unpack_from("<IIIIIIIIII", e, shoff + i * shentsize) for i in range(shnum)]
names = sections[shstrndx]
name = lambda s: e[names[4] + s[0]:e.index(b"\0", names[4] + s[0])].decode()
addr = {name(s): s for s in sections}
t_addr = addr[".text"][3]
bss = addr[".bss"]
body = open(text, "rb").read()
body += b"\0" * (-len(body) % 2048)
head = bytearray(2048)
head[0:8] = b"PS-X EXE"
struct.pack_into("<IIIIIIII", head, 0x10, entry, 0, t_addr, len(body), 0, 0, bss[3], bss[5])
head[0x4c:0x4c + 47] = b"Sony Computer Entertainment Inc. for Japan area"
open(out, "wb").write(bytes(head) + body)
print(f"rfvp.exe: {len(body)} bytes at {t_addr:#x}, entry {entry:#x}, bss {bss[5]} bytes")
PY

if [[ -z "$GAME" ]]; then
  echo "[ps1] $OUT/rfvp.exe"
  exit 0
fi
cp "$OUT/rfvp.exe" "$GAME/RFVP.EXE"
printf 'BOOT=cdrom:\\RFVP.EXE;1\r\nTCB=4\r\nEVENT=10\r\nSTACK=%s\r\n' "$STACK" > "$GAME/SYSTEM.CNF"
# DEBUG.TXT (see main.c), when the converted game has one.
XML=iso.xml
if [[ -f "$GAME/DEBUG.TXT" ]]; then
  sed 's|<dummy|<file name="DEBUG.TXT" type="data" source="DEBUG.TXT"/><dummy|' "$GAME/iso.xml" > "$GAME/iso_debug.xml"
  XML=iso_debug.xml
fi
(cd "$GAME" && "$PSN00BSDK/bin/mkpsxiso" -y -q "$XML")
echo "[ps1] $GAME/rfvp.bin"

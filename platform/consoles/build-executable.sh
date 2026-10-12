#!/usr/bin/env bash
# Compile one console runtime. Game conversion and disc packaging are separate.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
PLATFORM="${1:?usage: build-executable.sh ps1|ps2|psp|3ds|ps3|psv|switch}"
case "$PLATFORM" in ps1|ps2|psp|3ds|ps3|psv|switch) ;; *) echo "unknown platform: $PLATFORM" >&2; exit 1 ;; esac
export CARGO_TARGET_DIR="$ROOT/target/console-$PLATFORM"
mkdir -p "$ROOT/dist/consoles"
EXTRA_SOURCE=""
EXTRA_DESTINATION=""
case "$PLATFORM" in
    ps1)
        RFVP_PS1_OUT_DIR="$CARGO_TARGET_DIR/ps1-exe" bash platform/ps1/build_ps1.sh
        SOURCE="$CARGO_TARGET_DIR/ps1-exe/rfvp.exe"
        DESTINATION="$ROOT/dist/consoles/rfvp-ps1.exe"
        ;;
    ps2)
        make -C platform/ps2/runtime OUT="$CARGO_TARGET_DIR/ps2"
        SOURCE="$CARGO_TARGET_DIR/ps2/rfvp-ps2.elf"
        DESTINATION="$ROOT/dist/consoles/rfvp-ps2.elf"
        ;;
    psp)
        bash platform/psp/build_psp.sh --locked
        BOOTS=()
        while IFS= read -r boot; do BOOTS+=("$boot"); done < <(find "$CARGO_TARGET_DIR/mipsel-sony-psp/release" -type f -iname '*.pbp' | sort)
        [[ ${#BOOTS[@]} == 1 ]] || { echo "expected one PSP EBOOT, found ${#BOOTS[@]}" >&2; exit 1; }
        SOURCE="${BOOTS[0]}"
        DESTINATION="$ROOT/dist/consoles/EBOOT.PBP"
        ;;
    3ds)
        bash platform/3ds/build_3ds.sh
        SOURCE="$ROOT/target/rfvp-3ds/3ds/rfvp.3dsx"
        DESTINATION="$ROOT/dist/consoles/rfvp.3dsx"
        ;;
    ps3)
        bash platform/ps3/build_ps3.sh
        SOURCE="$ROOT/target/rfvp-ps3/ps3/rfvp.elf"
        DESTINATION="$ROOT/dist/consoles/rfvp-ps3.elf"
        EXTRA_SOURCE="$ROOT/target/rfvp-ps3/ps3/rfvp.self"
        EXTRA_DESTINATION="$ROOT/dist/consoles/rfvp-ps3.self"
        ;;
    psv)
        bash platform/vita/build_vita.sh --executable-only
        SOURCE="$ROOT/target/rfvp-psv/psv/eboot.bin"
        DESTINATION="$ROOT/dist/consoles/rfvp-psv-eboot.bin"
        ;;
    switch)
        bash platform/switch/build_switch.sh
        SOURCE="$ROOT/target/rfvp-switch/switch/rfvp.nro"
        DESTINATION="$ROOT/dist/consoles/rfvp.nro"
        ;;
esac
python3 - "$PLATFORM" "$SOURCE" "$EXTRA_SOURCE" <<'PY'
import pathlib, struct, sys
platform, source, extra_source = sys.argv[1:]
path = pathlib.Path(source)
with path.open('rb') as file:
    header = file.read(64)
magic = {'ps1': b'PS-X EXE', 'ps2': b'\x7fELF', 'psp': b'\0PBP',
         '3ds': b'3DSX', 'ps3': b'\x7fELF', 'psv': b'SCE\0', 'switch': b'NRO0'}[platform]
offset = 16 if platform == 'switch' else 0
if path.stat().st_size <= 64 or header[offset:offset + len(magic)] != magic:
    raise SystemExit(f'Invalid {platform} executable: {path}')
if platform == 'ps2' and (header[4:6] != b'\x01\x01' or struct.unpack_from('<H', header, 18)[0] != 8):
    raise SystemExit(f'Expected a little-endian ELF32 MIPS executable: {path}')
if platform == 'ps3':
    if header[4:6] != b'\x02\x02' or struct.unpack_from('>H', header, 18)[0] != 21:
        raise SystemExit(f'Expected a big-endian ELF64 PowerPC executable: {path}')
    self_path = pathlib.Path(extra_source)
    with self_path.open('rb') as file:
        self_magic = file.read(4)
    if self_path.stat().st_size <= 64 or self_magic != b'SCE\0':
        raise SystemExit(f'Invalid PS3 SELF executable: {self_path}')
PY
cp "$SOURCE" "$DESTINATION"
echo "$DESTINATION"
if [[ -n "$EXTRA_SOURCE" ]]; then
    cp "$EXTRA_SOURCE" "$EXTRA_DESTINATION"
    echo "$EXTRA_DESTINATION"
fi

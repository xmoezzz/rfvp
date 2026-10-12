#!/usr/bin/env bash
# Compile one console runtime. Game conversion and disc packaging are separate.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
PLATFORM="${1:?usage: build-executable.sh ps1|ps2|psp|3ds}"
case "$PLATFORM" in ps1|ps2|psp|3ds) ;; *) echo "unknown platform: $PLATFORM" >&2; exit 1 ;; esac
export CARGO_TARGET_DIR="$ROOT/target/console-$PLATFORM"
mkdir -p "$ROOT/dist/consoles"
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
esac
python3 - "$PLATFORM" "$SOURCE" <<'PY'
import pathlib, struct, sys
platform, source = sys.argv[1:]
path = pathlib.Path(source)
with path.open('rb') as file:
    header = file.read(64)
magic = {'ps1': b'PS-X EXE', 'ps2': b'\x7fELF', 'psp': b'\0PBP', '3ds': b'3DSX'}[platform]
if path.stat().st_size <= 64 or not header.startswith(magic):
    raise SystemExit(f'Invalid {platform} executable: {path}')
if platform == 'ps2' and (header[4:6] != b'\x01\x01' or struct.unpack_from('<H', header, 18)[0] != 8):
    raise SystemExit(f'Expected a little-endian ELF32 MIPS executable: {path}')
PY
cp "$SOURCE" "$DESTINATION"
echo "$DESTINATION"

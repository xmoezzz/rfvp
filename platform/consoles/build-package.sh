#!/usr/bin/env bash
# Build one console runtime, convert the original demo, and make a release ZIP.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
source platform/consoles/toolchain.env
PLATFORM="${1:?usage: build-package.sh ps1|ps2|psp|3ds}"
case "$PLATFORM" in ps1|ps2|psp|3ds) ;; *) echo "unknown platform: $PLATFORM" >&2; exit 1 ;; esac
export CARGO_TARGET_DIR="$ROOT/target/console-$PLATFORM"
WORK="$CARGO_TARGET_DIR/package"
mkdir -p "$WORK" "$ROOT/dist/consoles"
python3 platform/consoles/make-demo.py "$WORK/source"

if [[ "$PLATFORM" == ps1 ]]; then
    cargo +stable build --locked --release -p rfvp-psx
    "$CARGO_TARGET_DIR/release/rfvp-psx-convert" \
        --input "$WORK/source" --output "$WORK/game" --no-movies --threads 2
    RFVP_PS1_OUT_DIR="$CARGO_TARGET_DIR/ps1-exe" bash platform/ps1/build_ps1.sh "$WORK/game"
else
    cargo +stable build --locked --release -p rfvp-rebuilder
    "$CARGO_TARGET_DIR/release/rfvp-rebuilder" --platform "$PLATFORM" \
        --input "$WORK/source" --output "$WORK/game"
    case "$PLATFORM" in
        ps2)
            make -C platform/ps2/runtime OUT="$CARGO_TARGET_DIR/ps2"
            mkdir -p "$WORK/disc"
            cp -R "$WORK/game/." "$WORK/disc/"
            cp "$CARGO_TARGET_DIR/ps2/rfvp-ps2.elf" "$WORK/disc/RFVP.ELF"
            printf 'BOOT2 = cdrom0:\\RFVP.ELF;1\r\nVER = 1.00\r\nVMODE = NTSC\r\n' > "$WORK/disc/SYSTEM.CNF"
            xorriso -as mkisofs -iso-level 2 -V RFVP -o "$WORK/rfvp-ps2.iso" "$WORK/disc"
            ;;
        psp) bash platform/psp/build_psp.sh ;;
        3ds) bash platform/3ds/build_3ds.sh ;;
    esac
fi
python3 platform/consoles/package.py "$PLATFORM" "$WORK" "$ROOT/dist/consoles"

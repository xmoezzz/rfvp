#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
EMULATOR="/Applications/DuckStation.app/Contents/MacOS/DuckStation"
DISC="${1:-$SCRIPT_DIR/output/disc/rfvp.cue}"

[[ -x "$EMULATOR" ]] || { echo "DuckStation is not installed." >&2; exit 1; }
[[ -f "$DISC" ]] || { echo "Disc image not found: $DISC" >&2; exit 1; }

exec "$EMULATOR" -fastboot -nofullscreen -- "$DISC"

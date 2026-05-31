#!/usr/bin/env bash
set -euo pipefail

cargo doc --workspace --no-deps --target-dir docs-build \
  --exclude rfvp-3ds \
  --exclude rfvp-horizon \
  --exclude rfvp-ps2 \
  --exclude rfvp-ps3 \
  --exclude rfvp-psp \
  --exclude rfvp-psv \
  --exclude rfvp-wii \
  --exclude rfvp-wiiu
mv docs-build/doc docs
rm -rf docs-build

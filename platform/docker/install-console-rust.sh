#!/usr/bin/env bash
set -euo pipefail
source /tmp/toolchain.env
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh
bash /tmp/rustup-init.sh -y --profile minimal --default-toolchain "$RFVP_CONSOLE_NIGHTLY"
rustup component add rust-src --toolchain "$RFVP_CONSOLE_NIGHTLY"
if [[ "${1:-}" == legacy ]]; then
    rustup toolchain install nightly-2025-03-19 --profile minimal --component rust-src
fi
rm /tmp/rustup-init.sh

#!/usr/bin/env bash
if [ -z "${BASH_VERSION:-}" ]; then
    exec bash "$0" "$@"
fi
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

if ! command -v cargo-psp >/dev/null 2>&1; then
    echo "error: cargo-psp is not installed or cargo-psp is not available in PATH" >&2
    echo "hint: cargo install cargo-psp" >&2
    exit 1
fi

if ! rustc +nightly-2025-03-19 --version >/dev/null 2>&1; then
    echo "error: nightly-2025-03-19 Rust toolchain is not available" >&2
    echo "hint: rustup toolchain install nightly-2025-03-19" >&2
    exit 1
fi

if ! rustup component list --toolchain nightly-2025-03-19 2>/dev/null | grep -q '^rust-src .*installed'; then
    echo "error: rust-src is not installed for nightly-2025-03-19" >&2
    echo "hint: rustup component add rust-src --toolchain nightly-2025-03-19" >&2
    exit 1
fi

(
    cd "$WORKSPACE_DIR/crates/rfvp-psp"
    cargo +nightly-2025-03-19 psp --features entrypoint --release "$@"
)
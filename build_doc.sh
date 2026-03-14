#!/usr/bin/env bash
set -euo pipefail

cargo doc --workspace --no-deps --target-dir docs-build
mv docs-build/doc docs
rm -rf docs-build
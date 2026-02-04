#!/usr/bin/env bash
set -euo pipefail

./platform/scripts/clean_ios_build.sh
./platform/scripts/clean_android_build.sh

echo "[clean] Cleaned platform build artifacts."

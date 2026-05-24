#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLATFORM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$PLATFORM_DIR/.." && pwd)"

UEFI_DIR="$PLATFORM_DIR/uefi"
UEFI_MANIFEST="$ROOT_DIR/crates/rfvp/uefi_app/Cargo.toml"
UEFI_APP_DIR="$(cd "$(dirname "$UEFI_MANIFEST")" && pwd)"

TGT_X86_64="x86_64-unknown-uefi"
TGT_AARCH64="aarch64-unknown-uefi"

OUT_X86_64="$UEFI_DIR/x86_64/esp/EFI/BOOT/BOOTX64.EFI"
OUT_AARCH64="$UEFI_DIR/aarch64/esp/EFI/BOOT/BOOTAA64.EFI"

command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found" >&2; exit 1; }
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup not found" >&2; exit 1; }

[[ -f "$UEFI_MANIFEST" ]] || { echo "ERROR: Missing UEFI manifest: $UEFI_MANIFEST" >&2; exit 1; }

find_single_efi() {
  local target="$1"
  local release_dir="$UEFI_APP_DIR/target/$target/release"

  if [[ ! -d "$release_dir" ]]; then
    echo "ERROR: Missing release directory: $release_dir" >&2
    exit 1
  fi

  local count
  count="$(find "$release_dir" -maxdepth 1 -type f -name '*.efi' | wc -l | tr -d ' ')"

  if [[ "$count" != "1" ]]; then
    echo "ERROR: expected exactly one EFI file under $release_dir, found $count" >&2
    find "$release_dir" -maxdepth 1 -type f -name '*.efi' -print >&2
    exit 1
  fi

  find "$release_dir" -maxdepth 1 -type f -name '*.efi' -print
}

build_target() {
  local target="$1"
  local output_efi="$2"

  echo "[uefi] Ensuring Rust target: $target"
  rustup target add "$target" >/dev/null 2>&1 || true

  echo "[uefi] Building: $target"
  pushd "$ROOT_DIR" >/dev/null
  cargo build --manifest-path "$UEFI_MANIFEST" --release --target "$target"
  popd >/dev/null

  local built_efi
  built_efi="$(find_single_efi "$target")"

  mkdir -p "$(dirname "$output_efi")"
  cp "$built_efi" "$output_efi"

  echo "[uefi] Built artifact: $built_efi"
  echo "[uefi] OK: $output_efi"
}

build_target "$TGT_X86_64" "$OUT_X86_64"
build_target "$TGT_AARCH64" "$OUT_AARCH64"

echo "[uefi] Done."
echo "[uefi] x86_64 ESP:  $UEFI_DIR/x86_64/esp"
echo "[uefi] aarch64 ESP: $UEFI_DIR/aarch64/esp"

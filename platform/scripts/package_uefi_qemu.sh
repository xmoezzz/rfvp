#!/usr/bin/env bash
set -euo pipefail
shopt -s nullglob

echo "[uefi-qemu] package_uefi_qemu.sh started"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLATFORM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$PLATFORM_DIR/.." && pwd)"

BUILD_SCRIPT="$SCRIPT_DIR/build_uefi.sh"
TESTCASE_DIR="$ROOT_DIR/testcase"
TARGET_QEMU_DIR="$ROOT_DIR/target/uefi-qemu"
UEFI_PROJECT_DIR_NAME="rfvp"

echo "[uefi-qemu] SCRIPT_DIR=$SCRIPT_DIR"
echo "[uefi-qemu] PLATFORM_DIR=$PLATFORM_DIR"
echo "[uefi-qemu] ROOT_DIR=$ROOT_DIR"
echo "[uefi-qemu] BUILD_SCRIPT=$BUILD_SCRIPT"
echo "[uefi-qemu] TESTCASE_DIR=$TESTCASE_DIR"
echo "[uefi-qemu] TARGET_QEMU_DIR=$TARGET_QEMU_DIR"
echo "[uefi-qemu] UEFI_PROJECT_DIR_NAME=$UEFI_PROJECT_DIR_NAME"

command -v uname >/dev/null 2>&1 || { echo "ERROR: uname not found" >&2; exit 1; }

# mtools writes directly to the FAT32 image without the macOS msdosfs kernel
# driver, which silently truncates large files to one cluster on newer macOS.
# Install with: brew install mtools
for _tool in mformat mcopy mmd; do
  command -v "$_tool" >/dev/null 2>&1 || {
    echo "ERROR: $_tool not found." >&2
    echo "Install mtools:  brew install mtools" >&2
    exit 1
  }
done

[ -f "$BUILD_SCRIPT" ] || { echo "ERROR: Missing build script: $BUILD_SCRIPT" >&2; exit 1; }
[ -x "$BUILD_SCRIPT" ] || chmod +x "$BUILD_SCRIPT"
[ -d "$TESTCASE_DIR" ] || { echo "ERROR: Missing testcase dir: $TESTCASE_DIR" >&2; exit 1; }

HOST_ARCH="$(uname -m)"
echo "[uefi-qemu] HOST_ARCH=$HOST_ARCH"

case "$HOST_ARCH" in
  x86_64|amd64)
    UEFI_ARCH="x86_64"
    QEMU_BIN_NAME="qemu-system-x86_64"
    ESP_DIR="$PLATFORM_DIR/uefi/x86_64/esp"
    BOOT_FILE="$ESP_DIR/EFI/BOOT/BOOTX64.EFI"
    CODE_FD_NAME="edk2-x86_64-code.fd"
    VARS_FD_CANDIDATES=("edk2-i386-vars.fd" "edk2-x86_64-vars.fd")
    QEMU_WORK_DIR="$TARGET_QEMU_DIR/x86_64"
    ;;
  arm64|aarch64)
    UEFI_ARCH="aarch64"
    QEMU_BIN_NAME="qemu-system-aarch64"
    ESP_DIR="$PLATFORM_DIR/uefi/aarch64/esp"
    BOOT_FILE="$ESP_DIR/EFI/BOOT/BOOTAA64.EFI"
    CODE_FD_NAME="edk2-aarch64-code.fd"
    VARS_FD_CANDIDATES=("edk2-arm-vars.fd" "edk2-aarch64-vars.fd")
    QEMU_WORK_DIR="$TARGET_QEMU_DIR/aarch64"
    ;;
  *)
    echo "ERROR: unsupported host architecture: $HOST_ARCH" >&2
    echo "Supported: x86_64, arm64/aarch64" >&2
    exit 1
    ;;
esac

IMAGE_FILE="$QEMU_WORK_DIR/uefi-testcase-fat32.img"
VARS_FILE="$QEMU_WORK_DIR/uefi-vars.fd"

echo "[uefi-qemu] Selected UEFI_ARCH=$UEFI_ARCH"
echo "[uefi-qemu] Expected boot file=$BOOT_FILE"
echo "[uefi-qemu] QEMU_WORK_DIR=$QEMU_WORK_DIR"
echo "[uefi-qemu] IMAGE_FILE=$IMAGE_FILE"
echo "[uefi-qemu] VARS_FILE=$VARS_FILE"

first_existing() {
  local p
  for p in "$@"; do
    if [ -f "$p" ]; then
      printf '%s\n' "$p"
      return 0
    fi
  done
  return 1
}

echo "[uefi-qemu] Locating QEMU binary..."

QEMU_BIN_PATH="$(command -v "$QEMU_BIN_NAME" || true)"

if [ -z "$QEMU_BIN_PATH" ]; then
  QEMU_BIN_PATH="$(first_existing \
    "/opt/homebrew/bin/$QEMU_BIN_NAME" \
    "/usr/local/bin/$QEMU_BIN_NAME" \
    /opt/homebrew/Cellar/qemu/*/bin/"$QEMU_BIN_NAME" \
    /usr/local/Cellar/qemu/*/bin/"$QEMU_BIN_NAME" \
    /opt/homebrew/var/homebrew/tmp/.cellar/qemu/*/bin/"$QEMU_BIN_NAME" \
    /usr/local/var/homebrew/tmp/.cellar/qemu/*/bin/"$QEMU_BIN_NAME" \
    || true)"
fi

if [ -z "$QEMU_BIN_PATH" ]; then
  echo "ERROR: $QEMU_BIN_NAME not found" >&2
  echo "Hint: brew install qemu" >&2
  exit 1
fi

echo "[uefi-qemu] QEMU_BIN_PATH=$QEMU_BIN_PATH"

echo "[uefi-qemu] Locating EDK2 firmware..."

CODE_FD="$(first_existing \
  "/opt/homebrew/share/qemu/$CODE_FD_NAME" \
  "/usr/local/share/qemu/$CODE_FD_NAME" \
  /opt/homebrew/Cellar/qemu/*/share/qemu/"$CODE_FD_NAME" \
  /usr/local/Cellar/qemu/*/share/qemu/"$CODE_FD_NAME" \
  /opt/homebrew/var/homebrew/tmp/.cellar/qemu/*/share/qemu/"$CODE_FD_NAME" \
  /usr/local/var/homebrew/tmp/.cellar/qemu/*/share/qemu/"$CODE_FD_NAME" \
  || true)"

if [ -z "$CODE_FD" ]; then
  echo "ERROR: $CODE_FD_NAME not found" >&2
  echo "Hint: brew install qemu" >&2
  exit 1
fi

QEMU_SHARE="$(dirname "$CODE_FD")"
VARS_FD=""

for name in "${VARS_FD_CANDIDATES[@]}"; do
  if [ -f "$QEMU_SHARE/$name" ]; then
    VARS_FD="$QEMU_SHARE/$name"
    break
  fi
done

if [ -z "$VARS_FD" ]; then
  echo "ERROR: UEFI vars fd not found under $QEMU_SHARE" >&2
  echo "Expected one of: ${VARS_FD_CANDIDATES[*]}" >&2
  echo "Available vars files:" >&2
  ls "$QEMU_SHARE" | grep 'vars.fd' >&2 || true
  exit 1
fi

echo "[uefi-qemu] QEMU_SHARE=$QEMU_SHARE"
echo "[uefi-qemu] CODE_FD=$CODE_FD"
echo "[uefi-qemu] VARS_FD=$VARS_FD"

echo "[uefi-qemu] Running build script..."
"$BUILD_SCRIPT"

if [ ! -f "$BOOT_FILE" ]; then
  echo "ERROR: Missing boot EFI after build: $BOOT_FILE" >&2
  exit 1
fi

TESTCASE_KB="$(du -sk "$TESTCASE_DIR" | awk '{print $1}')"
TESTCASE_MB="$(( (TESTCASE_KB + 1023) / 1024 ))"
AUTO_IMAGE_MB="$(( TESTCASE_MB + 512 ))"
if [ "$AUTO_IMAGE_MB" -lt 2048 ]; then
  AUTO_IMAGE_MB=2048
fi
IMAGE_SIZE_MB="${UEFI_IMAGE_SIZE_MB:-$AUTO_IMAGE_MB}"

echo "[uefi-qemu] TESTCASE_MB=$TESTCASE_MB"
echo "[uefi-qemu] IMAGE_SIZE_MB=$IMAGE_SIZE_MB"

mkdir -p "$QEMU_WORK_DIR"

rm -f "$IMAGE_FILE"

# ── FAT32 image creation via mtools ──────────────────────────────────────────
#
# We use mtools (mformat/mmd/mcopy) instead of mount_msdos + rsync because
# the macOS msdosfs kernel driver silently truncates large files to one FAT32
# cluster (~4 KB) on macOS 15+. mtools writes directly to the raw image,
# bypassing the kernel driver entirely.

echo "[uefi-qemu] Creating raw image (${IMAGE_SIZE_MB} MB)..."
# dd with seek creates a sparse file without writing all zeros to disk.
dd if=/dev/zero of="$IMAGE_FILE" bs=1m count=0 seek="$IMAGE_SIZE_MB" 2>/dev/null

echo "[uefi-qemu] Formatting FAT32 with mformat..."
# -F forces FAT32; -v sets the volume label.
# mformat reads the image size from the file and computes cluster geometry.
mformat -F -v "RFVP_UEFI" -i "$IMAGE_FILE" ::

echo "[uefi-qemu] Creating directory tree..."
mmd -i "$IMAGE_FILE" ::/EFI
mmd -i "$IMAGE_FILE" ::/EFI/BOOT
mmd -i "$IMAGE_FILE" ::/"$UEFI_PROJECT_DIR_NAME"

echo "[uefi-qemu] Copying boot EFI: $(basename "$BOOT_FILE")..."
mcopy -i "$IMAGE_FILE" "$BOOT_FILE" ::/EFI/BOOT/

echo "[uefi-qemu] Copying game data from $TESTCASE_DIR (${TESTCASE_MB} MB, may take a while)..."
# -s  recursive directory copy
# -b  batch mode (no prompts)
# The trailing /. copies directory contents, not the directory itself.
mcopy -s -b -i "$IMAGE_FILE" "$TESTCASE_DIR"/. ::/"$UEFI_PROJECT_DIR_NAME"/

echo "[uefi-qemu] FAT32 image prepared: $IMAGE_FILE"
echo "[uefi-qemu] Image layout:"
echo "[uefi-qemu]   /EFI/BOOT/$(basename "$BOOT_FILE")"
echo "[uefi-qemu]   /$UEFI_PROJECT_DIR_NAME/..."

cp "$VARS_FD" "$VARS_FILE"

echo "[uefi-qemu] Starting QEMU..."

case "$UEFI_ARCH" in
  x86_64)
    "$QEMU_BIN_PATH" \
      -machine q35,accel=hvf \
      -cpu host \
      -m 2048M \
      -device usb-ehci \
      -device usb-kbd \
      -device usb-tablet \
      -device usb-mouse \
      -device intel-hda \
      -device hda-duplex \
      -drive if=pflash,format=raw,readonly=on,file="$CODE_FD" \
      -drive if=pflash,format=raw,file="$VARS_FILE" \
      -drive format=raw,file="$IMAGE_FILE" \
      -serial stdio \
      -display cocoa
    ;;
  aarch64)
    "$QEMU_BIN_PATH" \
      -machine virt,accel=hvf,highmem=off \
      -cpu host \
      -m 2048M \
      -device ramfb \
      -device usb-ehci \
      -device usb-kbd \
      -device usb-tablet \
      -device usb-mouse \
      -device intel-hda \
      -device hda-duplex \
      -drive if=pflash,format=raw,readonly=on,file="$CODE_FD" \
      -drive if=pflash,format=raw,file="$VARS_FILE" \
      -drive format=raw,file="$IMAGE_FILE" \
      -serial stdio \
      -display cocoa
    ;;
esac

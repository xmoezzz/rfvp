#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
SWITCH_DIR="${ROOT_DIR}/platform/switch"
SWITCH_CRATES_DIR="${SWITCH_DIR}/crates"
BUILD_DIR="${SWITCH_DIR}/build"
DIST_DIR="${ROOT_DIR}/dist/switch"

DEVKITPRO="${DEVKITPRO:-/opt/devkitpro}"
DEVKITA64="${DEVKITA64:-${DEVKITPRO}/devkitA64}"

CC="${DEVKITA64}/bin/aarch64-none-elf-gcc"
NACPTOOL="${DEVKITPRO}/tools/bin/nacptool"
ELF2NRO="${DEVKITPRO}/tools/bin/elf2nro"
SWITCH_SPECS="${DEVKITPRO}/libnx/switch.specs"
LIBNX_INCLUDE="${DEVKITPRO}/libnx/include"
LIBNX_LIB="${DEVKITPRO}/libnx/lib"
PORTLIBS="${PORTLIBS:-${DEVKITPRO}/portlibs/switch}"

APP_TITLE="RFVP"
APP_AUTHOR="xmoezzz"
APP_VERSION="0.3.0"
TARGET_NAME="rfvp"
RUST_TARGET="${RFVP_SWITCH_RUST_TARGET:-aarch64-unknown-none}"
RUST_TARGET_DIR="${BUILD_DIR}/rust-target"
RUST_STATICLIB="${RUST_TARGET_DIR}/${RUST_TARGET}/release/librfvp_switch_host.a"

BUILD_CORE="${RFVP_SWITCH_BUILD_CORE:-0}"
LINK_CORE="${RFVP_SWITCH_LINK_CORE:-0}"
CORE_STATICLIB="${RFVP_SWITCH_CORE_STATICLIB:-}"
CORE_RUST_TARGET="${RFVP_SWITCH_CORE_RUST_TARGET:-aarch64-nintendo-switch-freestanding}"
CORE_RUST_TARGET_DIR="${BUILD_DIR}/rust-core-target"
CORE_BUILT_STATICLIB="${CORE_RUST_TARGET_DIR}/${CORE_RUST_TARGET}/release/librfvp_switch_core_staticlib.a"
CORE_TOOLCHAIN="${RFVP_SWITCH_CORE_TOOLCHAIN:-nightly}"
CORE_BUILD_STD="${RFVP_SWITCH_CORE_BUILD_STD:-core,alloc,compiler_builtins}"
CORE_BUILD_STD_FEATURES="${RFVP_SWITCH_CORE_BUILD_STD_FEATURES:-compiler-builtins-mem}"
HOST_FEATURES="ffi"

if [[ "${BUILD_CORE}" == "1" ]]; then
  LINK_CORE="1"
  CORE_STATICLIB="${CORE_BUILT_STATICLIB}"
fi

if [[ "${LINK_CORE}" == "1" ]]; then
  HOST_FEATURES="ffi,rfvp-core-link"
  if [[ -z "${CORE_STATICLIB}" ]]; then
    echo "ERROR: RFVP_SWITCH_LINK_CORE=1 requires RFVP_SWITCH_CORE_STATICLIB=/path/to/librfvp.a or RFVP_SWITCH_BUILD_CORE=1" >&2
    exit 1
  fi
fi

if [[ ! -x "${CC}" ]]; then
  echo "ERROR: aarch64-none-elf-gcc not found: ${CC}" >&2
  echo "Set DEVKITPRO and DEVKITA64 to your devkitPro installation." >&2
  exit 1
fi

if [[ ! -x "${NACPTOOL}" ]]; then
  echo "ERROR: nacptool not found: ${NACPTOOL}" >&2
  exit 1
fi

if [[ ! -x "${ELF2NRO}" ]]; then
  echo "ERROR: elf2nro not found: ${ELF2NRO}" >&2
  exit 1
fi

if [[ ! -f "${SWITCH_SPECS}" ]]; then
  echo "ERROR: libnx switch.specs not found: ${SWITCH_SPECS}" >&2
  exit 1
fi

if [[ ! -d "${LIBNX_INCLUDE}" || ! -d "${LIBNX_LIB}" ]]; then
  echo "ERROR: libnx include/lib directories were not found under ${DEVKITPRO}/libnx" >&2
  exit 1
fi

if [[ ! -d "${PORTLIBS}/include" || ! -d "${PORTLIBS}/lib" ]]; then
  echo "ERROR: Switch portlibs include/lib directories were not found under ${PORTLIBS}" >&2
  echo "Install the devkitPro Switch Mesa/OpenGL portlibs before building the GLES2 GPU backend." >&2
  exit 1
fi

if [[ ! -f "${PORTLIBS}/include/EGL/egl.h" || ! -f "${PORTLIBS}/include/GLES2/gl2.h" ]]; then
  echo "ERROR: EGL/GLES2 headers were not found under ${PORTLIBS}/include" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "ERROR: cargo is required to build the Rust Switch backend crates." >&2
  exit 1
fi

mkdir -p "${BUILD_DIR}" "${DIST_DIR}"

if [[ "${BUILD_CORE}" == "1" ]]; then
  if ! cargo +"${CORE_TOOLCHAIN}" --version >/dev/null 2>&1; then
    echo "ERROR: cargo +${CORE_TOOLCHAIN} is required for RFVP_SWITCH_BUILD_CORE=1" >&2
    echo "Install it with: rustup toolchain install ${CORE_TOOLCHAIN}" >&2
    echo "Also install rust-src for that toolchain: rustup +${CORE_TOOLCHAIN} component add rust-src" >&2
    exit 1
  fi

  printf 'Building RFVP Switch core staticlib: rfvp_switch_core_staticlib (%s, serde pin=1.0.217)\n' "${CORE_RUST_TARGET}"
  RUSTFLAGS="${RUSTFLAGS:-} --cfg rfvp_switch -C panic=abort" \
    cargo +"${CORE_TOOLCHAIN}" build \
      -Z "build-std=${CORE_BUILD_STD}" \
      -Z "build-std-features=${CORE_BUILD_STD_FEATURES}" \
      --manifest-path "${SWITCH_CRATES_DIR}/rfvp_switch_core_staticlib/Cargo.toml" \
      --target "${CORE_RUST_TARGET}" \
      --target-dir "${CORE_RUST_TARGET_DIR}" \
      --release

  if [[ ! -f "${CORE_STATICLIB}" ]]; then
    echo "ERROR: built RFVP Switch core staticlib not found: ${CORE_STATICLIB}" >&2
    exit 1
  fi
elif [[ "${LINK_CORE}" == "1" && ! -f "${CORE_STATICLIB}" ]]; then
  echo "ERROR: RFVP_SWITCH_CORE_STATICLIB not found: ${CORE_STATICLIB}" >&2
  exit 1
fi

printf 'Building Rust Switch backend crate: rfvp_switch_host (%s, features=%s)\n' "${RUST_TARGET}" "${HOST_FEATURES}"
RUSTFLAGS="${RUSTFLAGS:-} --cfg rfvp_switch -C panic=abort" \
  cargo build \
    --manifest-path "${SWITCH_CRATES_DIR}/Cargo.toml" \
    -p rfvp_switch_host \
    --target "${RUST_TARGET}" \
    --target-dir "${RUST_TARGET_DIR}" \
    --release \
    --features "${HOST_FEATURES}"

test -f "${RUST_STATICLIB}"

ARCH_FLAGS=(
  -march=armv8-a
  -mtune=cortex-a57
  -mtp=soft
  -fPIE
)

COMMON_FLAGS=(
  -g
  -O2
  -Wall
  -Wextra
  -ffunction-sections
  -fdata-sections
  -D__SWITCH__
  -I"${LIBNX_INCLUDE}"
  -I"${PORTLIBS}/include"
  -I"${SWITCH_DIR}/include"
)

LDFLAGS=(
  -specs="${SWITCH_SPECS}"
  -g
  -Wl,--gc-sections
  -Wl,-Map,"${BUILD_DIR}/${TARGET_NAME}.map"
  -L"${LIBNX_LIB}"
  -L"${PORTLIBS}/lib"
)

SWITCH_LIBS=(
  -lEGL
  -lGLESv2
  -lglapi
  -ldrm_nouveau
  -lnx
  -lm
)

SRC="${SWITCH_DIR}/source/main.c"
OBJ="${BUILD_DIR}/main.o"
ELF="${BUILD_DIR}/${TARGET_NAME}.elf"
NACP="${BUILD_DIR}/${TARGET_NAME}.nacp"
NRO="${DIST_DIR}/${TARGET_NAME}.nro"
ICON="${SWITCH_DIR}/icon.jpg"

if [[ ! -f "${SRC}" ]]; then
  echo "ERROR: missing Switch source file: ${SRC}" >&2
  exit 1
fi

"${CC}" "${ARCH_FLAGS[@]}" "${COMMON_FLAGS[@]}" -c "${SRC}" -o "${OBJ}"
if [[ "${LINK_CORE}" == "1" ]]; then
  "${CC}" "${ARCH_FLAGS[@]}" "${LDFLAGS[@]}" "${OBJ}" "${RUST_STATICLIB}" "${CORE_STATICLIB}" "${SWITCH_LIBS[@]}" -o "${ELF}"
else
  "${CC}" "${ARCH_FLAGS[@]}" "${LDFLAGS[@]}" "${OBJ}" "${RUST_STATICLIB}" "${SWITCH_LIBS[@]}" -o "${ELF}"
fi
"${NACPTOOL}" --create "${APP_TITLE}" "${APP_AUTHOR}" "${APP_VERSION}" "${NACP}"

if [[ -f "${ICON}" ]]; then
  "${ELF2NRO}" "${ELF}" "${NRO}" --nacp="${NACP}" --icon="${ICON}"
else
  "${ELF2NRO}" "${ELF}" "${NRO}" --nacp="${NACP}"
fi

echo "Switch NRO written to ${NRO}"

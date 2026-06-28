#!/usr/bin/env bash
set -euo pipefail

target="${LINUX_CROSS_TARGET:-x86_64-unknown-linux-gnu}"
sysroot="${LINUX_SYSROOT_DIR:-}"
pkg_config="${LINUX_PKG_CONFIG:-pkg-config}"
linker="${LINUX_CROSS_LINKER:-}"

usage() {
  cat >&2 <<'EOF'
Usage: LINUX_SYSROOT_DIR=/path/to/sysroot scripts/check_linux_cross.sh

Optional environment variables:
  LINUX_CROSS_TARGET   Rust target triple. Default: x86_64-unknown-linux-gnu
  LINUX_CROSS_LINKER   Cross linker command, for example x86_64-linux-gnu-gcc
  LINUX_PKG_CONFIG     pkg-config wrapper or command. Default: pkg-config

The script exports cross pkg-config settings only for this process and does not
change the default macOS build.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ -z "$sysroot" ]]; then
  echo "LINUX_SYSROOT_DIR is required." >&2
  usage
  exit 2
fi

if [[ ! -d "$sysroot" ]]; then
  echo "Linux sysroot does not exist: $sysroot" >&2
  exit 1
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup is required to verify the installed Rust target." >&2
  exit 1
fi

if ! rustup target list --installed | grep -Fxq "$target"; then
  echo "Missing Rust target: $target" >&2
  echo "Install it with: rustup target add $target" >&2
  exit 1
fi

if ! command -v "$pkg_config" >/dev/null 2>&1; then
  echo "Missing pkg-config command: $pkg_config" >&2
  exit 1
fi

export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_SYSROOT_DIR="$sysroot"
export PKG_CONFIG_PATH="${LINUX_PKG_CONFIG_PATH:-$sysroot/usr/lib/pkgconfig:$sysroot/usr/lib/x86_64-linux-gnu/pkgconfig:$sysroot/usr/share/pkgconfig}"
export PKG_CONFIG="$pkg_config"

if [[ -n "$linker" ]]; then
  target_env="$(printf '%s' "$target" | tr '[:lower:]-' '[:upper:]_')"
  export "CARGO_TARGET_${target_env}_LINKER=$linker"
fi

cargo check --target "$target"

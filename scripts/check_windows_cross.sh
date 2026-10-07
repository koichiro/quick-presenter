#!/usr/bin/env bash
set -euo pipefail

cross_toolchain="${WINDOWS_CROSS_TOOLCHAIN:-stable}"
cross_target="${WINDOWS_CROSS_TARGET:-x86_64-pc-windows-msvc}"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'EOF'
Usage: scripts/check_windows_cross.sh

Optional environment variables:
  WINDOWS_CROSS_TOOLCHAIN  rustup toolchain. Default: stable
  WINDOWS_CROSS_TARGET     Rust target triple. Default: x86_64-pc-windows-msvc
  RC_PATH                  LLVM resource compiler path

Checks all Cargo targets without linking or running Windows binaries. Uses the
selected rustup compiler explicitly, even when Homebrew Rust comes first in PATH.
LLVM is found through RC_PATH, PATH, or Homebrew's LLVM installation.
EOF
  exit 0
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup is required." >&2
  exit 1
fi

cross_rustc="$(rustup which --toolchain "$cross_toolchain" rustc)"
cross_cargo="$(rustup which --toolchain "$cross_toolchain" cargo)"
cross_targets="$(rustup target list --installed --toolchain "$cross_toolchain")"
case $'\n'"$cross_targets"$'\n' in
  *$'\n'"$cross_target"$'\n'*) ;;
  *)
    echo "Missing Rust target: $cross_target" >&2
    echo "Install it with: rustup target add --toolchain $cross_toolchain $cross_target" >&2
    exit 1
    ;;
esac

cross_rc="${RC_PATH:-}"
if [[ -z "$cross_rc" ]]; then
  cross_rc="$(command -v llvm-rc || true)"
fi
if [[ -z "$cross_rc" ]] && command -v brew >/dev/null 2>&1; then
  cross_llvm_prefix="$(brew --prefix llvm 2>/dev/null || true)"
  if [[ -n "$cross_llvm_prefix" ]]; then
    cross_rc="$cross_llvm_prefix/bin/llvm-rc"
  fi
fi
if [[ -z "$cross_rc" ]] || ! command -v "$cross_rc" >/dev/null 2>&1; then
  echo "llvm-rc is required. Install LLVM or set RC_PATH to its resource compiler." >&2
  exit 1
fi

RUSTC="$cross_rustc" RC_PATH="$cross_rc" \
  "$cross_cargo" check --locked --all-targets --target "$cross_target"

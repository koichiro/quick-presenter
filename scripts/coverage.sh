#!/usr/bin/env bash
set -euo pipefail

if ! command -v cargo-llvm-cov >/dev/null 2>&1 && ! cargo llvm-cov --version >/dev/null 2>&1; then
  echo "cargo-llvm-cov is required. Install it with: cargo install cargo-llvm-cov" >&2
  exit 1
fi

if [[ -z "${LLVM_COV:-}" ]] && command -v xcrun >/dev/null 2>&1; then
  LLVM_COV="$(xcrun --find llvm-cov 2>/dev/null || true)"
  export LLVM_COV
fi

if [[ -z "${LLVM_PROFDATA:-}" ]] && command -v xcrun >/dev/null 2>&1; then
  LLVM_PROFDATA="$(xcrun --find llvm-profdata 2>/dev/null || true)"
  export LLVM_PROFDATA
fi

cargo llvm-cov \
  --summary-only \
  --fail-under-lines 80 \
  --ignore-filename-regex '(^|/)src/main\.rs$'

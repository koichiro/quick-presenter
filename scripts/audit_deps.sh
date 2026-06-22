#!/usr/bin/env bash
set -euo pipefail

if ! command -v cargo-audit >/dev/null 2>&1 && ! cargo audit --version >/dev/null 2>&1; then
  echo "cargo-audit is required. Install it with: cargo install cargo-audit --locked" >&2
  exit 1
fi

cargo audit

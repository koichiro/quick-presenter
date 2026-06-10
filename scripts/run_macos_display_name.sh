#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "scripts/run_macos_display_name.sh is only supported on macOS." >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
executable="$repo_root/target/debug/Quick Presenter"

cargo build --bin qp

ln -sf qp "$executable"

exec "$executable" "$@"

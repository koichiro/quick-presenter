#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 DEST" >&2
  exit 2
fi

dest="$1"
desktop_dir="$dest/share/applications"
icon_base_dir="$dest/share/icons/hicolor"

mkdir -p "$desktop_dir"
cp "packaging/linux/quick-presenter.desktop" "$desktop_dir/quick-presenter.desktop"

for size in 16 24 32 48 64 128 256 512 1024; do
  source_icon="assets/icons/png/quick-presenter-icon-${size}.png"
  target_dir="$icon_base_dir/${size}x${size}/apps"
  target_icon="$target_dir/quick-presenter.png"

  if [[ ! -s "$source_icon" ]]; then
    echo "Missing icon asset: $source_icon" >&2
    exit 1
  fi

  mkdir -p "$target_dir"
  cp "$source_icon" "$target_icon"
done

#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 DEST [BINARY]" >&2
  exit 2
fi

dest="$1"
binary="${2:-target/release/qp}"
app_name="Quick Presenter.app"
bundle="$dest/$app_name"
contents="$bundle/Contents"
macos_dir="$contents/MacOS"
resources_dir="$contents/Resources"
license_dir="$resources_dir/licenses"
version="$(
  awk -F '"' '/^version = / { print $2; exit }' Cargo.toml
)"

if [[ -z "$version" ]]; then
  echo "Could not read package version from Cargo.toml" >&2
  exit 1
fi

if [[ ! -x "$binary" ]]; then
  echo "Missing executable binary: $binary" >&2
  exit 1
fi

if [[ ! -s "assets/icons/macos/QuickPresenter.icns" ]]; then
  echo "Missing macOS icon: assets/icons/macos/QuickPresenter.icns" >&2
  exit 1
fi

if [[ ! -d "pdfium" ]]; then
  echo "Missing bundled PDFium directory: pdfium" >&2
  exit 1
fi

if [[ ! -s "LICENSE" ]]; then
  echo "Missing Quick Presenter license file: LICENSE" >&2
  exit 1
fi

if [[ ! -s "pdfium/LICENSE" ]]; then
  echo "Missing PDFium license file: pdfium/LICENSE" >&2
  exit 1
fi

rm -rf "$bundle"
mkdir -p "$macos_dir" "$resources_dir" "$license_dir"

cp "$binary" "$macos_dir/qp"
chmod 755 "$macos_dir/qp"

sed "s/@APP_VERSION@/$version/g" \
  packaging/macos/Info.plist.in > "$contents/Info.plist"

cp "assets/icons/macos/QuickPresenter.icns" "$resources_dir/QuickPresenter.icns"
cp -R "pdfium" "$resources_dir/pdfium"
cp "LICENSE" "$license_dir/QuickPresenter-LICENSE.txt"
cp "pdfium/LICENSE" "$license_dir/PDFium-LICENSE.txt"

echo "Staged $bundle"

#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 3 ]]; then
  echo "Usage: $0 DEST [GUI_BINARY] [CLI_BINARY]" >&2
  exit 2
fi

dest="$1"
binary="${2:-target/release/quick-presenter}"
cli_binary="${3:-$(dirname "$binary")/qp}"
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

if [[ ! -x "$cli_binary" ]]; then
  echo "Missing CLI executable: $cli_binary" >&2
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

if [[ ! -s "pdfium/VERSION" ]]; then
  echo "Missing PDFium version file: pdfium/VERSION" >&2
  exit 1
fi

if [[ ! -s "packaging/SOURCE-OFFER.txt" ]]; then
  echo "Missing source offer file: packaging/SOURCE-OFFER.txt" >&2
  exit 1
fi

rm -rf "$bundle"
mkdir -p "$macos_dir" "$resources_dir" "$license_dir"

cp "$binary" "$macos_dir/quick-presenter"
chmod 755 "$macos_dir/quick-presenter"
cp "$cli_binary" "$macos_dir/qp"
chmod 755 "$macos_dir/qp"
mkdir -p "$resources_dir/docs"
cp docs/CLI.md docs/CONTROL_PROTOCOL.md "$resources_dir/docs/"

sed "s/@APP_VERSION@/$version/g" \
  packaging/macos/Info.plist.in > "$contents/Info.plist"

cp "assets/icons/macos/QuickPresenter.icns" "$resources_dir/QuickPresenter.icns"
cp -R "pdfium" "$resources_dir/pdfium"
cp "LICENSE" "$license_dir/QuickPresenter-LICENSE.txt"
cp "packaging/SOURCE-OFFER.txt" "$license_dir/QuickPresenter-SOURCE-OFFER.txt"
cp "pdfium/LICENSE" "$license_dir/PDFium-LICENSE.txt"

proxy="$contents/Helpers/RendererProxy.app"
service="$proxy/Contents/XPCServices/org.quickpresenter.renderer.xpc"
mkdir -p "$proxy/Contents/MacOS" "$service/Contents/MacOS" "$service/Contents/Resources"
cp "$binary" "$proxy/Contents/MacOS/quick-presenter-proxy"
sed "s/@APP_VERSION@/$version/g" packaging/macos/Proxy-Info.plist.in > "$proxy/Contents/Info.plist"
cp "$binary" "$service/Contents/MacOS/quick-presenter-renderer"
cp -R "pdfium" "$service/Contents/Resources/pdfium"
sed "s/@APP_VERSION@/$version/g" packaging/macos/Renderer-Info.plist.in > "$service/Contents/Info.plist"

# XPC requires signed nested code even for a developer/CI staging run. These
# ad-hoc signatures are replaced by the Developer ID signing step for release.
if [[ "$(uname -s)" == "Darwin" ]]; then
  find "$contents" -name '*.dylib' -type f -print0 |
    while IFS= read -r -d '' library; do codesign --force --sign - "$library"; done
  codesign --force --sign - "$macos_dir/qp"
  codesign --force --sign - --entitlements packaging/macos/Renderer.entitlements "$service"
  codesign --force --sign - "$proxy"
  codesign --force --sign - "$bundle"
fi

echo "Staged $bundle"
echo "Layout only: this ad-hoc bundle cannot open PDFs until signed with scripts/sign_macos_app.sh." >&2

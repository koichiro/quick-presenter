#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "Usage: $0 APP_BUNDLE OUTPUT_DMG [VOLUME_NAME]" >&2
  exit 2
fi

app_bundle="$1"
output_dmg="$2"
volume_name="${3:-Quick Presenter}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "create_macos_dmg.sh is only supported on macOS." >&2
  exit 1
fi

for tool in ditto hdiutil; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Missing required macOS tool: $tool" >&2
    exit 1
  fi
done

if [[ ! -d "$app_bundle" ]]; then
  echo "Missing app bundle: $app_bundle" >&2
  exit 1
fi

if [[ "${app_bundle##*.}" != "app" ]]; then
  echo "Expected APP_BUNDLE to end with .app: $app_bundle" >&2
  exit 1
fi

if [[ ! -s "$app_bundle/Contents/Info.plist" ]]; then
  echo "Missing app bundle Info.plist: $app_bundle/Contents/Info.plist" >&2
  exit 1
fi

if [[ ! -x "$app_bundle/Contents/MacOS/quick-presenter" ]]; then
  echo "Missing executable app binary: $app_bundle/Contents/MacOS/quick-presenter" >&2
  exit 1
fi

output_dir="$(dirname "$output_dmg")"
mkdir -p "$output_dir"

staging_dir="$(mktemp -d "${TMPDIR:-/tmp}/quick-presenter-dmg.XXXXXX")"
cleanup() {
  rm -rf "$staging_dir"
}
trap cleanup EXIT

ditto "$app_bundle" "$staging_dir/$(basename "$app_bundle")"
ln -s /Applications "$staging_dir/Applications"

rm -f "$output_dmg"
hdiutil create \
  -volname "$volume_name" \
  -srcfolder "$staging_dir" \
  -ov \
  -format UDZO \
  "$output_dmg"

echo "Created $output_dmg"

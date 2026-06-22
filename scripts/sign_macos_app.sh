#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/sign_macos_app.sh APP_BUNDLE [IDENTITY]

Signs a staged macOS .app bundle for Developer ID distribution.

Arguments:
  APP_BUNDLE  Path to Quick Presenter.app.
  IDENTITY    Optional codesigning identity. If omitted, the script uses
              MACOS_SIGNING_IDENTITY.

Environment:
  MACOS_SIGNING_IDENTITY  Developer ID Application identity to use when the
                          IDENTITY argument is omitted.

Example:
  MACOS_SIGNING_IDENTITY="Developer ID Application: Example (TEAMID)" \
    scripts/sign_macos_app.sh "/tmp/quick-presenter-macos/Quick Presenter.app"
USAGE
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage
  exit 2
fi

app_bundle="$1"
identity="${2:-${MACOS_SIGNING_IDENTITY:-}}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "sign_macos_app.sh is only supported on macOS." >&2
  exit 1
fi

if [[ -z "$identity" ]]; then
  echo "Missing signing identity. Pass IDENTITY or set MACOS_SIGNING_IDENTITY." >&2
  exit 2
fi

if ! command -v codesign >/dev/null 2>&1; then
  echo "Missing required macOS tool: codesign" >&2
  exit 1
fi

if ! command -v file >/dev/null 2>&1; then
  echo "Missing required tool: file" >&2
  exit 1
fi

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

if ! security find-identity -v -p codesigning | grep -F "$identity" >/dev/null; then
  echo "Signing identity is not available to codesign: $identity" >&2
  exit 1
fi

signing_args=(
  --force
  --timestamp
  --options runtime
  --sign "$identity"
)

find "$app_bundle/Contents" -type f -print0 |
  while IFS= read -r -d '' file_path; do
    if [[ "$file_path" == "$app_bundle/Contents/Info.plist" ]]; then
      continue
    fi

    file_type="$(file -b "$file_path")"
    case "$file_type" in
      *Mach-O*)
        echo "Signing $file_path"
        codesign "${signing_args[@]}" "$file_path"
        ;;
    esac
  done

echo "Signing $app_bundle"
codesign "${signing_args[@]}" "$app_bundle"

codesign --verify --deep --strict --verbose=4 "$app_bundle"
echo "Signed $app_bundle"

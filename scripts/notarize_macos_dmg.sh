#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/notarize_macos_dmg.sh APP_BUNDLE OUTPUT_DMG [IDENTITY] [KEYCHAIN_PROFILE]

Creates a DMG from a signed macOS .app bundle, signs the DMG, submits it to
Apple notarization, staples the notarization ticket, and validates the result.

Arguments:
  APP_BUNDLE        Path to a signed Quick Presenter.app.
  OUTPUT_DMG        Path for the created DMG.
  IDENTITY          Optional Developer ID Application identity. If omitted, the
                    script uses MACOS_SIGNING_IDENTITY.
  KEYCHAIN_PROFILE  Optional notarytool keychain profile. If omitted, the
                    script uses NOTARYTOOL_KEYCHAIN_PROFILE.

Environment:
  MACOS_SIGNING_IDENTITY       Developer ID Application identity for signing
                               the DMG when IDENTITY is omitted.
  NOTARYTOOL_KEYCHAIN_PROFILE  notarytool keychain profile created with
                               xcrun notarytool store-credentials.

Example:
  export MACOS_SIGNING_IDENTITY="Developer ID Application: Example (TEAMID)"
  export NOTARYTOOL_KEYCHAIN_PROFILE="quick-presenter-notary"

  scripts/notarize_macos_dmg.sh \
    "/tmp/quick-presenter-macos/Quick Presenter.app" \
    "/tmp/quick-presenter-macos/Quick Presenter.dmg"
USAGE
}

if [[ $# -lt 2 || $# -gt 4 ]]; then
  usage
  exit 2
fi

app_bundle="$1"
output_dmg="$2"
identity="${3:-${MACOS_SIGNING_IDENTITY:-}}"
keychain_profile="${4:-${NOTARYTOOL_KEYCHAIN_PROFILE:-}}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "notarize_macos_dmg.sh is only supported on macOS." >&2
  exit 1
fi

if [[ -z "$identity" ]]; then
  echo "Missing signing identity. Pass IDENTITY or set MACOS_SIGNING_IDENTITY." >&2
  exit 2
fi

if [[ -z "$keychain_profile" ]]; then
  echo "Missing notarytool profile. Pass KEYCHAIN_PROFILE or set NOTARYTOOL_KEYCHAIN_PROFILE." >&2
  exit 2
fi

for tool in codesign hdiutil spctl xcrun; do
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

codesign --verify --deep --strict --verbose=4 "$app_bundle"

scripts/create_macos_dmg.sh "$app_bundle" "$output_dmg"

echo "Signing $output_dmg"
codesign --force --timestamp --sign "$identity" "$output_dmg"
codesign --verify --verbose=4 "$output_dmg"

echo "Submitting $output_dmg for notarization"
xcrun notarytool submit "$output_dmg" \
  --keychain-profile "$keychain_profile" \
  --wait

echo "Stapling notarization ticket to $output_dmg"
xcrun stapler staple "$output_dmg"
xcrun stapler validate "$output_dmg"

spctl --assess --type open --verbose=4 "$output_dmg"

echo "Created signed, notarized, and stapled DMG: $output_dmg"

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
  MACOS_DISTRIBUTION_MODE developer-id (default) or app-store (signing candidate;
                          Store runtime/package gates remain required).
  MACOS_SIGNING_IDENTITY  Developer ID Application identity to use when the
                          IDENTITY argument is omitted.
  MACOS_STORE_UI_ENTITLEMENTS Optional generated Store UI entitlements file.

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

strip_surrounding_quotes() {
  local value="$1"

  if [[ ${#value} -ge 2 ]]; then
    if [[ "${value:0:1}" == '"' && "${value: -1}" == '"' ]]; then
      value="${value:1:${#value}-2}"
    elif [[ "${value:0:1}" == "'" && "${value: -1}" == "'" ]]; then
      value="${value:1:${#value}-2}"
    fi
  fi

  printf '%s' "$value"
}

identity="$(strip_surrounding_quotes "$identity")"

distribution_mode="${MACOS_DISTRIBUTION_MODE:-developer-id}"
case "$distribution_mode" in
  developer-id|app-store) ;;
  *) echo "Unsupported MACOS_DISTRIBUTION_MODE" >&2; exit 2 ;;
esac

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

available_identities="$(security find-identity -v -p codesigning)"
if [[ "$available_identities" != *"$identity"* ]]; then
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
        if [[ "$file_path" == */org.quickpresenter.renderer.xpc/Contents/MacOS/quick-presenter-renderer ]]; then
          continue
        elif [[ "$file_path" == */RendererProxy.app/Contents/MacOS/quick-presenter-proxy ]]; then
          continue
        else
          codesign "${signing_args[@]}" "$file_path"
        fi
        ;;
    esac
done

codesign "${signing_args[@]}" --entitlements packaging/macos/Renderer.entitlements \
  "$app_bundle/Contents/Helpers/RendererProxy.app/Contents/XPCServices/org.quickpresenter.renderer.xpc"
if [[ "$distribution_mode" == "app-store" ]]; then
  codesign "${signing_args[@]}" --entitlements packaging/macos/Proxy-Inherit.entitlements "$app_bundle/Contents/Helpers/RendererProxy.app"
else
  codesign "${signing_args[@]}" "$app_bundle/Contents/Helpers/RendererProxy.app"
fi

echo "Signing $app_bundle"
if [[ "$distribution_mode" == "app-store" ]]; then
  codesign "${signing_args[@]}" --entitlements "${MACOS_STORE_UI_ENTITLEMENTS:-packaging/macos/Store-UI.entitlements}" "$app_bundle"
else
  codesign "${signing_args[@]}" "$app_bundle"
fi

codesign --verify --deep --strict --verbose=4 "$app_bundle"
if [[ "$distribution_mode" == "developer-id" ]]; then
  python3 scripts/check_macos_renderer.py "$app_bundle" tests/fixtures/marp-speaker-notes.pdf
fi
echo "Signed $app_bundle"

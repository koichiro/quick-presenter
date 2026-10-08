#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 5 ]]; then
  echo "Usage: $0 DEST BINARY PROFILE APP_IDENTITY INSTALLER_IDENTITY" >&2
  exit 2
fi

dest="$1"
binary="$2"
profile="$3"
app_identity="$4"
installer_identity="$5"
mkdir -p "$dest"
dest="$(cd "$dest" && pwd)"
scripts/stage_macos_app_bundle.sh "$dest" "$binary"
app="$dest/Quick Presenter.app"
profile_plist="$dest/profile.plist"
security cms -D -i "$profile" > "$profile_plist"

python3 - "$app" "$profile_plist" "$dest/Store-UI.entitlements" <<'PY'
import os
import pathlib
import plistlib
import subprocess
import sys

app, profile_path, entitlements_path = map(pathlib.Path, sys.argv[1:])
profile = plistlib.loads(profile_path.read_bytes())
info_path = app / "Contents/Info.plist"
info = plistlib.loads(info_path.read_bytes())
team = profile["TeamIdentifier"][0]
app_id = profile["Entitlements"]["com.apple.application-identifier"]
if app_id != f'{team}.{info["CFBundleIdentifier"]}':
    raise SystemExit("Provisioning profile does not match the app Bundle ID")
if profile["Entitlements"].get("get-task-allow", False) or profile.get("ProvisionedDevices"):
    raise SystemExit("An App Store distribution profile is required")
entitlements = plistlib.loads(pathlib.Path("packaging/macos/Store-UI.entitlements").read_bytes())
entitlements["com.apple.application-identifier"] = app_id
entitlements["com.apple.developer.team-identifier"] = team
entitlements_path.write_bytes(plistlib.dumps(entitlements))
info["CFBundleVersion"] = os.environ.get("MACOS_STORE_BUILD_NUMBER", info["CFBundleVersion"])
# Store re-signing requires the native same-team XPC identity API.
info["LSMinimumSystemVersion"] = "14.4"
info["NSHumanReadableCopyright"] = "Copyright © 2026 Koichiro Ohba"
info["LSApplicationCategoryType"] = "public.app-category.productivity"
info["DTSDKName"] = "macosx" + subprocess.check_output(["xcrun", "--sdk", "macosx", "--show-sdk-version"], text=True).strip()
info["BuildMachineOSBuild"] = subprocess.check_output(["sw_vers", "-buildVersion"], text=True).strip()
info_path.write_bytes(plistlib.dumps(info))
for nested_info_path in (app / "Contents").glob("**/Contents/Info.plist"):
    nested_info = plistlib.loads(nested_info_path.read_bytes())
    nested_info["LSMinimumSystemVersion"] = "14.4"
    nested_info_path.write_bytes(plistlib.dumps(nested_info))
PY

cp -X "$profile" "$app/Contents/embedded.provisionprofile"
# Download quarantine metadata is not allowed in Store payloads (ITMS-91109).
# Sanitize only this generated bundle, leaving the source files untouched.
xattr -dr com.apple.quarantine "$app"
MACOS_DISTRIBUTION_MODE=app-store MACOS_STORE_UI_ENTITLEMENTS="$dest/Store-UI.entitlements" \
  scripts/sign_macos_app.sh "$app" "$app_identity"
package="$dest/Quick-Presenter-Mac-App-Store.pkg"
productbuild --component "$app" /Applications --sign "$installer_identity" "$package"
pkgutil --check-signature "$package"
echo "Store upload candidate: $package"
echo "Upload validation and Store-installed runtime validation are still required."

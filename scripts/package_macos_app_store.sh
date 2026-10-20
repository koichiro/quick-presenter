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
store_control="${MACOS_STORE_CONTROL:-0}"
if [[ "$store_control" != 0 && "$store_control" != 1 ]]; then
  echo "MACOS_STORE_CONTROL must be 0 or 1" >&2
  exit 2
fi
if [[ "$store_control" == 0 ]] && nm -g "$binary" | awk '$NF == "_qp_control_gui_start" {found=1} END {exit !found}'; then
  echo "This GUI binary requires MACOS_STORE_CONTROL=1" >&2
  exit 2
fi
if [[ "$store_control" == 1 && ! -x "$(dirname "$binary")/qp-control-broker" ]]; then
  echo "Missing Store control broker; build all bins with --features macos-store-control" >&2
  exit 2
fi
if [[ "$store_control" == 1 ]]; then
  for entry in \
    "$binary:_qp_control_gui_start" \
    "$(dirname "$binary")/qp:_qp_control_client_connect" \
    "$(dirname "$binary")/qp-control-broker:_qp_control_broker_main"; do
    executable="${entry%%:*}"
    symbol="${entry##*:}"
    if ! nm -g "$executable" | awk -v symbol="$symbol" '$NF == symbol {found=1} END {exit !found}'; then
      echo "Store control feature missing from $executable" >&2
      exit 2
    fi
  done
fi
mkdir -p "$dest"
dest="$(cd "$dest" && pwd)"
scripts/stage_macos_app_bundle.sh "$dest" "$binary"
app="$dest/Quick Presenter.app"
python3 scripts/check_macos_store_private_apis.py "$app"
profile_plist="$dest/profile.plist"
security cms -D -i "$profile" > "$profile_plist"

python3 - "$app" "$profile_plist" "$dest/Store-UI.entitlements" "$dest/Store-Control.entitlements" "$(dirname "$binary")/qp-control-broker" "$store_control" <<'PY'
import os
import pathlib
import plistlib
import subprocess
import sys

app = pathlib.Path(sys.argv[1])
profile_path = pathlib.Path(sys.argv[2])
entitlements_path = pathlib.Path(sys.argv[3])
control_entitlements_path = pathlib.Path(sys.argv[4])
broker_binary = pathlib.Path(sys.argv[5])
store_control = sys.argv[6] == '1'
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
if store_control:
    group = f"{team}.app.quickpresenter.control"
    if group not in profile["Entitlements"].get("com.apple.security.application-groups", []):
        raise SystemExit("The App Store profile must authorize the Store control App Group")
    entitlements["com.apple.security.application-groups"] = [group]
    contents = app / "Contents"
    cli_bundle = contents / "Helpers/ControlCLI.app"
    broker_bundle = contents / "Helpers/ControlBroker.app"
    roles = [(cli_bundle, "app.quickpresenter.qp", "qp", contents / "MacOS/qp"),
             (broker_bundle, "app.quickpresenter.control.broker", "qp-control-broker", broker_binary)]
    for bundle, identifier, executable, source in roles:
        if not source.is_file():
            raise SystemExit(f"Missing Store control binary: {source}")
        binary_path = bundle / "Contents/MacOS" / executable
        binary_path.parent.mkdir(parents=True, exist_ok=True)
        binary_path.write_bytes(source.read_bytes())
        binary_path.chmod(0o755)
        (bundle / "Contents/Info.plist").write_bytes(plistlib.dumps({
            "CFBundleIdentifier": identifier,
            "CFBundleExecutable": executable,
            "CFBundleName": executable,
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": info["CFBundleShortVersionString"],
            "CFBundleVersion": os.environ.get("MACOS_STORE_BUILD_NUMBER", info["CFBundleVersion"]),
            "LSMinimumSystemVersion": "14.4",
            "LSBackgroundOnly": True,
        }))
    (contents / "MacOS/qp").unlink()
    agents = contents / "Library/LaunchAgents"
    agents.mkdir(parents=True, exist_ok=True)
    (agents / "app.quickpresenter.control.agent.plist").write_bytes(plistlib.dumps({
        "Label": f"{group}.agent",
        "BundleProgram": "Contents/Helpers/ControlBroker.app/Contents/MacOS/qp-control-broker",
        "ProgramArguments": ["Contents/Helpers/ControlBroker.app/Contents/MacOS/qp-control-broker"],
        "MachServices": {f"{group}.register": True, f"{group}.lookup": True},
        "RunAtLoad": False,
    }))
    control_entitlements_path.write_bytes(plistlib.dumps({
        "com.apple.security.app-sandbox": True,
        "com.apple.security.application-groups": [group],
    }))
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
  MACOS_STORE_CONTROL_ENTITLEMENTS="$dest/Store-Control.entitlements" \
  scripts/sign_macos_app.sh "$app" "$app_identity"
package="$dest/Quick-Presenter-Mac-App-Store.pkg"
productbuild --component "$app" /Applications --sign "$installer_identity" "$package"
pkgutil --check-signature "$package"
echo "Store upload candidate: $package"
echo "Upload validation and Store-installed runtime validation are still required."

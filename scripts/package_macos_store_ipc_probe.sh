#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo 'Usage: package_macos_store_ipc_probe.sh NEW_DEST BINARY SIGNING_IDENTITY TEAM_ID' >&2
  exit 2
fi
dest="$1"
binary="$2"
identity="$3"
team="$4"

# Create a fresh development-only package; never modify a prior artifact or target binary.
python3 - "$dest" "$binary" "$team" <<'PY'
import hashlib
import pathlib
import plistlib
import re
import shutil
import sys

dest, binary = map(pathlib.Path, sys.argv[1:3])
team = sys.argv[3]
if not dest.is_absolute() or not binary.is_file() or not re.fullmatch(r'[A-Z0-9]{10}', team):
    raise SystemExit('Require an absolute new destination, built binary, and ten-character Team ID.')
dest.mkdir(parents=True, exist_ok=False)
group = f'{team}.app.quickpresenter.control-probe'
app = dest / 'Probe.app' / 'Contents'
(app / 'MacOS').mkdir(parents=True)
for name in ('Server', 'Client', 'WrongPeer', 'MissingGroup'):
    location = app if name == 'Server' else app / 'Helpers' / f'{name}.app' / 'Contents'
    (location / 'MacOS').mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, location / 'MacOS' / 'probe')
    if name != 'Server':
        identifier = {'Client': 'client', 'WrongPeer': 'wrong', 'MissingGroup': 'missing'}[name]
        info = dict(CFBundleExecutable='probe', CFBundleIdentifier=f'app.quickpresenter.ipc-probe.{identifier}',
                    CFBundleName=name, CFBundlePackageType='APPL', CFBundleVersion='1',
                    CFBundleShortVersionString='0.0.1', LSMinimumSystemVersion='14.4')
        (location / 'Info.plist').write_bytes(plistlib.dumps(info))
    entitlements = {'com.apple.security.app-sandbox': True}
    if name != 'MissingGroup':
        entitlements['com.apple.security.application-groups'] = [group]
    (dest / f'{name}.entitlements').write_bytes(plistlib.dumps(entitlements))
info = dict(CFBundleExecutable='probe', CFBundleIdentifier='app.quickpresenter.ipc-probe.server',
            CFBundleName='Store IPC Probe', CFBundlePackageType='APPL', CFBundleVersion='1',
            CFBundleShortVersionString='0.0.1', LSMinimumSystemVersion='14.4')
(app / 'Info.plist').write_bytes(plistlib.dumps(info))
(dest / 'input-binary.sha256').write_text(hashlib.sha256(binary.read_bytes()).hexdigest() + '\n')
PY

app="$dest/Probe.app"
for entry in 'Client:client' 'WrongPeer:wrong' 'MissingGroup:missing'; do
  name="${entry%%:*}"
  identifier="${entry#*:}"
  # No timestamp/notarization: these are local sandbox experiments, not release packages.
  codesign --force --options runtime --timestamp=none --sign "$identity" \
    --identifier "app.quickpresenter.ipc-probe.$identifier" \
    --entitlements "$dest/$name.entitlements" "$app/Contents/Helpers/$name.app"
done
codesign --force --options runtime --timestamp=none --sign "$identity" \
  --entitlements "$dest/Server.entitlements" "$app"
codesign --verify --deep --strict "$app"
echo "Prepared development IPC probes in $dest"

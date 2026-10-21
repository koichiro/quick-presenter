#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo 'Usage: package_macos_store_ipc_exec_probe.sh NEW_DEST SOURCE SIGNING_IDENTITY TEAM_ID' >&2
  exit 2
fi
dest="$1"
source_file="$2"
identity="$3"
team="$4"

python3 - "$dest" "$source_file" "$team" <<'PY'
import pathlib
import plistlib
import re
import sys

dest = pathlib.Path(sys.argv[1])
source = pathlib.Path(sys.argv[2])
team = sys.argv[3]
if not dest.is_absolute() or dest.exists() or not source.is_file() or not re.fullmatch(r'[A-Z0-9]{10}', team):
    raise SystemExit('Require a fresh absolute destination, source file, and ten-character Team ID.')
dest.mkdir(parents=True)
group = f'{team}.app.quickpresenter.control-probe'
for name in ('SandboxedWrong',):
    contents = dest / f'{name}.app/Contents'
    (contents / 'MacOS').mkdir(parents=True)
    info = dict(CFBundleExecutable='peer-exec-probe',
                CFBundleIdentifier=f'app.quickpresenter.ipc-probe.exec-{name.lower()}',
                CFBundleName=name, CFBundlePackageType='APPL', CFBundleVersion='1',
                CFBundleShortVersionString='0.0.1', LSMinimumSystemVersion='14.4')
    (contents / 'Info.plist').write_bytes(plistlib.dumps(info))
    entitlements = {'com.apple.security.app-sandbox': True,
                    'com.apple.security.application-groups': [group]}
    (dest / f'{name}.entitlements').write_bytes(plistlib.dumps(entitlements))
PY

binary="$dest/peer-exec-probe"
xcrun clang -fobjc-arc -Wall -Wextra -Werror -framework Foundation -framework Security \
  "$source_file" -o "$binary"
for entry in 'SandboxedWrong:sandboxedwrong'; do
  name="${entry%%:*}"
  suffix="${entry#*:}"
  app="$dest/$name.app"
  cp "$binary" "$app/Contents/MacOS/peer-exec-probe"
  codesign --force --options runtime --timestamp=none --sign "$identity" \
    --identifier "app.quickpresenter.ipc-probe.exec-$suffix" \
    --entitlements "$dest/$name.entitlements" "$app"
  codesign --verify --strict "$app"
done
echo "Prepared exec-semantics probes in $dest"

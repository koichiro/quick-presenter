#!/usr/bin/env python3
"""Stage a distinct Developer ID-signed product XPC fixture; never upload it."""
import argparse
import os
import pathlib
import plistlib
import re
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('destination', type=pathlib.Path)
    parser.add_argument('gui', type=pathlib.Path)
    parser.add_argument('cli', type=pathlib.Path)
    parser.add_argument('broker', type=pathlib.Path)
    parser.add_argument('identity')
    parser.add_argument('team')
    args = parser.parse_args()
    if not args.destination.is_absolute() or args.destination.exists():
        parser.error('destination must be a fresh absolute directory')
    if not re.fullmatch(r'[A-Z0-9]{10}', args.team):
        parser.error('team must be a ten-character identifier')
    for binary in (args.gui, args.cli, args.broker):
        if not binary.is_file():
            parser.error(f'missing release binary: {binary}')
    args.destination.mkdir(parents=True)
    subprocess.run(['scripts/stage_macos_app_bundle.sh', str(args.destination),
                    str(args.gui), str(args.cli)], check=True)
    app = args.destination / 'Quick Presenter.app'
    contents = app / 'Contents'
    group = f'{args.team}.app.quickpresenter.control'
    version = plistlib.loads((contents / 'Info.plist').read_bytes())['CFBundleShortVersionString']
    roles = [('ControlCLI', 'app.quickpresenter.qp', 'qp', args.cli),
             ('ControlWrongPeer', 'app.quickpresenter.control.wrong', 'qp', args.cli),
             ('ControlBroker', 'app.quickpresenter.control.broker',
              'qp-control-broker', args.broker)]
    for name, identifier, executable, source in roles:
        bundle = contents / 'Helpers' / f'{name}.app'
        macos = bundle / 'Contents/MacOS'
        macos.mkdir(parents=True)
        shutil.copy2(source, macos / executable)
        (bundle / 'Contents/Info.plist').write_bytes(plistlib.dumps({
            'CFBundleIdentifier': identifier,
            'CFBundleExecutable': executable,
            'CFBundleName': executable,
            'CFBundlePackageType': 'APPL',
            'CFBundleShortVersionString': version,
            'CFBundleVersion': version,
            'LSMinimumSystemVersion': '14.4',
            'LSBackgroundOnly': True,
        }))
    (contents / 'MacOS/qp').unlink()
    agents = contents / 'Library/LaunchAgents'
    agents.mkdir(parents=True)
    (agents / 'app.quickpresenter.control.agent.plist').write_bytes(plistlib.dumps({
        'Label': f'{group}.developer-id-agent',
        'BundleProgram': 'Contents/Helpers/ControlBroker.app/Contents/MacOS/qp-control-broker',
        'ProgramArguments': ['Contents/Helpers/ControlBroker.app/Contents/MacOS/qp-control-broker'],
        'MachServices': {f'{group}.register': True, f'{group}.lookup': True},
        'RunAtLoad': False,
    }))
    root_entitlements = plistlib.loads(
        pathlib.Path('packaging/macos/Store-UI.entitlements').read_bytes())
    root_entitlements['com.apple.security.application-groups'] = [group]
    control_entitlements = {
        'com.apple.security.app-sandbox': True,
        'com.apple.security.application-groups': [group],
    }
    root_path = args.destination / 'Store-UI.entitlements'
    control_path = args.destination / 'Store-Control.entitlements'
    root_path.write_bytes(plistlib.dumps(root_entitlements))
    control_path.write_bytes(plistlib.dumps(control_entitlements))
    env = dict(os.environ, MACOS_DISTRIBUTION_MODE='app-store',
               MACOS_SIGNING_TIMESTAMP='none', MACOS_STORE_UI_ENTITLEMENTS=str(root_path),
               MACOS_STORE_CONTROL_ENTITLEMENTS=str(control_path))
    subprocess.run(['scripts/sign_macos_app.sh', str(app), args.identity], check=True, env=env)
    print(f'Staged signed product XPC fixture: {app}')


if __name__ == '__main__':
    main()

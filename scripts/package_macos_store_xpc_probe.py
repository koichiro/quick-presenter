#!/usr/bin/env python3
"""Build a fresh Developer ID-signed, sandboxed XPC rendezvous experiment."""
import argparse
import pathlib
import plistlib
import re
import shutil
import subprocess


def run(*args):
    subprocess.run(args, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('destination', type=pathlib.Path)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('identity')
    parser.add_argument('team')
    args = parser.parse_args()
    destination = args.destination
    if not destination.is_absolute() or destination.exists() or not args.source.is_file():
        parser.error('destination must be a fresh absolute path; source must exist')
    if not re.fullmatch(r'[A-Z0-9]{10}', args.team):
        parser.error('team must be a ten-character identifier')

    destination.mkdir(parents=True)
    group = f'{args.team}.app.quickpresenter.control-xpc-probe'
    root = destination / 'XPCProbe.app'
    roles = {'GUI': (root, 'gui'),
             'Broker': (root / 'Contents/Helpers/Broker.app', 'broker'),
             'Client': (root / 'Contents/Helpers/Client.app', 'client'),
             'WrongPeer': (root / 'Contents/Helpers/WrongPeer.app', 'wrong')}
    raw = destination / 'xpc-probe-unsigned'
    run('xcrun', 'clang', '-fblocks', '-Wall', '-Wextra', '-Werror',
        '-framework', 'Foundation', '-framework', 'Security',
        '-framework', 'ServiceManagement', str(args.source), '-o', str(raw))

    for name, (app, suffix) in roles.items():
        contents = app / 'Contents'
        (contents / 'MacOS').mkdir(parents=True)
        shutil.copy2(raw, contents / 'MacOS/xpc-probe')
        (contents / 'Info.plist').write_bytes(plistlib.dumps({
            'CFBundleExecutable': 'xpc-probe',
            'CFBundleIdentifier': f'app.quickpresenter.xpc-probe.{suffix}',
            'CFBundleName': f'Store XPC Probe {name}',
            'CFBundlePackageType': 'APPL',
            'CFBundleVersion': '1',
            'CFBundleShortVersionString': '0.0.1',
            'LSMinimumSystemVersion': '14.4',
            'LSUIElement': True,
        }))
        (destination / f'{name}.entitlements').write_bytes(plistlib.dumps({
            'com.apple.security.app-sandbox': True,
            'com.apple.security.application-groups': [group],
        }))

    launch_agents = root / 'Contents/Library/LaunchAgents'
    launch_agents.mkdir(parents=True)
    plist_name = 'app.quickpresenter.xpc-probe.agent.plist'
    (launch_agents / plist_name).write_bytes(plistlib.dumps({
        'Label': f'{group}.agent',
        'BundleProgram': 'Contents/Helpers/Broker.app/Contents/MacOS/xpc-probe',
        'ProgramArguments': ['Contents/Helpers/Broker.app/Contents/MacOS/xpc-probe', 'broker'],
        'MachServices': {f'{group}.register': True, f'{group}.lookup': True},
        'RunAtLoad': False,
    }))

    for name in ('Broker', 'Client', 'WrongPeer', 'GUI'):
        app, suffix = roles[name]
        run('codesign', '--force', '--options', 'runtime', '--timestamp=none',
            '--sign', args.identity, '--identifier', f'app.quickpresenter.xpc-probe.{suffix}',
            '--entitlements', str(destination / f'{name}.entitlements'), str(app))
    run('codesign', '--verify', '--deep', '--strict', str(root))
    signed_info = subprocess.run(
        ('codesign', '-d', '--verbose=4', str(root)),
        capture_output=True, text=True, check=True).stderr
    signed_team = re.search(r'^TeamIdentifier=([A-Z0-9]{10})$', signed_info, re.MULTILINE)
    if not signed_team or signed_team.group(1) != args.team:
        parser.error('team must match the signed application Team ID')
    print(f'Prepared signed XPC diagnostics in {destination}')


if __name__ == '__main__':
    main()

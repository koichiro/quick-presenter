#!/usr/bin/env python3
"""Verify and run the XPC probe from an installed Apple-re-signed TestFlight app."""
import argparse
import hashlib
import json
import pathlib
import plistlib
import re
import subprocess
import sys
import tempfile


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True, timeout=30)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', type=pathlib.Path)
    parser.add_argument('build')
    parser.add_argument('--version', help='Expected application version')
    args = parser.parse_args()
    app = args.app.resolve()
    build = args.build
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    if info['CFBundleVersion'] != build or not re.fullmatch(r'[1-9][0-9]*', build):
        raise AssertionError('wrong installed build')
    if args.version and info['CFBundleShortVersionString'] != args.version:
        raise AssertionError('wrong installed application version')
    if not (app / 'Contents/_MASReceipt/receipt').is_file():
        raise AssertionError('missing TestFlight receipt')
    run('codesign', '--verify', '--deep', '--strict', str(app))
    run('codesign', '--verify', '--strict', '-R',
        '=anchor apple generic and identifier "app.quickpresenter.QuickPresenter"', str(app))
    root_detail = run('codesign', '-dv', '--verbose=4', str(app)).stderr
    if 'Authority=TestFlight Beta Distribution' not in root_detail and \
            'Authority=Apple Mac OS Application Signing' not in root_detail:
        raise AssertionError('product GUI lacks Apple distribution authority')
    root_team = re.search(r'^TeamIdentifier=([A-Z0-9]{10})$', root_detail, re.MULTILINE)
    if not root_team:
        raise AssertionError('product GUI has no expected team')
    root_entitlement_result = run('codesign', '-d', '--entitlements', ':-', str(app))
    root_entitlement_text = root_entitlement_result.stdout + root_entitlement_result.stderr
    root_xml_start = root_entitlement_text.find('<?xml')
    root_xml_end = root_entitlement_text.find('</plist>', root_xml_start)
    if root_xml_start < 0 or root_xml_end < 0:
        raise AssertionError('product GUI has no readable entitlements')
    root_entitlements = plistlib.loads(
        root_entitlement_text[root_xml_start:root_xml_end + len('</plist>')].encode())
    if root_entitlements.get('com.apple.security.app-sandbox') is not True or \
            'com.apple.security.application-groups' in root_entitlements:
        raise AssertionError('product GUI has unexpected sandbox/group entitlements')
    product_binary = app / 'Contents/MacOS' / info['CFBundleExecutable']
    product_hash = hashlib.sha256(product_binary.read_bytes()).hexdigest()
    helper = app / 'Contents/Helpers/StoreXPCProbe.app'
    roles = {'GUI': (helper, 'gui'),
             'Broker': (helper / 'Contents/Helpers/Broker.app', 'broker'),
             'Client': (helper / 'Contents/Helpers/Client.app', 'client'),
             'WrongPeer': (helper / 'Contents/Helpers/WrongPeer.app', 'wrong')}
    team = root_team.group(1)
    hashes = {}
    for name, (bundle, suffix) in roles.items():
        expected = f'app.quickpresenter.xpc-probe.{suffix}'
        run('codesign', '--verify', '--strict', '-R',
            f'=anchor apple generic and identifier "{expected}"', str(bundle))
        detail = run('codesign', '-dv', '--verbose=4', str(bundle)).stderr
        if 'Authority=TestFlight Beta Distribution' not in detail and \
                'Authority=Apple Mac OS Application Signing' not in detail:
            raise AssertionError(f'{name} lacks Apple distribution authority')
        match = re.search(r'^TeamIdentifier=([A-Z0-9]{10})$', detail, re.MULTILINE)
        if not match or (team and match.group(1) != team):
            raise AssertionError(f'{name} has unexpected team')
        team = match.group(1)
        entitlement_result = run('codesign', '-d', '--entitlements', ':-', str(bundle))
        entitlement_output = entitlement_result.stdout + entitlement_result.stderr
        xml_start = entitlement_output.find('<?xml')
        xml_end = entitlement_output.find('</plist>', xml_start)
        if xml_start < 0 or xml_end < 0:
            raise AssertionError(f'{name} has no readable entitlements')
        ent = plistlib.loads(entitlement_output[xml_start:xml_end + len('</plist>')].encode())
        if ent.get('com.apple.security.app-sandbox') is not True or \
                ent.get('com.apple.security.application-groups') != [
                    f'{team}.app.quickpresenter.control-xpc-probe']:
            raise AssertionError(f'{name} has unexpected sandbox/group entitlements')
        executable = bundle / 'Contents/MacOS/xpc-probe'
        hashes[name] = hashlib.sha256(executable.read_bytes()).hexdigest()

    with tempfile.TemporaryDirectory(prefix='qp-xpc-testflight-') as folder:
        (pathlib.Path(folder) / 'XPCProbe.app').symlink_to(helper)
        runner = pathlib.Path(__file__).resolve().with_name('check_macos_store_xpc_probe.py')
        result = subprocess.run([sys.executable, str(runner), folder],
                                capture_output=True, text=True, timeout=60)
        if result.returncode != 0:
            raise AssertionError(f'installed XPC probe failed: {result.stdout} {result.stderr}')
        tested = json.loads(result.stdout.strip())
    print(json.dumps({'probe': 1, 'scope': 'installed Apple-re-signed TestFlight',
                      'version': info['CFBundleShortVersionString'], 'build': build,
                      'team': team, 'product_gui_sha256': product_hash, 'sha256': hashes,
                      'passed': tested['passed'],
                      'remaining': [gate for gate in tested['remaining']
                                    if gate != 'Apple-re-signed TestFlight']}))


if __name__ == '__main__':
    try:
        main()
    except (AssertionError, OSError, subprocess.CalledProcessError,
            subprocess.TimeoutExpired) as error:
        print(json.dumps({'probe': 1, 'failed': str(error),
                          'scope': 'installed Apple-re-signed TestFlight'}))
        raise SystemExit(1)

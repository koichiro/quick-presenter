#!/usr/bin/env python3
"""Verify and run the XPC probe from an installed Apple-re-signed TestFlight app."""
import hashlib
import json
import pathlib
import plistlib
import re
import subprocess
import sys
import tempfile


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True)


def main():
    if len(sys.argv) != 3:
        raise SystemExit('Usage: check_macos_store_xpc_testflight.py INSTALLED_APP BUILD')
    app = pathlib.Path(sys.argv[1]).resolve()
    build = sys.argv[2]
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    if info['CFBundleVersion'] != build or not re.fullmatch(r'[1-9][0-9]*', build):
        raise AssertionError('wrong installed build')
    if not (app / 'Contents/_MASReceipt/receipt').is_file():
        raise AssertionError('missing TestFlight receipt')
    run('codesign', '--verify', '--deep', '--strict', str(app))
    helper = app / 'Contents/Helpers/StoreXPCProbe.app'
    roles = {'GUI': (helper, 'gui'),
             'Broker': (helper / 'Contents/Helpers/Broker.app', 'broker'),
             'Client': (helper / 'Contents/Helpers/Client.app', 'client'),
             'WrongPeer': (helper / 'Contents/Helpers/WrongPeer.app', 'wrong')}
    team = None
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
        result = subprocess.run(['python3', 'scripts/check_macos_store_xpc_probe.py', folder],
                                capture_output=True, text=True, timeout=60)
        if result.returncode != 0:
            raise AssertionError(f'installed XPC probe failed: {result.stdout} {result.stderr}')
        tested = json.loads(result.stdout.strip())
    print(json.dumps({'probe': 1, 'scope': 'installed Apple-re-signed TestFlight',
                      'build': build, 'team': team, 'sha256': hashes,
                      'passed': tested['passed'], 'remaining': tested['remaining']}))


if __name__ == '__main__':
    try:
        main()
    except (AssertionError, OSError, subprocess.CalledProcessError,
            subprocess.TimeoutExpired) as error:
        print(json.dumps({'probe': 1, 'failed': str(error),
                          'scope': 'installed Apple-re-signed TestFlight'}))
        raise SystemExit(1)

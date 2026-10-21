#!/usr/bin/env python3
"""Verify an installed Apple-signed diagnostic payload, then run its IPC checks."""
import argparse
import hashlib
import json
import pathlib
import plistlib
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', type=pathlib.Path)
    parser.add_argument('raw_probe', type=pathlib.Path)
    parser.add_argument('build')
    args = parser.parse_args()
    app = args.app.resolve()
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    if info.get('CFBundleVersion') != args.build:
        parser.error('Installed build does not match the requested diagnostic build')
    if not (app / 'Contents/_MASReceipt/receipt').is_file():
        parser.error('Installed app has no Store/TestFlight receipt')
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)],
                   check=True, capture_output=True)
    server = app / 'Contents/Helpers/StoreIPCProbe.app'
    bundles = [(app, 'app.quickpresenter.QuickPresenter'),
               (server, 'app.quickpresenter.ipc-probe.server')]
    bundles += [(server / f'Contents/Helpers/{name}.app', f'app.quickpresenter.ipc-probe.{identifier}')
                for name, identifier in [('Client', 'client'), ('WrongPeer', 'wrong'),
                                         ('MissingGroup', 'missing')]]
    identities = []
    team = None
    for bundle, identifier in bundles:
        requirement = f'=anchor apple generic and identifier "{identifier}"'
        subprocess.run(['codesign', '--verify', '--strict', '-R', requirement,
                        str(bundle)], check=True, capture_output=True)
        details = subprocess.run(['codesign', '-dvvv', str(bundle)],
                                 check=True, capture_output=True, text=True).stderr
        values = dict(line.split('=', 1) for line in details.splitlines() if '=' in line)
        authorities = [line.split('=', 1)[1] for line in details.splitlines()
                       if line.startswith('Authority=')]
        if not authorities or authorities[0] not in ('TestFlight Beta Distribution',
                                                     'Apple Mac OS Application Signing'):
            parser.error('Payload is not signed by the expected Apple distribution authority')
        current = values.get('TeamIdentifier')
        if not current or current == 'not set' or (team and team != current):
            parser.error('Expected one nonempty developer team across the installed payload')
        team = current
        bundle_info = plistlib.loads((bundle / 'Contents/Info.plist').read_bytes())
        binary = bundle / 'Contents/MacOS' / bundle_info['CFBundleExecutable']
        identities.append(dict(identifier=identifier, authority=authorities[0],
                               binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest()))
    results = []
    # The adapter only adds a temporary symlink; installed signed bytes stay untouched.
    with tempfile.TemporaryDirectory(prefix='qp-installed-ipc-') as temp:
        (pathlib.Path(temp) / 'Probe.app').symlink_to(server, target_is_directory=True)
        scripts = pathlib.Path(__file__).resolve().parent
        for name, extra in [('check_macos_store_ipc_probe.py', [str(args.raw_probe.resolve())]),
                            ('check_macos_store_ipc_load.py', [])]:
            result = subprocess.run(['python3', str(scripts / name), temp, *extra],
                                    capture_output=True, text=True, timeout=90)
            try:
                detail = json.loads(result.stdout)
            except ValueError:
                detail = dict(error='Runner did not return a JSON result')
            detail['scope'] = 'Apple-signed installed diagnostic helpers, not production GUI'
            if 'remaining' in detail:
                detail['remaining'] = [gate for gate in detail['remaining']
                                       if gate != 'Apple re-signing']
            results.append(dict(runner=name, exit=result.returncode, result=detail))
            if result.returncode:
                print(json.dumps(dict(build=args.build, identities=identities, results=results,
                                      scope='Apple-signed installed diagnostic, gate remains open')))
                return 1
    print(json.dumps(dict(build=args.build, version=info['CFBundleShortVersionString'],
                          identities=identities, results=results,
                          scope='Apple-signed installed diagnostic; not GUI or socket-race qualification')))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

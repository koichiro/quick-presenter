#!/usr/bin/env python3
"""Check signed product XPC control against an already-running GUI with a PDF."""
import argparse
import json
import os
import pathlib
import plistlib
import re
import select
import subprocess
import time


def signed_entitlements(bundle):
    result = subprocess.run(['codesign', '-d', '--entitlements', '-', str(bundle)],
                            check=True, capture_output=True)
    output = result.stdout + result.stderr
    start = output.find(b'<?xml')
    end = output.find(b'</plist>', start)
    if start >= 0 and end >= 0:
        return plistlib.loads(output[start:end + len(b'</plist>')])
    text = output.decode(errors='replace')
    values = {}
    for key in ('com.apple.security.app-sandbox',
                'com.apple.security.application-groups'):
        match = re.search(r'^\s*\[Key\] ' + re.escape(key) + r'\s*$', text,
                          re.MULTILINE)
        if not match:
            continue
        block = re.split(r'\n\s*\[Key\] ', text[match.end():], maxsplit=1)[0]
        if key == 'com.apple.security.app-sandbox':
            values[key] = re.search(r'\[Bool\] true\b', block) is not None
        else:
            values[key] = re.findall(r'^\s*\[String\] (.+)$', block, re.MULTILINE)
    return values


def validate_product(app, testflight):
    contents = app / 'Contents'
    roles = [
        (app, 'app.quickpresenter.QuickPresenter', 'quick-presenter',
         '_qp_control_gui_start'),
        (contents / 'Helpers/ControlCLI.app', 'app.quickpresenter.qp', 'qp',
         '_qp_control_client_connect'),
        (contents / 'Helpers/ControlBroker.app', 'app.quickpresenter.control.broker',
         'qp-control-broker', '_qp_control_broker_main'),
    ]
    if not testflight:
        roles.append((contents / 'Helpers/ControlWrongPeer.app',
                      'app.quickpresenter.control.wrong', 'qp',
                      '_qp_control_client_connect'))
    team = None
    for bundle, identifier, executable, symbol in roles:
        info = plistlib.loads((bundle / 'Contents/Info.plist').read_bytes())
        if info.get('CFBundleIdentifier') != identifier or \
                info.get('CFBundleExecutable') != executable:
            raise AssertionError(f'unexpected bundle identity: {bundle}')
        binary = bundle / 'Contents/MacOS' / executable
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise AssertionError(f'missing executable: {binary}')
        subprocess.run(['codesign', '--verify', '--strict', '-R',
                        f'=anchor apple generic and identifier "{identifier}"', str(bundle)],
                       check=True, capture_output=True)
        details = subprocess.run(['codesign', '-dv', '--verbose=4', str(bundle)],
                                 check=True, capture_output=True, text=True).stderr
        match = re.search(r'^TeamIdentifier=([A-Z0-9]{10})$', details, re.MULTILINE)
        if not match or (team is not None and match.group(1) != team):
            raise AssertionError(f'unexpected signing team: {bundle}')
        team = match.group(1)
        entitlements = signed_entitlements(bundle)
        if entitlements.get('com.apple.security.app-sandbox') is not True or \
                entitlements.get('com.apple.security.application-groups') != [
                    f'{team}.app.quickpresenter.control']:
            raise AssertionError(f'unexpected control entitlements: {bundle}')
        symbols = subprocess.run(['nm', '-g', str(binary)], check=True,
                                 capture_output=True, text=True).stdout
        if not any(line.split()[-1:] == [symbol] for line in symbols.splitlines()):
            raise AssertionError(f'Store control feature missing: {binary}')
    agent = contents / 'Library/LaunchAgents/app.quickpresenter.control.agent.plist'
    config = plistlib.loads(agent.read_bytes())
    if config.get('MachServices') != {
            f'{team}.app.quickpresenter.control.register': True,
            f'{team}.app.quickpresenter.control.lookup': True}:
        raise AssertionError('unexpected control broker Mach services')
    broker_program = 'Contents/Helpers/ControlBroker.app/Contents/MacOS/qp-control-broker'
    if config.get('BundleProgram') != broker_program or \
            config.get('ProgramArguments') != [broker_program]:
        raise AssertionError('unexpected control broker launch target')


def command(cli, *args, success=True):
    result = subprocess.run([str(cli), *args, '--json'], capture_output=True,
                            text=True, timeout=12)
    if success and result.returncode != 0:
        raise AssertionError(f'{args}: {result.returncode} {result.stderr.strip()}')
    if not success and (result.returncode == 0 or result.stdout):
        raise AssertionError(f'{args}: unexpectedly accepted: {result.stdout!r}')
    return json.loads(result.stdout if success else result.stderr)


def watch_event(process, predicate, timeout=8):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        ready, _, _ = select.select([process.stdout], [], [], 0.2)
        if ready:
            line = process.stdout.readline()
            if not line:
                raise AssertionError('watch exited early: ' +
                                     process.stderr.read().decode(errors='replace'))
            event = json.loads(line)
            if predicate(event):
                return event
    raise AssertionError('watch event timed out')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', type=pathlib.Path)
    parser.add_argument('--testflight', action='store_true',
                        help='check an Apple-distributed app without the wrong-peer fixture')
    parser.add_argument('--legacy-open-rejection', action='store_true',
                        help='check pre-#432 builds that intentionally reject qp open')
    args = parser.parse_args()
    app = args.app.resolve()
    cli = app / 'Contents/Helpers/ControlCLI.app/Contents/MacOS/qp'
    wrong_bundle = app / 'Contents/Helpers/ControlWrongPeer.app'
    wrong = wrong_bundle / 'Contents/MacOS/qp'
    if not cli.is_file():
        parser.error('missing signed CLI')
    if args.testflight:
        if not (app / 'Contents/_MASReceipt/receipt').is_file():
            parser.error('missing App Store receipt')
        if wrong_bundle.exists():
            parser.error('development wrong-peer fixture is present in TestFlight app')
    elif not wrong.is_file():
        parser.error('missing wrong-identifier fixture')
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
    validate_product(app, args.testflight)
    passed = []
    status = command(cli, 'status')
    if status['kind'] != 'status' or status['pages'] < 2:
        raise AssertionError('GUI must have a multi-page PDF selected')
    original = status['page']
    if original is None or not 1 <= original < status['pages']:
        raise AssertionError('GUI must start before the last page for navigation')
    passed.append('status_query')
    if command(cli, 'timer', 'elapsed')['kind'] != 'timer_elapsed':
        raise AssertionError('elapsed timer response missing')
    for name in ('notes', 'slide', 'context'):
        response = command(cli, name)
        if response['kind'] != name:
            raise AssertionError(f'{name}: wrong response kind')
    for name in ('slide', 'context'):
        response = command(cli, name, '--full')
        if response['kind'] != name:
            raise AssertionError(f'{name} --full: wrong response kind')
    exported = command(cli, 'export')
    if not isinstance(exported, list) or len(exported) != status['pages']:
        raise AssertionError('full-document export did not include every page')
    passed.append('read_only_queries')
    if not args.testflight:
        rejected = command(wrong, 'status', success=False)
        if rejected['code'] != 'IPC_FAILURE':
            raise AssertionError(f'wrong identifier was not rejected as IPC_FAILURE: {rejected}')
        passed.append('wrong_identifier_rejected')
    if args.legacy_open_rejection:
        open_error = command(cli, 'open', '/tmp/store-control-unsupported.pdf', success=False)
        if open_error['code'] != 'UNSUPPORTED_PLATFORM':
            raise AssertionError(f'raw path unexpectedly accepted: {open_error}')
        passed.append('raw_open_rejected')
    watch = subprocess.Popen([str(cli), 'watch', '--json'], stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, bufsize=0)
    try:
        snapshot = watch_event(watch, lambda event: event.get('event') == 'presentation.snapshot')
        if snapshot['state']['page'] != original:
            raise AssertionError('watch snapshot page differs from status')
        command(cli, 'next')
        changed = watch_event(watch, lambda event: event.get('event') == 'page.changed')
        if changed['page'] != original + 1:
            raise AssertionError('watch did not follow navigation')
        passed.append('watch_and_navigation')
    finally:
        watch.terminate()
        watch.wait(timeout=5)
        watch.stdout.close()
        watch.stderr.close()
    command(cli, 'goto', str(original))
    command(cli, 'blackout', 'on')
    if command(cli, 'status')['blackout'] is not True:
        raise AssertionError('blackout on failed')
    command(cli, 'blackout', 'off')
    if command(cli, 'status')['blackout'] is not False:
        raise AssertionError('blackout off failed')
    passed.append('goto_and_blackout')
    command(cli, 'close')
    if command(cli, 'status')['document'] is not None:
        raise AssertionError('close did not clear the document')
    passed.append('close')
    if not args.legacy_open_rejection:
        opened = command(cli, 'open', status['document'])
        if opened['kind'] != 'mutation' or opened['state']['document'] != status['document']:
            raise AssertionError('authorized PDF did not reopen')
        command(cli, 'close')
        passed.append('authorized_open')
    for _ in range(32):
        if command(cli, 'status')['document'] is not None:
            raise AssertionError('repeated connection returned stale document state')
    passed.append('repeated_connections')
    scope = ('Apple-distributed TestFlight product XPC' if args.testflight
             else 'Developer ID-signed product XPC development fixture')
    print(json.dumps({'scope': scope,
                      'passed': passed, 'version': '2.5.0'}))


if __name__ == '__main__':
    main()

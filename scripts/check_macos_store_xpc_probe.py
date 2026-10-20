#!/usr/bin/env python3
"""Validate a signed local XPC rendezvous and clean up its LaunchAgent."""
import json
import pathlib
import select
import subprocess
import sys
import time


def command(path, *args, success):
    result = subprocess.run([str(path), *args], capture_output=True, text=True, timeout=8)
    if (result.returncode == 0) != success:
        raise AssertionError(f'{args}: exit {result.returncode}: {result.stderr.strip()}')
    if success:
        lines = [json.loads(line) for line in result.stdout.splitlines()]
        if len(lines) != 1 or lines[0].get('probe') != 1:
            raise AssertionError(f'{args}: unexpected response: {result.stdout!r}')
        return lines[0]
    if result.stdout or result.returncode != 7:
        raise AssertionError(f'{args}: unexpected rejected output: {result.stdout!r}')
    for line in result.stderr.splitlines():
        json.loads(line)
    return None


def rejected_at(path, stage, *args):
    result = subprocess.run([str(path), *args], capture_output=True, text=True, timeout=8)
    try:
        errors = [json.loads(line) for line in result.stderr.splitlines()]
    except ValueError as error:
        raise AssertionError(f'{args}: malformed rejection: {result.stderr!r}') from error
    if result.returncode != 7 or result.stdout or not any(
            'xpc_error' in error for error in errors) or not any(
            error.get('error') == stage for error in errors) or any(
            error.get('error') == 'direct_rejected' for error in errors):
        raise AssertionError(f'{args}: rejected at the wrong stage: {result.stderr!r}')


def start_gui(path):
    process = subprocess.Popen([str(path), 'gui', '30'], stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, bufsize=1)
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        ready, _, _ = select.select([process.stdout], [], [], 0.1)
        if ready:
            line = process.stdout.readline()
            if line and json.loads(line) == {'probe': 1, 'event': 'gui_registered'}:
                return process
        if process.poll() is not None:
            raise AssertionError(f'GUI failed: {process.stderr.read()}')
    process.terminate()
    process.wait(timeout=3)
    raise AssertionError('GUI registration timed out')


def stop_gui(process):
    if process and process.poll() is None:
        process.terminate()
    if process:
        process.wait(timeout=3)
        process.stdout.close()
        process.stderr.close()


def main():
    if len(sys.argv) != 2:
        raise SystemExit('Usage: check_macos_store_xpc_probe.py PACKAGE_DIR')
    package = pathlib.Path(sys.argv[1]).resolve()
    root = package / 'XPCProbe.app/Contents'
    gui = root / 'MacOS/xpc-probe'
    client = root / 'Helpers/Client.app/Contents/MacOS/xpc-probe'
    wrong = root / 'Helpers/WrongPeer.app/Contents/MacOS/xpc-probe'
    for path in (gui, client, wrong):
        if not path.is_file():
            raise AssertionError(f'missing executable: {path}')
    initial = command(gui, 'status', success=True)
    # A never-registered bundle can report NotFound (3); after unregister it is 0.
    if initial['status'] not in (0, 3):
        raise AssertionError(f'diagnostic agent already registered: {initial}')

    passed = []
    registered = False
    process = None
    try:
        command(client, 'client', success=False)
        passed.append('unregistered_service_rejected')
        assert command(gui, 'register', success=True)['status'] == 1
        registered = True
        passed.append('service_management_registration_enabled')
        process = start_gui(gui)
        passed.append('anonymous_gui_endpoint_registered')
        for _ in range(3):
            assert command(client, 'client', success=True)['event'] == 'roundtrip'
        passed.append('independent_cli_direct_roundtrip')
        rejected_at(wrong, 'lookup_rejected', 'client')
        passed.append('wrong_client_identifier_rejected')
        command(gui, 'gui', '2', success=False)
        passed.append('second_gui_registration_rejected')
        stop_gui(process)
        process = None
        deadline = time.monotonic() + 4
        while True:
            result = subprocess.run([str(client), 'client'], capture_output=True,
                                    text=True, timeout=4)
            if result.returncode == 7:
                break
            if time.monotonic() >= deadline:
                raise AssertionError('stale endpoint remained available')
            time.sleep(0.1)
        passed.append('stale_endpoint_rejected_after_gui_exit')
        rejected_at(wrong, 'registration_rejected', 'gui', '2')
        passed.append('wrong_gui_identifier_rejected')
        process = start_gui(gui)
        assert command(client, 'client', success=True)['event'] == 'roundtrip'
        passed.append('gui_restart_registers_new_endpoint')
    finally:
        stop_gui(process)
        if registered:
            assert command(gui, 'unregister', success=True)['status'] == 0
    passed.append('agent_unregistered_after_test')
    print(json.dumps({'probe': 1, 'passed': passed, 'scope': 'local Developer ID-signed sandbox',
                      'remaining': ['Apple-re-signed TestFlight', 'macOS 14.4 and Intel',
                                    'Protocol v1 and product GUI integration', 'resource and watch limits']}))


if __name__ == '__main__':
    try:
        main()
    except (AssertionError, ValueError, OSError, subprocess.TimeoutExpired) as error:
        print(json.dumps({'probe': 1, 'failed': str(error), 'scope': 'local XPC diagnostic'}))
        raise SystemExit(1)

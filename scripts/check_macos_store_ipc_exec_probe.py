#!/usr/bin/env python3
"""Test whether a queued request is authenticated as a post-exec peer image."""
import json
import os
import pathlib
import select
import signal
import subprocess
import sys
import tempfile
import time


def wait_json(process, event, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        ready, _, _ = select.select([process.stdout], [], [], min(0.1, deadline - time.monotonic()))
        if ready:
            line = process.stdout.readline()
            if not line:
                break
            try:
                value = json.loads(line)
            except json.JSONDecodeError:
                continue
            if value.get('event') == event:
                return value
        if process.poll() is not None:
            break
    raise AssertionError(f'{event} not observed; exit={process.poll()}')


def wait_server(server, output, error):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        output.seek(0)
        line = output.readline()
        if line:
            assert json.loads(line).get('event') == 'listening'
            return
        if server.poll() is not None:
            error.seek(0)
            raise AssertionError(f'server failed: {error.read()}')
        time.sleep(0.05)
    raise AssertionError('server startup timed out')


def wait_exec(pid, expected, timeout=3):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = subprocess.run(['ps', '-p', str(pid), '-o', 'comm='], capture_output=True, text=True)
        if result.stdout.strip() == str(expected):
            return
        time.sleep(0.02)
    raise AssertionError(f'PID {pid} did not exec {expected}')


def stopped(pid):
    result = subprocess.run(['ps', '-p', str(pid), '-o', 'state='], capture_output=True, text=True)
    return result.returncode == 0 and result.stdout.strip().startswith('T')


def run_case(server_binary, fixture, client, mode):
    output = tempfile.TemporaryFile(mode='w+t')
    error = tempfile.TemporaryFile(mode='w+t')
    server = subprocess.Popen([str(server_binary), 'server', '30'], stdout=output, stderr=error)
    attacker = None
    resumed = False
    try:
        wait_server(server, output, error)
        os.kill(server.pid, signal.SIGSTOP)
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline and not stopped(server.pid):
            time.sleep(0.02)
        assert stopped(server.pid), 'server did not enter stopped state'

        command = [str(fixture), mode]
        if mode == 'exec':
            command.append(str(client))
        attacker = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, text=True, start_new_session=True,
                                    bufsize=1)
        queued = wait_json(attacker, 'request_queued', 3)
        assert queued.get('cloexec') is True
        if mode == 'exec':
            wait_exec(attacker.pid, client)

        os.kill(server.pid, signal.SIGCONT)
        resumed = True
        time.sleep(0.4)
        attacker.stdin.write('G')
        attacker.stdin.flush()
        result = wait_json(attacker, 'queued_request_result', 5)
        return result
    finally:
        if not resumed and server.poll() is None:
            try:
                os.kill(server.pid, signal.SIGCONT)
            except PermissionError:
                pass
        if attacker is not None:
            if attacker.stdin and not attacker.stdin.closed:
                attacker.stdin.close()
            if attacker.poll() is None:
                try:
                    os.killpg(attacker.pid, signal.SIGTERM)
                except PermissionError:
                    pass
            try:
                attacker.wait(timeout=3)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(attacker.pid, signal.SIGKILL)
                except PermissionError:
                    pass
        if server.poll() is None:
            try:
                server.terminate()
            except PermissionError:
                pass
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass
        output.close()
        error.close()


def main():
    if len(sys.argv) != 3:
        raise SystemExit('Usage: check_macos_store_ipc_exec_probe.py INSTALLED_APP EXEC_PROBE_DIR')
    installed = pathlib.Path(sys.argv[1]).resolve()
    package = pathlib.Path(sys.argv[2]).resolve()
    helper = installed / 'Contents/Helpers/StoreIPCProbe.app/Contents'
    server = helper / 'MacOS/probe'
    client = helper / 'Helpers/Client.app/Contents/MacOS/probe'
    fixtures = [package / 'SandboxedWrong.app/Contents/MacOS/peer-exec-probe']
    for path in [server, client, *fixtures]:
        if not path.is_file():
            raise AssertionError(f'missing executable: {path}')

    passed = []
    for fixture in fixtures:
        direct = run_case(server, fixture, client, 'direct')
        assert direct == {'probe': 1, 'event': 'queued_request_result',
                          'response_received': False, 'expected_diagnostic': False}, direct
        passed.append(f'{fixture.parents[2].stem}_wrong_identity_rejected')

        executed = run_case(server, fixture, client, 'exec')
        if executed.get('response_received') and executed.get('expected_diagnostic'):
            print(json.dumps({'probe': 1, 'passed': passed + ['queued_wrong_code_request_accepted_after_exec'],
                              'fixture': fixture.parents[2].stem,
                              'adoption_gate': 'failed',
                              'reason': 'LOCAL_PEERTOKEN identified the post-exec image, not queued-byte origin'}))
            return
        passed.append(f'{fixture.parents[2].stem}_queued_request_not_accepted_after_exec')
    print(json.dumps({'probe': 1, 'passed': passed, 'adoption_gate': 'open',
                      'reason': 'exec substitution was not reproduced by the signed sandboxed fixture'}))


if __name__ == '__main__':
    try:
        main()
    except (AssertionError, OSError, subprocess.TimeoutExpired) as error:
        print(json.dumps({'probe': 1, 'adoption_gate': 'open', 'failed': str(error)}))
        raise SystemExit(1)

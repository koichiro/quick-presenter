#!/usr/bin/env python3
"""Exercise a signed diagnostic package; no Store support claim or credentials."""
import concurrent.futures
import json
import pathlib
import subprocess
import sys
import tempfile
import time


PASSED = []

def main():
    if len(sys.argv) != 3:
        raise SystemExit('Usage: check_macos_store_ipc_probe.py PACKAGE_DIR RAW_BINARY')
    package = pathlib.Path(sys.argv[1]).resolve()
    raw = pathlib.Path(sys.argv[2]).resolve()
    binaries = {'Server': package / 'Probe.app/Contents/MacOS/probe'}
    binaries.update({name: package / f'Probe.app/Contents/Helpers/{name}.app/Contents/MacOS/probe'
                     for name in ('Client', 'WrongPeer', 'MissingGroup')})
    results = PASSED

    def command(binary, role, count, success):
        result = subprocess.run([str(binary), role, str(count)], capture_output=True, text=True, timeout=15)
        if (result.returncode == 0) != success:
            raise AssertionError(f'{role}: unexpected exit {result.returncode}: {result.stderr.strip()}')
        if success and role == 'client':
            lines = [json.loads(line) for line in result.stdout.splitlines()]
            assert lines == [dict(probe=1, sequence=i, count=count) for i in range(count)], lines
        elif not success:
            assert not result.stdout, result.stdout
            assert result.returncode == 7, result.returncode
            json.loads(result.stderr.strip())
        return result

    def start(binary):
        # File logs avoid pipe backpressure and make startup waiting explicitly bounded.
        output = tempfile.TemporaryFile(mode='w+t')
        error = tempfile.TemporaryFile(mode='w+t')
        process = subprocess.Popen([str(binary), 'server', '30'], stdout=output, stderr=error)
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            output.seek(0)
            line = output.readline()
            if line:
                assert json.loads(line)['event'] == 'listening'
                return process, output, error
            if process.poll() is not None:
                error.seek(0)
                raise AssertionError(f'Server failed to start: {error.read()}')
            time.sleep(0.05)
        process.terminate()
        process.wait(timeout=5)
        raise AssertionError('Server startup timed out')

    def stop(state):
        process, output, error = state
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=5)
        output.close()
        error.close()

    command(binaries['MissingGroup'], 'client', 1, False)
    command(raw, 'client', 1, False)
    results += ['missing_group_self_rejected', 'raw_ad_hoc_self_rejected']
    server = start(binaries['Server'])
    results.append('signed_container_lookup_and_socket_bind')
    try:
        command(binaries['Client'], 'client', 1, True)
        command(binaries['Client'], 'client', 3, True)
        results += ['mutual_identity_and_roundtrip', 'ordered_three_reply_stream']
        with tempfile.TemporaryDirectory() as temp:
            link = pathlib.Path(temp) / 'qp-probe'
            link.symlink_to(binaries['Client'])
            command(link, 'client', 1, True)
        results.append('symlink_invocation')
        command(binaries['WrongPeer'], 'client', 1, False)
        results.append('wrong_client_identifier_rejected')
        command(binaries['Server'], 'server', 1, False)
        results.append('second_server_rejected')
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
            futures = [executor.submit(command, binaries['Client'], 'client', 1, True) for _ in range(8)]
            for future in futures:
                future.result()
        results.append('concurrent_commands')
    finally:
        stop(server)
    server = start(binaries['Server'])
    try:
        command(binaries['Client'], 'client', 1, True)
        results.append('stale_socket_recovery_after_terminated_server')
    finally:
        stop(server)
    server = start(binaries['WrongPeer'])
    try:
        command(binaries['Client'], 'client', 1, False)
        results.append('wrong_server_identifier_rejected')
    finally:
        stop(server)
    print(json.dumps({'probe': 1, 'passed': results, 'scope': 'local signed sandbox only',
                      'remaining': ['Apple re-signing', 'macOS 14.4', 'PID/exec/FD races',
                                    'watcher/client saturation and resource measurements', 'real GUI/protocol integration']}))


if __name__ == '__main__':
    try:
        main()
    except (AssertionError, subprocess.TimeoutExpired, OSError) as error:
        print(json.dumps({'probe': 1, 'passed': PASSED, 'failed': str(error),
                          'scope': 'local signed sandbox only', 'adoption_gate': 'open'}))
        raise SystemExit(1)

#!/usr/bin/env python3
"""Bounded signed-probe watcher/load checks; never contacts the production GUI."""
import concurrent.futures
import json
import pathlib
import selectors
import subprocess
import sys
import tempfile
import time


def main():
    if len(sys.argv) != 2:
        raise SystemExit('Usage: check_macos_store_ipc_load.py PACKAGE_DIR')
    package = pathlib.Path(sys.argv[1]).resolve() / 'Probe.app/Contents'
    server_binary = package / 'MacOS/probe'
    client_binary = package / 'Helpers/Client.app/Contents/MacOS/probe'
    passed = []
    clients = []
    server = None
    with tempfile.TemporaryFile(mode='w+t') as errors:
        def client(count=1):
            return subprocess.run([str(client_binary), 'client', str(count)],
                                  capture_output=True, text=True, timeout=8)

        def first_line(process, timeout=8):
            with selectors.DefaultSelector() as selector:
                selector.register(process.stdout, selectors.EVENT_READ)
                if not selector.select(timeout):
                    raise AssertionError('Timed out waiting for diagnostic line')
            line = process.stdout.readline()
            if not line:
                raise AssertionError('Probe exited before diagnostic line')
            return json.loads(line)

        def start(seconds):
            process = subprocess.Popen([str(server_binary), 'server', str(seconds)],
                                       stdout=subprocess.PIPE, stderr=errors, text=True)
            try:
                assert first_line(process)['event'] == 'listening'
            except BaseException:
                process.terminate()
                process.wait(timeout=5)
                raise
            return process

        def stop_clients():
            for process in clients:
                if process.poll() is None:
                    process.terminate()
                process.wait(timeout=5)
                process.stdout.close()
            clients.clear()

        try:
            server = start(30)
            for _ in range(4):
                process = subprocess.Popen([str(client_binary), 'client', '30'],
                                           stdout=subprocess.PIPE, stderr=errors, text=True)
                clients.append(process)
                assert first_line(process) == dict(probe=1, sequence=0, count=30)
            result = client(2)
            assert result.returncode == 7 and not result.stdout, result
            result = client()
            assert result.returncode == 0, result.stderr
            passed.append('four_watchers_reject_fifth_and_allow_single_reply')
            stop_clients()
            # Workers may be asleep between replies; wait for observable slot recovery.
            deadline = time.monotonic() + 6
            while True:
                result = client(2)
                if result.returncode == 0:
                    break
                assert time.monotonic() < deadline, result.stderr
                time.sleep(0.1)
            passed.append('watcher_slots_recovered_after_clients_exit')
            with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
                for result in executor.map(lambda _: client(), range(100)):
                    assert result.returncode == 0, result.stderr
                    assert json.loads(result.stdout) == dict(probe=1, sequence=0, count=1)
            passed.append('hundred_roundtrips_with_four_concurrent_callers')
            server.terminate()
            server.wait(timeout=5)
            server.stdout.close()
            server = None
            # Finite server lifetime must end even with a 30-reply subscriber.
            server = start(3)
            process = subprocess.Popen([str(client_binary), 'client', '30'],
                                       stdout=subprocess.PIPE, stderr=errors, text=True)
            clients.append(process)
            assert first_line(process)['sequence'] == 0
            started = time.monotonic()
            assert server.wait(timeout=7) == 0
            shutdown = time.monotonic() - started
            assert process.wait(timeout=4) == 7
            passed.append('finite_server_shutdown_with_active_watcher')
            errors.seek(0)
            # Confirm the fifth request failed at the intended server gate.
            assert 'probe watcher limit' in errors.read()
            print(json.dumps({'passed': passed, 'shutdown_wait_seconds': round(shutdown, 3),
                              'scope': 'local signed diagnostic, not GUI or long-term resource qualification'}))
        finally:
            stop_clients()
            if server:
                if server.poll() is None:
                    server.terminate()
                server.wait(timeout=5)
                server.stdout.close()


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Run rendering and denial checks against the staged/installed executable."""
import argparse
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("pdf", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="quick-presenter-sandbox-") as directory:
        sentinel = Path(directory) / "unrelated-private-file"
        sentinel.write_text("sandbox sentinel", encoding="utf-8")
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            environment = dict(os.environ)
            environment["QUICK_PRESENTER_SANDBOX_DENIAL_PROBE"] = str(sentinel)
            environment["QUICK_PRESENTER_SANDBOX_CONNECT_PROBE"] = "127.0.0.1:" + str(listener.getsockname()[1])
            stopped = threading.Event()
            connected = threading.Event()
            attempts = []
            attacker = None
            if os.name == "nt":
                with socket.socket() as reservation:
                    reservation.bind(("127.0.0.1", 0))
                    inbound_port = reservation.getsockname()[1]
                environment["QUICK_PRESENTER_SANDBOX_INBOUND_PORT"] = str(inbound_port)

                def try_inbound():
                    while not stopped.is_set():
                        attempts.append(1)
                        try:
                            with socket.create_connection(("127.0.0.1", inbound_port), timeout=0.1):
                                connected.set()
                        except OSError:
                            pass
                        stopped.wait(0.02)

                attacker = threading.Thread(target=try_inbound)
                attacker.start()
            try:
                result = subprocess.run(
                    [str(args.binary.resolve()), "--smoke-open-pdf", str(args.pdf.resolve())],
                    cwd=directory, env=environment, capture_output=True, text=True, timeout=45,
                )
            finally:
                stopped.set()
                if attacker is not None:
                    attacker.join()
            if connected.is_set():
                raise SystemExit("External process connected to the AppContainer listener")
            if os.name == "nt" and not attempts:
                raise SystemExit("Inbound network gate made no external connection attempts")
            if result.returncode:
                raise SystemExit("Renderer sandbox gate failed:\n" + result.stderr)
            if sentinel.read_text(encoding="utf-8") != "sandbox sentinel":
                raise SystemExit("Renderer modified the unrelated sentinel")
            print(result.stdout.strip())
            print("Renderer sandbox file/read/write, network/inbound/outbound, and child denial gate passed")


if __name__ == "__main__":
    main()

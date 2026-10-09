#!/usr/bin/env python3
"""Bound CI commands without waiting for inherited descendant output handles."""
import argparse
from contextlib import redirect_stderr
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def run(command, timeout, label, log_dir):
    log_dir = Path(log_dir)
    log_dir.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    print(f"Starting {label} (timeout {timeout}s)", file=sys.stderr, flush=True)
    # A GUI started through an MSIX console alias can inherit output handles.
    # Pipes/read-to-EOF and PowerShell's native pipeline then wait for the GUI,
    # even after qp exits. Files let us wait only for the actual command process.
    with tempfile.NamedTemporaryFile(dir=log_dir, suffix=".stdout", delete=False) as out, \
            tempfile.NamedTemporaryFile(dir=log_dir, suffix=".stderr", delete=False) as err:
        stdout_path, stderr_path = Path(out.name), Path(err.name)
        process = subprocess.Popen(command, stdout=out, stderr=err,
                                   stdin=subprocess.DEVNULL,
                                   start_new_session=os.name != "nt")
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            print(f"Timed out: {label}; terminating process tree {process.pid}",
                  file=sys.stderr, flush=True)
            if os.name == "nt":
                try:
                    subprocess.run(["taskkill.exe", "/PID", str(process.pid), "/T", "/F"],
                                   timeout=10, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL)
                except (OSError, subprocess.TimeoutExpired):
                    pass
            else:
                os.killpg(process.pid, signal.SIGKILL)
            if process.poll() is None:
                process.kill()
            process.wait(timeout=5)
            code = 124
    sys.stdout.write(stdout_path.read_bytes().decode("utf-8", errors="replace"))
    sys.stderr.write(stderr_path.read_bytes().decode("utf-8", errors="replace"))
    print(f"Finished {label}: exit={code}, elapsed={time.monotonic() - started:.1f}s; "
          f"logs={stdout_path}, {stderr_path}", file=sys.stderr, flush=True)
    return code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timeout", type=float, required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--log-dir", required=True)
    parser.add_argument("--diagnostics", type=Path)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command or args.timeout <= 0:
        parser.error("a command and a positive timeout are required")
    # Windows PowerShell 5 turns native stderr into terminating error records
    # under ErrorActionPreference=Stop, even when the command succeeds.
    if args.diagnostics:
        with args.diagnostics.open('w', encoding='utf-8') as diagnostics, redirect_stderr(diagnostics):
            return run(command, args.timeout, args.label, args.log_dir)
    return run(command, args.timeout, args.label, args.log_dir)


if __name__ == "__main__":
    raise SystemExit(main())

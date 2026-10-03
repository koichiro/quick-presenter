#!/usr/bin/env python3
"""Exercise the actual signed XPC service, not an unsandboxed test helper."""
import argparse
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import re
import shutil
import time
import sys

parser = argparse.ArgumentParser()
parser.add_argument("bundle", type=Path)
parser.add_argument("pdf", type=Path)
parser.add_argument("--debug-tests", action="store_true", help="Requires a signed debug build; exercise actual XPC lifetime/fault paths")
parser.add_argument("--expect-untrusted", action="store_true", help="CI layout artifacts must reject ad-hoc signing before PDF parsing")
parser.add_argument("--expect-unbundled", action="store_true", help="Require a raw release executable to reject PDF work")
args = parser.parse_args()
binary = args.bundle.resolve() if args.expect_unbundled else args.bundle.resolve() / "Contents/MacOS/quick-presenter"
with tempfile.TemporaryDirectory(prefix="quick-presenter-xpc-") as directory:
    sentinel = Path(directory) / "ui-private-state"
    sentinel.write_text("sandbox sentinel", encoding="utf-8")
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        environment = dict(os.environ)
        environment.pop("PDFIUM_DYNAMIC_LIB_PATH", None)
        environment.pop("QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE", None)
        environment["QUICK_PRESENTER_SANDBOX_DENIAL_PROBE"] = str(sentinel)
        environment["QUICK_PRESENTER_SANDBOX_CONNECT_PROBE"] = "127.0.0.1:" + str(listener.getsockname()[1])
        result = subprocess.run([str(binary), "--smoke-open-pdf", str(args.pdf.resolve())],
                                cwd=directory, env=environment, capture_output=True, text=True, timeout=45)
        if args.expect_untrusted or args.expect_unbundled:
            expected = "macOS release renderer requires a signed app bundle" if args.expect_unbundled else "XPC renderer startup failed"
            if result.returncode == 0 or expected not in result.stderr:
                raise SystemExit("Untrusted XPC package did not fail closed:\n" + result.stdout + result.stderr)
            print("Untrusted/unbundled package rejected; this is not a signed rendering/security validation")
            sys.exit(0)
        if result.returncode:
            raise SystemExit("XPC renderer gate failed:\n" + result.stderr)
        if sentinel.read_text(encoding="utf-8") != "sandbox sentinel":
            raise SystemExit("Renderer modified UI private state")
        print(result.stdout.strip())
        print("Signed XPC PDF render/notes and private-file/network/child denial gate passed")

    if args.debug_tests:
        base = dict(os.environ)
        base.pop("PDFIUM_DYNAMIC_LIB_PATH", None)
        base.pop("QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE", None)
        base["QUICK_PRESENTER_HELPER_TEST_REPORT_SERVICE"] = "1"
        base["QUICK_PRESENTER_HELPER_TEST_DEADLINE_MS"] = "1000"

        def services_gone(stderr):
            pids = [int(pid) for pid in re.findall(r"renderer-service-pid=(\d+)", stderr)]
            if not pids:
                raise SystemExit("No actual XPC service identity reported")
            deadline = time.monotonic() + 2
            for pid in pids:
                while True:
                    try:
                        os.kill(pid, 0)
                    except ProcessLookupError:
                        break
                    if time.monotonic() >= deadline:
                        raise SystemExit("XPC service survived broker teardown")
                    time.sleep(0.02)
            return pids

        def run(arguments, extra, success, expected=""):
            env = dict(base, **extra)
            result = subprocess.run([str(binary)] + list(map(str, arguments)),
                                    cwd=directory, env=env, capture_output=True, text=True, timeout=20)
            if (result.returncode == 0) != success or expected not in result.stdout + result.stderr:
                raise SystemExit("XPC fault/lifetime gate failed:\n" + result.stdout + result.stderr)
            pids = services_gone(result.stderr)
            return pids

        for fault, operation in [("hang-on-handshake", "Handshake"), ("hang-on-open", "Open"),
                                 ("wait-on-render", "VisibleRender"), ("hang-on-notes", "Notes")]:
            run(["--smoke-open-pdf", args.pdf.resolve()],
                {"QUICK_PRESENTER_HELPER_TEST_FAULT": fault}, False, "operation=" + operation)
        run(["--smoke-open-pdf", args.pdf.resolve()],
            {"QUICK_PRESENTER_HELPER_TEST_FAULT": "hang-on-shutdown"}, True)

        fault_pdf = Path(directory) / "fault.pdf"
        shutil.copyfile(args.pdf, fault_pdf)
        # This fault stalls broker acquisition before any candidate XPC service exists.
        run(["--renderer-scheduler-smoke", fault_pdf, args.pdf.resolve()],
            {"QUICK_PRESENTER_HELPER_TEST_FAULT": "hang-before-input",
             "QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE": "fault.pdf"}, True, "active_preserved=true")
        run(["--renderer-scheduler-smoke", fault_pdf, args.pdf.resolve()],
            {"QUICK_PRESENTER_HELPER_TEST_FAULT": "hang-before-input",
             "QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE": "fault.pdf",
             "QUICK_PRESENTER_HELPER_TEST_SHUTDOWN": "1"}, True, "shutdown_reaped=true")
        for fault in ["hang-on-open", "abort-on-render"]:
            pids = run(["--renderer-scheduler-smoke", fault_pdf, args.pdf.resolve()],
                       {"QUICK_PRESENTER_HELPER_TEST_FAULT": fault,
                        "QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE": "fault.pdf"}, True, "active_preserved=true")
            if len(set(pids)) < 2:
                raise SystemExit("Active/candidate XPC process independence not established: " + str(pids))

        marker = Path(directory) / "once"
        marker.touch()
        run(["--renderer-recovery-smoke", fault_pdf, args.pdf.resolve()],
            {"QUICK_PRESENTER_HELPER_TEST_FAULT": "abort-on-render",
             "QUICK_PRESENTER_HELPER_TEST_FAULT_PAGE": "1",
             "QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE": "fault.pdf",
             "QUICK_PRESENTER_HELPER_TEST_FAULT_ONCE": str(marker)}, True, "recovery_result=true restart_attempts=1")

        env = dict(base, QUICK_PRESENTER_HELPER_TEST_FAULT="wait-on-render")
        broker = subprocess.Popen([str(binary), "--smoke-open-pdf", str(args.pdf.resolve())],
                                  cwd=directory, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        # Killing the UI closes its input pipe even during blocked native work.
        time.sleep(0.5)
        broker.kill()
        _, stderr = broker.communicate(timeout=5)
        services_gone(stderr)
        print("Actual XPC deadline, candidate isolation, restart, and broker-loss gates passed")

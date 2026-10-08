#!/usr/bin/env python3
"""Verify a staged CLI without starting or controlling the presentation GUI."""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib


def verify(binary: Path, expected_version: str) -> dict:
    with tempfile.TemporaryDirectory(prefix="qp-package-check-") as directory:
        root = Path(directory)
        invalid_runtime = root / "not-a-directory"
        invalid_runtime.write_text("CLI metadata must not connect to IPC", encoding="utf-8")
        environment = os.environ.copy()
        environment["XDG_RUNTIME_DIR"] = str(invalid_runtime)
        environment["PDFIUM_DYNAMIC_LIB_PATH"] = str(root / "missing-pdfium")
        def run(args: list[str], expected_exit: int = 0) -> str:
            process = subprocess.run([str(binary), *args], cwd=root, env=environment,
                                     capture_output=True, text=True, timeout=10)
            if process.returncode != expected_exit:
                raise ValueError(f"qp {args} exited {process.returncode}: {process.stderr.strip()}")
            if expected_exit:
                if process.stdout or json.loads(process.stderr)["code"] != "INVALID_REQUEST":
                    raise ValueError("Invalid commands must have empty stdout and typed stderr")
            elif process.stderr:
                raise ValueError(f"Unexpected CLI stderr: {process.stderr.strip()}")
            return process.stdout
        version = json.loads(run(["--version", "--json"]))
        if version != {"application_version": expected_version, "protocol_version": 1}:
            raise ValueError(f"Unexpected CLI version contract: {version}")
        if run(["-V"]) != f"qp {expected_version} (Control Protocol v1)\n":
            raise ValueError("Unexpected human version output")
        help_text = run(["--help"])
        if "timer elapsed" not in help_text or "watch" not in help_text or "--full" not in help_text:
            raise ValueError("CLI help is missing supported commands/options")
        for name in ["start", "stop", "reset"]:
            run(["timer", name, "--json"], 2)
            run([name, "--json"], 2)
        run(["--version", "--full", "--json"], 2)
        return {"result": "passed", "application_version": expected_version,
                "protocol_version": 1, "checks": 10}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--expected-version", default=tomllib.loads(
        (Path(__file__).resolve().parent.parent / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"])
    args = parser.parse_args()
    print(json.dumps(verify(args.binary.resolve(strict=True), args.expected_version)))


if __name__ == "__main__":
    main()

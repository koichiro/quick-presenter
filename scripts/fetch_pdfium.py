#!/usr/bin/env python3
"""Download a prebuilt PDFium binary from bblanchon/pdfium-binaries.

The script intentionally vendors only the native library into ./pdfium/.
It is meant for local development and CI; release packaging can reuse it.
"""
from __future__ import annotations

import argparse
import os
import platform
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
from pathlib import Path

REPO = "https://github.com/bblanchon/pdfium-binaries/releases/latest/download"


def asset_name(system: str, machine: str) -> str:
    sys = system.lower()
    arch = machine.lower()
    if sys == "darwin":
        os_name = "mac"
        cpu = "arm64" if arch in {"arm64", "aarch64"} else "x64"
    elif sys == "windows":
        os_name = "win"
        cpu = "x64" if "64" in arch else "x86"
    elif sys == "linux":
        os_name = "linux"
        cpu = "arm64" if arch in {"aarch64", "arm64"} else "x64"
    else:
        raise SystemExit(f"Unsupported OS: {system}")
    return f"pdfium-{os_name}-{cpu}.tgz"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default="pdfium", help="output directory")
    parser.add_argument("--asset", default=None, help="override asset name, e.g. pdfium-mac-arm64.tgz")
    args = parser.parse_args()

    asset = args.asset or asset_name(platform.system(), platform.machine())
    url = f"{REPO}/{asset}"
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as td:
        archive = Path(td) / asset
        print(f"Downloading {url}")
        urllib.request.urlretrieve(url, archive)
        with tarfile.open(archive, "r:gz") as tf:
            tf.extractall(out)

    print(f"PDFium extracted to {out.resolve()}")
    print("Set PDFIUM_DYNAMIC_LIB_PATH if your dynamic loader cannot find it automatically.")


if __name__ == "__main__":
    main()

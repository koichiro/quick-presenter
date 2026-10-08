#!/usr/bin/env python3
"""Reject private CGS imports in a macOS Store executable or app bundle."""

import pathlib
import re
import subprocess
import sys


MACHO_MAGICS = {
    bytes.fromhex(value)
    for value in (
        "feedface", "cefaedfe", "feedfacf", "cffaedfe", "cafebabe", "bebafeca",
        "cafebabf", "bfbafeca",
    )
}
# Private CGS functions use a capitalized word after CGS. The public
# CGShieldingWindowLevel function starts with "CGSh" and must remain allowed.
PRIVATE_CGS_IMPORT = re.compile(r"\b_CGS[A-Z][A-Za-z0-9_]*\b")


def macho_files(path: pathlib.Path):
    candidates = path.rglob("*") if path.is_dir() else (path,)
    for candidate in candidates:
        if not candidate.is_file():
            continue
        with candidate.open("rb") as stream:
            if stream.read(4) in MACHO_MAGICS:
                yield candidate


def main() -> int:
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} MACHO_OR_APP_BUNDLE", file=sys.stderr)
        return 2

    path = pathlib.Path(sys.argv[1])
    if not path.exists():
        print(f"Missing path: {path}", file=sys.stderr)
        return 2

    binaries = list(macho_files(path))
    if not binaries:
        print(f"No Mach-O files found in {path}", file=sys.stderr)
        return 1

    failed = False
    for binary in binaries:
        result = subprocess.run(["nm", "-u", str(binary)], capture_output=True, text=True)
        if result.returncode:
            print(f"Could not inspect {binary}: {result.stderr.strip()}", file=sys.stderr)
            failed = True
            continue
        private_imports = sorted(set(PRIVATE_CGS_IMPORT.findall(result.stdout)))
        if private_imports:
            print(f"Private CGS imports in {binary}: {', '.join(private_imports)}", file=sys.stderr)
            failed = True

    if failed:
        return 1
    print(f"Checked {len(binaries)} Mach-O files: no private CGS imports")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

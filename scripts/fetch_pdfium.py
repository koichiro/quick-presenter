#!/usr/bin/env python3
"""Download and verify a pinned PDFium binary archive.

The script intentionally vendors only the native library into ./pdfium/.
It is meant for local development, CI, and release packaging.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import posixpath
import shutil
import stat
import tarfile
import tempfile
import urllib.parse
import urllib.request
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Any

GITHUB_RELEASE_API = "https://api.github.com/repos/bblanchon/pdfium-binaries/releases"
MANIFEST_PATH = Path(__file__).with_name("pdfium_manifest.json")
SUPPORTED_ASSETS = {
    "pdfium-linux-arm64.tgz",
    "pdfium-linux-x64.tgz",
    "pdfium-mac-arm64.tgz",
    "pdfium-mac-x64.tgz",
    "pdfium-win-x64.tgz",
    "pdfium-win-x86.tgz",
}


def asset_name(system: str, machine: str) -> str:
    sys_name = system.lower()
    arch = machine.lower()
    if sys_name == "darwin":
        os_name = "mac"
        cpu = "arm64" if arch in {"arm64", "aarch64"} else "x64"
    elif sys_name == "windows":
        os_name = "win"
        cpu = "x64" if "64" in arch else "x86"
    elif sys_name == "linux":
        os_name = "linux"
        cpu = "arm64" if arch in {"aarch64", "arm64"} else "x64"
    else:
        raise SystemExit(f"Unsupported OS: {system}")
    return f"pdfium-{os_name}-{cpu}.tgz"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default="pdfium", help="output directory")
    parser.add_argument("--asset", default=None, help="override asset name, e.g. pdfium-mac-arm64.tgz")
    parser.add_argument("--sha256", default=None, help="expected SHA256 for an overridden asset")
    parser.add_argument(
        "--manifest",
        default=str(MANIFEST_PATH),
        help="pinned PDFium manifest path",
    )
    parser.add_argument(
        "--update-manifest",
        action="store_true",
        help="refresh the manifest from bblanchon/pdfium-binaries and exit",
    )
    parser.add_argument(
        "--version",
        default="latest",
        help="release tag for --update-manifest, or 'latest'",
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="remove and recreate the output directory before extraction",
    )
    args = parser.parse_args()

    manifest_path = Path(args.manifest)
    if args.update_manifest:
        update_manifest(manifest_path, args.version)
        return

    manifest = load_manifest(manifest_path)
    asset = args.asset or asset_name(platform.system(), platform.machine())
    asset_entry = manifest["assets"].get(asset)
    if asset_entry is None and args.sha256 is None:
        supported = ", ".join(sorted(manifest["assets"]))
        raise SystemExit(f"Unknown PDFium asset: {asset}. Supported pinned assets: {supported}")

    expected_sha256 = args.sha256 or asset_entry["sha256"]
    url = f"{manifest['base_url']}/{asset}"
    out = Path(args.out)
    prepare_output_dir(out, clean=args.clean)

    with tempfile.TemporaryDirectory() as td:
        archive = Path(td) / asset
        print(f"Downloading {url}")
        urllib.request.urlretrieve(url, archive)
        verify_sha256(archive, expected_sha256)
        with tarfile.open(archive, "r:gz") as tf:
            safe_extract(tf, out)

    print(f"PDFium {manifest['version']} extracted to {out.resolve()}")
    print("Set PDFIUM_DYNAMIC_LIB_PATH only for local development or explicit troubleshooting.")


def load_manifest(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as manifest_file:
        manifest = json.load(manifest_file)

    if not isinstance(manifest.get("version"), str):
        raise SystemExit(f"Manifest {path} is missing a string version")
    if not isinstance(manifest.get("base_url"), str):
        raise SystemExit(f"Manifest {path} is missing a string base_url")
    if not isinstance(manifest.get("assets"), dict):
        raise SystemExit(f"Manifest {path} is missing an assets object")

    for asset, entry in manifest["assets"].items():
        if not isinstance(entry, dict) or not valid_sha256(entry.get("sha256")):
            raise SystemExit(f"Manifest {path} has an invalid SHA256 for {asset}")

    return manifest


def update_manifest(path: Path, version: str) -> None:
    release = fetch_release(version)
    tag = release["tag_name"]
    assets: dict[str, dict[str, str]] = {}
    for asset in release["assets"]:
        name = asset["name"]
        digest = asset.get("digest", "")
        if name not in SUPPORTED_ASSETS:
            continue
        if not digest.startswith("sha256:") or not valid_sha256(digest.removeprefix("sha256:")):
            raise SystemExit(f"Release asset {name} does not expose a valid SHA256 digest")
        assets[name] = {"sha256": digest.removeprefix("sha256:")}

    missing_assets = SUPPORTED_ASSETS.difference(assets)
    if missing_assets:
        missing = ", ".join(sorted(missing_assets))
        raise SystemExit(f"Release {tag} is missing expected assets: {missing}")

    manifest = {
        "version": tag,
        "base_url": f"https://github.com/bblanchon/pdfium-binaries/releases/download/{tag}",
        "assets": {name: assets[name] for name in sorted(assets)},
    }
    path.write_text(json.dumps(manifest, indent=2, sort_keys=False) + "\n", encoding="utf-8")
    print(f"Updated {path} to {tag}")


def fetch_release(version: str) -> dict[str, Any]:
    if version == "latest":
        url = f"{GITHUB_RELEASE_API}/latest"
    else:
        url = f"{GITHUB_RELEASE_API}/tags/{urllib.parse.quote(version, safe='')}"

    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "User-Agent": "quick-presenter-fetch-pdfium",
        },
    )
    with urllib.request.urlopen(request) as response:
        return json.loads(response.read().decode("utf-8"))


def verify_sha256(path: Path, expected: str) -> None:
    if not valid_sha256(expected):
        raise SystemExit(f"Invalid expected SHA256: {expected}")

    digest = hashlib.sha256()
    with path.open("rb") as archive:
        for chunk in iter(lambda: archive.read(1024 * 1024), b""):
            digest.update(chunk)

    actual = digest.hexdigest()
    if actual != expected.lower():
        raise SystemExit(f"SHA256 mismatch for {path.name}: expected {expected}, got {actual}")


def valid_sha256(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(ch in "0123456789abcdefABCDEF" for ch in value)
    )


def prepare_output_dir(out: Path, clean: bool) -> None:
    resolved = out.resolve(strict=False)
    if resolved.parent == resolved:
        raise SystemExit(f"Refusing to use filesystem root as output directory: {out}")
    if out.is_symlink():
        raise SystemExit(f"Refusing symlink output directory: {out}")
    if out.exists() and not out.is_dir():
        raise SystemExit(f"Output path exists and is not a directory: {out}")

    if clean and out.exists():
        shutil.rmtree(out)

    out.mkdir(parents=True, exist_ok=True)


def safe_extract(tf: tarfile.TarFile, out: Path) -> None:
    root = out.resolve()
    members = tf.getmembers()
    for member in members:
        validate_member(member, root)

    for member in members:
        extract_member(tf, member, root)


def validate_member(member: tarfile.TarInfo, root: Path) -> None:
    target = safe_member_path(member.name, root)
    if member.isdir() or member.isfile():
        return
    if member.issym() or member.islnk():
        safe_link_target(member, target, root)
        return
    raise SystemExit(f"Refusing unsupported tar member type: {member.name}")


def extract_member(tf: tarfile.TarFile, member: tarfile.TarInfo, root: Path) -> None:
    target = safe_member_path(member.name, root)
    if member.isdir():
        target.mkdir(parents=True, exist_ok=True)
        return
    if member.isfile():
        source = tf.extractfile(member)
        if source is None:
            raise SystemExit(f"Could not read tar member: {member.name}")
        target.parent.mkdir(parents=True, exist_ok=True)
        with source, target.open("wb") as output:
            while chunk := source.read(1024 * 1024):
                output.write(chunk)
        os.chmod(target, safe_file_mode(member.mode))
        return
    if member.issym():
        safe_link_target(member, target, root)
        target.parent.mkdir(parents=True, exist_ok=True)
        os.symlink(member.linkname, target)
        return
    if member.islnk():
        link_target = safe_link_target(member, target, root)
        target.parent.mkdir(parents=True, exist_ok=True)
        os.link(link_target, target)
        return
    raise SystemExit(f"Refusing unsupported tar member type: {member.name}")


def safe_member_path(name: str, root: Path) -> Path:
    if is_unsafe_archive_path(name):
        raise SystemExit(f"Refusing unsafe tar path: {name}")
    return contained_path(root / Path(*PurePosixPath(name).parts), root, name)


def safe_link_target(member: tarfile.TarInfo, target: Path, root: Path) -> Path:
    linkname = member.linkname
    if is_unsafe_archive_path(linkname):
        raise SystemExit(f"Refusing unsafe tar link target: {member.name} -> {linkname}")

    if member.issym():
        link_target = target.parent / Path(*PurePosixPath(linkname).parts)
    else:
        link_target = root / Path(*PurePosixPath(linkname).parts)
    return contained_path(link_target, root, f"{member.name} -> {linkname}")


def is_unsafe_archive_path(name: str) -> bool:
    if not name or "\x00" in name:
        return True
    posix = PurePosixPath(name)
    windows = PureWindowsPath(name)
    if posix.is_absolute() or windows.is_absolute() or windows.drive:
        return True
    normalized = posixpath.normpath(name)
    return normalized == ".." or normalized.startswith("../") or ".." in posix.parts


def contained_path(path: Path, root: Path, original_name: str) -> Path:
    resolved = path.resolve(strict=False)
    try:
        resolved.relative_to(root)
    except ValueError as err:
        raise SystemExit(f"Refusing tar path outside output directory: {original_name}") from err
    return resolved


def safe_file_mode(mode: int) -> int:
    executable = mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return 0o755 if executable else 0o644


if __name__ == "__main__":
    main()

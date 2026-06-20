#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import importlib.util
import io
import tarfile
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = REPO_ROOT / "scripts" / "fetch_pdfium.py"
SPEC = importlib.util.spec_from_file_location("fetch_pdfium", SCRIPT_PATH)
fetch_pdfium = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(fetch_pdfium)


class FetchPdfiumTests(unittest.TestCase):
    def test_verify_sha256_accepts_matching_digest(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "archive.tgz"
            path.write_bytes(b"pdfium")
            digest = hashlib.sha256(b"pdfium").hexdigest()

            fetch_pdfium.verify_sha256(path, digest)

    def test_verify_sha256_rejects_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "archive.tgz"
            path.write_bytes(b"pdfium")

            with self.assertRaises(SystemExit):
                fetch_pdfium.verify_sha256(path, "0" * 64)

    def test_safe_extract_writes_regular_files(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            out = Path(td) / "out"
            archive = build_tar(
                [
                    (directory("pdfium"), None),
                    regular_file("pdfium/VERSION", b"MAJOR=151\n"),
                    regular_file("pdfium/lib/libpdfium.so", b"binary"),
                ]
            )

            with tarfile.open(fileobj=archive, mode="r:gz") as tf:
                fetch_pdfium.safe_extract(tf, out)

            self.assertEqual((out / "pdfium" / "VERSION").read_bytes(), b"MAJOR=151\n")
            self.assertEqual((out / "pdfium" / "lib" / "libpdfium.so").read_bytes(), b"binary")

    def test_safe_extract_rejects_absolute_paths(self) -> None:
        self.assert_unsafe(regular_file("/tmp/evil", b"bad"))

    def test_safe_extract_rejects_parent_paths(self) -> None:
        self.assert_unsafe(regular_file("../evil", b"bad"))

    def test_safe_extract_rejects_escaping_symlinks(self) -> None:
        self.assert_unsafe((symlink("pdfium/evil", "../../evil"), None))

    def test_safe_extract_rejects_special_files(self) -> None:
        info = tarfile.TarInfo("pdfium/device")
        info.type = tarfile.CHRTYPE

        self.assert_unsafe((info, None))

    def test_update_manifest_keeps_supported_assets_sorted(self) -> None:
        release = {
            "tag_name": "chromium/9999",
            "assets": [
                {"name": name, "digest": f"sha256:{index:064x}"}
                for index, name in enumerate(sorted(fetch_pdfium.SUPPORTED_ASSETS), start=1)
            ],
        }

        with tempfile.TemporaryDirectory() as td:
            manifest_path = Path(td) / "manifest.json"
            original_fetch_release = fetch_pdfium.fetch_release
            fetch_pdfium.fetch_release = lambda version: release
            try:
                fetch_pdfium.update_manifest(manifest_path, "latest")
            finally:
                fetch_pdfium.fetch_release = original_fetch_release

            text = manifest_path.read_text(encoding="utf-8")

        self.assertIn('"version": "chromium/9999"', text)
        self.assertIn('"pdfium-linux-arm64.tgz"', text)
        self.assertLess(text.index("pdfium-linux-arm64.tgz"), text.index("pdfium-win-x86.tgz"))

    def assert_unsafe(self, member: tuple[tarfile.TarInfo, bytes | None]) -> None:
        with tempfile.TemporaryDirectory() as td:
            archive = build_tar([member])
            with tarfile.open(fileobj=archive, mode="r:gz") as tf:
                with self.assertRaises(SystemExit):
                    fetch_pdfium.safe_extract(tf, Path(td) / "out")


def build_tar(members: list[tuple[tarfile.TarInfo, bytes | None]]) -> io.BytesIO:
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode="w:gz") as tf:
        for member, data in members:
            tf.addfile(member, io.BytesIO(data) if data is not None else None)
    archive.seek(0)
    return archive


def directory(name: str) -> tarfile.TarInfo:
    info = tarfile.TarInfo(name)
    info.type = tarfile.DIRTYPE
    return info


def regular_file(name: str, data: bytes) -> tuple[tarfile.TarInfo, bytes]:
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mode = 0o644
    return info, data


def symlink(name: str, target: str) -> tarfile.TarInfo:
    info = tarfile.TarInfo(name)
    info.type = tarfile.SYMTYPE
    info.linkname = target
    return info


if __name__ == "__main__":
    unittest.main()

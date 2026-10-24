"""Exercise AppKit document delivery and bounded native ownership on macOS."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


@unittest.skipUnless(sys.platform == "darwin", "AppKit requires macOS")
class MacosDocumentsTests(unittest.TestCase):
    def test_native_document_bridge(self):
        source = Path(__file__).with_name("macos_document_bridge.m")
        with tempfile.TemporaryDirectory() as directory:
            binary = str(Path(directory) / "document-bridge")
            subprocess.run(["xcrun", "clang", "-Wall", "-Wextra", "-Werror",
                            "-framework", "AppKit", "-framework", "Foundation",
                            str(source), "-o", binary], check=True, capture_output=True)
            subprocess.run([binary], check=True, capture_output=True, timeout=30)

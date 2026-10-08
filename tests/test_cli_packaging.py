"""Verify CLI payload layout and fail-fast packaging without launching the GUI."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

REPO = Path(__file__).resolve().parents[1]


class CliPackagingTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for name in ("assets", "packaging", "docs", "scripts"):
            shutil.copytree(REPO / name, self.root / name)
        for name in ("Cargo.toml", "LICENSE"):
            shutil.copy(REPO / name, self.root / name)
        (self.root / "pdfium/licenses").mkdir(parents=True)
        (self.root / "pdfium/LICENSE").write_text("fixture license")
        (self.root / "pdfium/VERSION").write_text("fixture version")
        (self.root / "pdfium/licenses/component.txt").write_text("fixture notice")
        (self.root / "bin").mkdir()
        self.gui = self.root / "bin/quick-presenter"
        self.cli = self.root / "bin/custom-qp"
        for binary in (self.gui, self.cli):
            binary.write_text("#!/bin/sh\nprintf 'fixture executable\\n'\n")
            binary.chmod(0o755)
        # Layout tests use fake payloads; real release CI verifies signed binaries.
        (self.root / "bin/codesign").write_text("#!/bin/sh\nexit 0\n")
        (self.root / "bin/codesign").chmod(0o755)
        self.environment = {**os.environ, "PATH": str(self.root / "bin") + os.pathsep + os.environ["PATH"]}

    def run_script(self, script, *args):
        return subprocess.run(["bash", str(REPO / "scripts" / script), *map(str, args)],
                              cwd=self.root, env=self.environment, capture_output=True, text=True)

    def test_macos_explicit_cli_and_bundled_documentation(self):
        result = self.run_script("stage_macos_app_bundle.sh", "out", self.gui, self.cli)
        self.assertEqual(result.returncode, 0, result.stderr)
        contents = self.root / "out/Quick Presenter.app/Contents"
        self.assertEqual((contents / "MacOS/qp").read_bytes(), self.cli.read_bytes())
        self.assertTrue(os.access(contents / "MacOS/qp", os.X_OK))
        self.assertTrue((contents / "Resources/docs/CONTROL_PROTOCOL.md").is_file())
        self.assertTrue((contents / "Resources/docs/CLI.md").is_file())

    def test_missing_sibling_cli_preserves_existing_macos_bundle(self):
        sentinel = self.root / "out/Quick Presenter.app/sentinel"
        sentinel.parent.mkdir(parents=True)
        sentinel.write_text("existing bundle")
        result = self.run_script("stage_macos_app_bundle.sh", "out", self.gui)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing CLI executable", result.stderr)
        self.assertEqual(sentinel.read_text(), "existing bundle")

    @unittest.skipUnless(shutil.which("dpkg-deb"), "dpkg-deb is required")
    def test_debian_cli_link_permissions_and_docs(self):
        result = self.run_script("build_linux_deb.sh", "--binary", self.gui,
                                 "--cli-binary", self.cli, "--artifact-dir", "out")
        self.assertEqual(result.returncode, 0, result.stderr)
        package = next((self.root / "out").glob("*.deb"))
        extracted = self.root / "extracted"
        subprocess.run(["dpkg-deb", "-x", str(package), str(extracted)], check=True, capture_output=True)
        cli = extracted / "usr/bin/qp"
        self.assertEqual(os.readlink(cli), "../lib/quick-presenter/qp")
        self.assertEqual(cli.read_bytes(), self.cli.read_bytes())
        self.assertEqual(cli.stat().st_mode & 0o777, 0o755)
        for name in ("CLI.md", "CONTROL_PROTOCOL.md"):
            self.assertTrue((extracted / "usr/share/doc/quick-presenter" / name).is_file())

    @unittest.skipUnless(shutil.which("dpkg-deb"), "dpkg-deb is required")
    def test_missing_cli_preserves_existing_debian_staging(self):
        sentinel = self.root / "out/linux-deb-root/sentinel"
        sentinel.parent.mkdir(parents=True)
        sentinel.write_text("existing package")
        result = self.run_script("build_linux_deb.sh", "--binary", self.gui,
                                 "--artifact-dir", "out")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing CLI executable", result.stderr)
        self.assertEqual(sentinel.read_text(), "existing package")

    def test_msix_alias_targets_console_cli_without_replacing_gui(self):
        manifest = ET.parse(REPO / "packaging/windows/AppxManifest.xml.in")
        ns = {"f": "http://schemas.microsoft.com/appx/manifest/foundation/windows10",
              "u": "http://schemas.microsoft.com/appx/manifest/uap/windows10/5",
              "d": "http://schemas.microsoft.com/appx/manifest/desktop/windows10/4"}
        app = manifest.find("f:Applications/f:Application", ns)
        self.assertEqual(app.attrib["Executable"], "quick-presenter.exe")
        # MakeAppx requires multi-instance activation for a console alias.
        self.assertEqual(app.attrib["{" + ns["d"] + "}SupportsMultipleInstances"], "true")
        extension = app.find("f:Extensions/u:Extension", ns)
        self.assertEqual(extension.attrib["Executable"], "qp.exe")
        self.assertEqual(extension.attrib["Category"], "windows.appExecutionAlias")
        self.assertEqual(extension.attrib["EntryPoint"], "Windows.FullTrustApplication")
        alias = extension.find("u:AppExecutionAlias", ns)
        self.assertEqual(alias.attrib["{" + ns["d"] + "}Subsystem"], "console")
        self.assertEqual(alias.find("u:ExecutionAlias", ns).attrib["Alias"], "qp.exe")


if __name__ == "__main__":
    unittest.main()

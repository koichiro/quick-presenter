"""Exercise artifact inventory, provenance, and release failure conditions."""
import copy
import base64
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("release_sbom", Path(__file__).resolve().parents[1] / "scripts/release_sbom.py")
sbom = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sbom)
TARGET = "x86_64-unknown-linux-gnu"
REVISION = "a" * 40


class ReleaseSbomTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.repo = self.root / "repo"
        self.payload = self.root / "payload"
        self.repo.mkdir()
        (self.repo / "scripts").mkdir()
        (self.repo / "vendor/patched").mkdir(parents=True)
        (self.repo / "Cargo.toml").write_text('[package]\nname = "quick-presenter"\nversion = "1.4.0"\n')
        (self.repo / "Cargo.lock").write_text("locked dependency inventory")
        (self.repo / "scripts/pdfium_manifest.json").write_text(json.dumps({
            "version": "chromium/8076", "base_url": "https://example.org/chromium/8076",
            "assets": {"pdfium-linux-x64.tgz": {"sha256": "b" * 64}}}))
        self.pdfium = self.payload / "usr/lib/quick-presenter/pdfium"
        (self.pdfium / "licenses").mkdir(parents=True)
        (self.pdfium / "lib").mkdir()
        (self.pdfium / "VERSION").write_text("MAJOR=156\nMINOR=0\nBUILD=8076\nPATCH=0\n")
        (self.pdfium / "LICENSE").write_text("PDFium license text")
        (self.pdfium / "licenses/freetype.txt").write_text("FreeType license evidence")
        (self.pdfium / "lib/libpdfium.so").write_bytes(b"native library")
        self.notices = self.payload / "usr/lib/quick-presenter/licenses"
        self.notices.mkdir()
        (self.notices.parent / "quick-presenter").write_bytes(b"application executable")
        for name in ("QuickPresenter-LICENSE.txt", "QuickPresenter-SOURCE-OFFER.txt", "PDFium-LICENSE.txt"):
            (self.notices / name).write_text("notice")
        self.artifact = self.root / "quick-presenter_1.4.0_amd64.deb"
        self.artifact.write_bytes(b"final package bytes")
        self.cargo = {
            "bomFormat": "CycloneDX", "specVersion": "1.5", "version": 1,
            "metadata": {"component": {"type": "application", "bom-ref": "local-root", "name": "quick-presenter", "version": "1.4.0"},
                         "properties": [{"name": "cdx:rustc:sbom:target:triple", "value": TARGET}]},
            "components": [{"type": "library", "bom-ref": "path+file:///build/vendor/patched", "name": "patched", "version": "1.0", "purl": "pkg:cargo/patched@1.0"}],
            "dependencies": [{"ref": "local-root", "dependsOn": ["path+file:///build/vendor/patched"]},
                             {"ref": "path+file:///build/vendor/patched", "dependsOn": []}],
        }

    def generate(self):
        return sbom.make_bom(self.cargo, self.artifact, self.payload, TARGET, REVISION, self.repo)

    def test_artifact_and_extracted_files_have_separate_hashes(self):
        bom = self.generate()
        self.assertEqual(bom["metadata"]["component"]["hashes"], sbom.hashes(self.artifact))
        file = next(c for c in bom["components"] if c["name"].endswith("lib/libpdfium.so"))
        self.assertEqual(file["hashes"], sbom.hashes(self.pdfium / "lib/libpdfium.so"))
        native = next(c for c in bom["components"] if c["name"] == "PDFium")
        self.assertEqual(native["version"], "156.0.8076.0")
        self.assertEqual(native["externalReferences"][0]["hashes"][0]["content"], "b" * 64)

    def test_local_patch_provenance_and_dependency_refs(self):
        bom = self.generate()
        patch = next(c for c in bom["components"] if c["name"] == "patched")
        self.assertNotIn("purl", patch)
        self.assertIn(REVISION, patch["bom-ref"])
        self.assertNotIn("/build/", json.dumps(bom))
        sbom.validate_refs(bom)

    def test_license_evidence_does_not_invent_native_versions(self):
        bom = self.generate()
        evidence = next(c for c in bom["components"] if c["name"] == "PDFium license evidence: freetype.txt")
        self.assertEqual(base64.b64decode(evidence["licenses"][0]["license"]["text"]["content"]), b"FreeType license evidence")
        self.assertNotIn("version", evidence)
        self.assertEqual(bom["compositions"][0]["aggregate"], "incomplete")

    def test_non_utf8_license_bytes_are_preserved(self):
        data = b"Copyright \xa9 upstream"
        (self.pdfium / "licenses/freetype.txt").write_bytes(data)
        evidence = next(c for c in self.generate()["components"] if c["name"] == "PDFium license evidence: freetype.txt")
        self.assertEqual(base64.b64decode(evidence["licenses"][0]["license"]["text"]["content"]), data)

    def test_generation_is_repeatable_without_mutating_cargo_input(self):
        original = copy.deepcopy(self.cargo)
        self.assertEqual(self.generate(), self.generate())
        self.assertEqual(self.cargo, original)

    def test_package_or_payload_change_invalidates_expected_bom(self):
        original = self.generate()
        self.artifact.write_bytes(b"resigned package")
        self.assertNotEqual(original, self.generate())
        original = self.generate()
        (self.pdfium / "lib/libpdfium.so").write_bytes(b"changed binary")
        self.assertNotEqual(original, self.generate())

    def test_target_mismatch_fails(self):
        self.cargo["metadata"]["properties"][0]["value"] = "aarch64-apple-darwin"
        with self.assertRaisesRegex(ValueError, "target"):
            self.generate()

    def test_application_version_mismatch_fails(self):
        self.cargo["metadata"]["component"]["version"] = "1.3.0"
        with self.assertRaisesRegex(ValueError, "application version"):
            self.generate()

    def test_pdfium_pin_mismatch_fails(self):
        (self.pdfium / "VERSION").write_text("MAJOR=155\nMINOR=0\nBUILD=8000\nPATCH=0\n")
        with self.assertRaisesRegex(ValueError, "pin"):
            self.generate()

    def test_missing_or_empty_notice_fails(self):
        path = self.notices / "QuickPresenter-SOURCE-OFFER.txt"
        path.write_text("")
        with self.assertRaisesRegex(ValueError, "notice"):
            self.generate()
        path.unlink()
        with self.assertRaisesRegex(ValueError, "notice"):
            self.generate()

    def test_missing_application_fails(self):
        (self.notices.parent / "quick-presenter").unlink()
        with self.assertRaisesRegex(ValueError, "application executable"):
            self.generate()

    def test_missing_library_or_license_fails(self):
        (self.pdfium / "lib/libpdfium.so").unlink()
        with self.assertRaisesRegex(ValueError, "native library"):
            self.generate()

    def test_empty_component_license_fails(self):
        (self.pdfium / "licenses/freetype.txt").write_text("")
        with self.assertRaisesRegex(ValueError, "Empty PDFium license"):
            self.generate()

    def test_dangling_and_duplicate_references_fail(self):
        bom = self.generate()
        bom["dependencies"][0]["dependsOn"].append("missing")
        with self.assertRaisesRegex(ValueError, "missing component"):
            sbom.validate_refs(bom)
        bom = self.generate()
        bom["components"].append(bom["components"][0])
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            sbom.validate_refs(bom)

    @unittest.skipIf(os.name == "nt", "Creating symlinks may need Windows developer privileges")
    def test_symlink_target_is_recorded_without_reading_host_files(self):
        (self.payload / "launcher").symlink_to("/usr/lib/quick-presenter/quick-presenter")
        file = next(c for c in self.generate()["components"] if c["name"] == "launcher")
        self.assertNotIn("hashes", file)
        self.assertEqual(file["properties"][0]["value"], "/usr/lib/quick-presenter/quick-presenter")

    def windows_bom(self, version="1.4.0.0", architecture="x64"):
        artifact = self.root / "QuickPresenter-1.4.0.msix"
        artifact.write_bytes(b"MSIX input")
        (self.pdfium / "lib/libpdfium.so").rename(self.pdfium / "lib/pdfium.dll")
        (self.payload / "AppxManifest.xml").write_text(
            '<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">'
            f'<Identity Name="Store.Name" Publisher="CN=Publisher" Version="{version}" '
            f'ProcessorArchitecture="{architecture}" /></Package>')
        target = "x86_64-pc-windows-msvc"
        cargo = copy.deepcopy(self.cargo)
        cargo["metadata"]["properties"][0]["value"] = target
        manifest_path = self.repo / "scripts/pdfium_manifest.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["assets"]["pdfium-win-x64.tgz"] = {"sha256": "c" * 64}
        manifest_path.write_text(json.dumps(manifest))
        return sbom.make_bom(cargo, artifact, self.payload, target, REVISION, self.repo)

    def test_msix_submission_identity_is_recorded(self):
        bom = self.windows_bom()
        metadata = {p["name"]: p["value"] for p in bom["metadata"]["component"]["properties"]}
        self.assertEqual(metadata["quick-presenter:package-identity"], "Store.Name")
        self.assertEqual(metadata["quick-presenter:package-version"], "1.4.0.0")
        self.assertIn("re-sign", metadata["quick-presenter:package-scope"])

    def test_msix_identity_version_mismatch_fails(self):
        with self.assertRaisesRegex(ValueError, "MSIX package version"):
            self.windows_bom(version="1.3.0.0")

    def test_msix_identity_architecture_mismatch_fails(self):
        with self.assertRaisesRegex(ValueError, "MSIX architecture"):
            self.windows_bom(architecture="arm64")

    def test_msix_path_traversal_is_rejected(self):
        artifact = self.root / "unsafe.msix"
        for name in ("../escape", "\\..\\escape", "C:/escape", "/escape"):
            with self.subTest(name=name):
                with zipfile.ZipFile(artifact, "w") as archive:
                    archive.writestr(name, "unsafe")
                with self.assertRaisesRegex(ValueError, "Unsafe MSIX"):
                    with sbom.unpack(artifact):
                        self.fail("Unsafe archive was accepted")

    def test_msix_payload_is_extracted(self):
        artifact = self.root / "safe.msix"
        with zipfile.ZipFile(artifact, "w") as archive:
            archive.writestr("pdfium/VERSION", "version")
        with sbom.unpack(artifact) as payload:
            self.assertEqual((payload / "pdfium/VERSION").read_text(), "version")
        self.assertFalse(payload.exists())


if __name__ == "__main__":
    unittest.main()

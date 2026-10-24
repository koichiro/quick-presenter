"""Check publication links and transcript retention without network or GUI access."""
from datetime import date
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from urllib.parse import unquote, urlsplit


REPO = Path(__file__).resolve().parents[1]
GITHUB_MAIN = "https://github.com/koichiro/quick-presenter/blob/main/"
PUBLICATION_DOCS = (
    "README.md",
    "CHANGELOG.md",
    "docs/CLI.md",
    "docs/GUI_SMOKE_CHECKLIST.md",
    "docs/MAC_APP_STORE.md",
    "docs/PACKAGING.md",
    "docs/RELEASE.md",
    "docs/SPEAKER_NOTE_COMPATIBILITY.md",
    "docs/validation/README.md",
    "docs/validation/ISSUE400_HIDDEN_SLIDE.md",
    "docs/validation/V200_WINDOWS_20261016.md",
    "docs/validation/V250_RELEASE_INTEGRATION_20261024.md",
)
REMOVED_TRANSCRIPTS = (
    "docs/validation/V200_WINDOWS_20261016/cli-metadata.stdout",
    "docs/validation/V200_WINDOWS_20261016/sandbox.stdout",
    "docs/validation/V200_WINDOWS_20261016/gui-smoke.md",
    "docs/research/speaker-notes/smoke-open.txt",
    "docs/research/speaker-notes/pdfium-api.txt",
)


def prose(path):
    # These documents use fenced code blocks and inline Markdown links.
    return re.sub(r"(?ms)^```[^\n]*\n.*?^```[ \t]*$", "",
                  path.read_text(encoding="utf-8"))


def links(path):
    return re.findall(r"\]\(([^()\s]+)\)", prose(path))


def anchors(path):
    text = prose(path)
    result = set(re.findall(r'<a\s+id="([^"]+)"', text))
    occurrences = {}
    for heading in re.findall(r"(?m)^#{1,6}\s+(.+?)\s*#*\s*$", text):
        heading = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", heading)
        heading = re.sub(r"<[^>]*>", "", heading)
        slug = re.sub(r"[^\w -]", "", heading.lower()).replace(" ", "-")
        number = occurrences.get(slug, 0)
        occurrences[slug] = number + 1
        result.add(f"{slug}-{number}" if number else slug)
    return result


class PublicationDocsTests(unittest.TestCase):
    def test_publication_links_resolve_to_local_files_and_anchors(self):
        for name in PUBLICATION_DOCS:
            source = REPO / name
            for reference in links(source):
                # Validate new GitHub main links against this checkout as well;
                # external sites and historical release revisions need no HTTP.
                if reference.startswith(GITHUB_MAIN):
                    base = REPO
                    reference = reference[len(GITHUB_MAIN):]
                else:
                    base = source.parent
                url = urlsplit(reference)
                if url.scheme or url.netloc:
                    continue
                with self.subTest(document=name, link=reference):
                    target = base / unquote(url.path) if url.path else source
                    # This existing link is populated by fetch_pdfium.py and is
                    # intentionally absent from a clean documentation checkout.
                    if target.resolve() == REPO / "pdfium/LICENSE":
                        continue
                    self.assertTrue(target.exists(), f"Missing link target: {target}")
                    if url.fragment and target.suffix == ".md":
                        self.assertIn(unquote(url.fragment), anchors(target))

    def test_legacy_store_guide_anchors_remain_available(self):
        self.assertTrue({"experimental-v250-control-build",
                         "clicontrol-limitation-in-v200"}.issubset(
                             anchors(REPO / "docs/MAC_APP_STORE.md")))

    def test_release_and_validation_sections_remain_addressable(self):
        sections = {
            "docs/MAC_APP_STORE.md": {"v250-cli-control"},
            "docs/RELEASE.md": {"v250-customer-store-qualification-timing",
                                "v200-distribution-and-trust-policy"},
            "docs/validation/README.md": {"v250-release-status",
                                         "artifact-and-runtime-results",
                                         "security-and-release-decisions",
                                         "historical-records"},
        }
        for name, expected in sections.items():
            with self.subTest(document=name):
                self.assertTrue(expected.issubset(anchors(REPO / name)))

    def test_packaged_cli_uses_absolute_links_for_new_store_guidance(self):
        # Packages ship CLI.md and CONTROL_PROTOCOL.md, not the Store guide
        # or validation index. Their new links must work outside the checkout.
        references = links(REPO / "docs/CLI.md")
        for target in ("docs/validation/README.md",
                       "docs/MAC_APP_STORE.md#v250-cli-control"):
            with self.subTest(target=target):
                self.assertIn(GITHUB_MAIN + target, references)

    def test_readme_guides_do_not_pin_users_to_v200(self):
        references = links(REPO / "README.md")
        for target in ("docs/CLI.md", "docs/CONTROL_PROTOCOL.md",
                       "docs/schemas/qp-export.schema.json",
                       "examples/autopresent/README.md"):
            with self.subTest(target=target):
                self.assertIn(target, references)
        for reference in references:
            with self.subTest(link=reference):
                self.assertNotRegex(reference, r"/(?:blob|tree)/v2\.0\.0/")

    def test_latest_changelog_release_matches_package_version(self):
        manifest = tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))
        header = re.search(r"(?m)^## (.+)$", prose(REPO / "CHANGELOG.md"))
        self.assertIsNotNone(header)
        version = manifest["package"]["version"]
        self.assertRegex(header.group(1), rf"^\[{re.escape(version)}\]\(")

    def test_published_release_history_has_unique_dated_tag_links(self):
        text = prose(REPO / "CHANGELOG.md")
        headers = re.findall(r"(?m)^## \[([^]]+)\]\(([^)]+)\) — (\S+)$", text)
        versions = [version for version, _, _ in headers]
        for version, published in (("2.5.0", "2026-10-24"), ("2.0.0", "2026-10-15")):
            with self.subTest(version=version):
                self.assertEqual(versions.count(version), 1)
                _, url, release_date = headers[versions.index(version)]
                self.assertEqual(url, "https://github.com/koichiro/quick-presenter/"
                                 f"releases/tag/v{version}")
                self.assertEqual(date.fromisoformat(release_date), date.fromisoformat(published))
        self.assertLess(versions.index("2.5.0"), versions.index("2.0.0"))
        self.assertLess(versions.index("2.0.0"), versions.index("1.5.0"))

    def test_removed_transcripts_have_no_remaining_document_references(self):
        documents = [REPO / "README.md", REPO / "CHANGELOG.md",
                     *(REPO / "docs").rglob("*.md")]
        removed_paths = {(REPO / path).resolve() for path in REMOVED_TRANSCRIPTS}
        for document in documents:
            text = document.read_text(encoding="utf-8")
            for reference in links(document):
                if reference.startswith(GITHUB_MAIN):
                    base = REPO
                    reference = reference[len(GITHUB_MAIN):]
                else:
                    base = document.parent
                url = urlsplit(reference)
                if url.scheme or url.netloc or not url.path:
                    continue
                with self.subTest(document=document.relative_to(REPO), link=reference):
                    self.assertNotIn((base / unquote(url.path)).resolve(), removed_paths)
            for removed in REMOVED_TRANSCRIPTS:
                # Also reject plain-text paths; unrelated manual-gui-smoke
                # reports remain valid. Markdown paths use '/' on every OS.
                relative = Path(removed).relative_to("docs").as_posix()
                with self.subTest(document=document.relative_to(REPO), reference=relative):
                    self.assertNotIn(relative, text)

    def test_removed_transcripts_are_not_tracked_even_if_regenerated_locally(self):
        result = subprocess.run(["git", "ls-files", "-z", "--", *REMOVED_TRANSCRIPTS],
                                cwd=REPO, check=True, capture_output=True, timeout=10)
        self.assertEqual(result.stdout, b"")


class TranscriptIgnoreTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        # Isolate the real rules from developer/global excludes and tracked files.
        subprocess.run(["git", "init", "--quiet", str(self.root)],
                       check=True, capture_output=True, timeout=10)
        shutil.copyfile(REPO / ".gitignore", self.root / ".gitignore")
        (self.root / "empty-excludes").touch()

    def assert_ignored(self, paths, expected):
        result = subprocess.run(
            ["git", "-c", f"core.excludesFile={self.root / 'empty-excludes'}",
             "check-ignore", "--no-index", "--stdin", "-z"],
            input="\0".join(paths) + "\0", cwd=self.root, capture_output=True,
            text=True, timeout=10)
        self.assertEqual(result.returncode, 0 if expected else 1, result.stderr)
        self.assertEqual(set(result.stdout.split("\0")) - {""}, set(paths) if expected else set())

    def test_validation_transcripts_are_ignored_at_every_depth(self):
        paths = [f"docs/validation/{directory}{name}"
                 for directory in ("", "release/", "release/platform/", "release with spaces/")
                 for name in ("cli.stdout", "renderer.stderr", "sandbox.log", "gui-smoke.md")]
        self.assert_ignored(paths, expected=True)

    def test_removed_transcripts_stay_ignored_when_regenerated(self):
        self.assert_ignored(REMOVED_TRANSCRIPTS, expected=True)

    def test_result_summaries_inventories_and_research_fixtures_are_not_ignored(self):
        self.assert_ignored([
            "docs/validation/README.md",
            "docs/validation/V200_WINDOWS_20261016.md",
            "docs/validation/V200_WINDOWS_20261016/package-verification.json",
            "docs/validation/V200_WINDOWS_20261016/binary-hashes.json",
            "docs/validation/V200_WINDOWS_20261016/manual-observations.md",
            "docs/validation/V200_WINDOWS_20261016/cold-start-summary.json",
            "docs/validation/V200_WINDOWS_20261016/live-cli-summary.json",
            "docs/validation/V200_WINDOWS_20261016/sandbox.json",
            "docs/research/speaker-notes/inspection.json",
            "docs/research/speaker-notes/pdfium-probe.txt",
            "docs/research/speaker-notes/beamer-annotations.pdf",
            "tests/fixtures/marp-speaker-notes.pdf",
        ], expected=False)

    def test_transcript_rules_do_not_ignore_unrelated_paths_or_similar_names(self):
        self.assert_ignored([
            "docs/research/other/smoke-open.txt",
            "docs/research/speaker-notes/nested/pdfium-api.txt",
            "docs/research/speaker-notes/smoke-open.txt.md",
            "docs/validation/run.stdout.json",
            "docs/validation/release/manual-gui-smoke.md",
            "docs/validation/release/gui-smoke-summary.md",
            "docs/validation-other/cli.stdout",
            "examples/docs/validation/cli.stdout",
            "examples/docs/research/speaker-notes/pdfium-api.txt",
            "tests/cli.stdout",
        ], expected=False)


if __name__ == "__main__":
    unittest.main()

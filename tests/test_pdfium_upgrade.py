"""Protect the upgrade decision against false passes."""
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location("check_pdfium_upgrade", SCRIPTS / "check_pdfium_upgrade.py")
upgrade = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(upgrade)


def sample():
    return {"deck.pdf": {"page_count": 1, "input_sha256": "input", "pages": [
        {"notes_sha256": "notes", "text_sha256": "text", "renders": {
            str(width): {"width": width, "height": 100, "sha256": "pixels", "median_ms": 10}
            for width in upgrade.WIDTHS}}]}}


class DecisionTests(unittest.TestCase):
    def test_interrupted_run_writes_a_failure_report(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "run"
            arguments = ["check_pdfium_upgrade", "--baseline-manifest", "unused.json",
                         "--output", str(output)]
            with patch.object(sys, "argv", arguments), \
                    patch.object(sys, "stdout", io.StringIO()), \
                    patch.object(upgrade.platform, "platform", return_value="test-host"), \
                    patch.object(upgrade.subprocess, "check_output", side_effect=["commit", ""]), \
                    patch.object(upgrade, "command", side_effect=KeyboardInterrupt):
                self.assertEqual(upgrade.main(), 1)
            report = json.loads((output / "report.json").read_text())
            self.assertEqual(report["decision"], "FAIL")
            self.assertEqual(report["error"], "KeyboardInterrupt")

    def test_identical_results_pass(self):
        self.assertEqual(upgrade.compare(sample(), sample()), ([], []))

    def test_missing_fixture_fails(self):
        self.assertTrue(upgrade.compare(sample(), {})[0])

    def test_page_count_change_fails(self):
        candidate = sample()
        candidate["deck.pdf"]["page_count"] = 2
        self.assertTrue(upgrade.compare(sample(), candidate)[0])

    def test_input_change_fails(self):
        candidate = sample()
        candidate["deck.pdf"]["input_sha256"] = "different"
        self.assertTrue(upgrade.compare(sample(), candidate)[0])

    def test_incomplete_page_evidence_fails(self):
        candidate = sample()
        candidate["deck.pdf"]["pages"] = []
        self.assertTrue(upgrade.compare(sample(), candidate)[0])

    def test_text_and_notes_changes_require_review(self):
        for field in ("notes_sha256", "text_sha256"):
            candidate = sample()
            candidate["deck.pdf"]["pages"][0][field] = "changed"
            failed, review = upgrade.compare(sample(), candidate)
            self.assertFalse(failed)
            self.assertTrue(review)

    def test_pixel_change_requires_review_even_with_same_dimensions(self):
        candidate = sample()
        candidate["deck.pdf"]["pages"][0]["renders"]["640"]["sha256"] = "changed"
        self.assertTrue(upgrade.compare(sample(), candidate)[1])

    def test_review_is_bound_to_exact_evidence_and_cannot_waive_failures_or_latency(self):
        record = {"fingerprint": "reviewed", "rationale": "Inspected gradient rounding differences"}
        for decision, fingerprint, issues, expected in [
            ("REVIEW", "reviewed", ["pixels changed"], "PASS"),
            ("REVIEW", "different", ["pixels changed"], "REVIEW"),
            ("FAIL", "reviewed", [], "FAIL"),
            ("REVIEW", "reviewed", ["render latency regression"], "REVIEW"),
        ]:
            report = dict(decision=decision, review_fingerprint=fingerprint, review=issues)
            upgrade.apply_review(report, [record])
            self.assertEqual(report["decision"], expected)

    def test_review_fingerprint_ignores_timing_but_binds_pixels_and_native_library(self):
        report = {label: {"fixtures": sample(), "metadata": {"library_sha256": "native"}}
                  for label in ("baseline", "candidate")}
        original = upgrade.review_fingerprint(report)
        render = report["candidate"]["fixtures"]["deck.pdf"]["pages"][0]["renders"]["640"]
        render["median_ms"] = 20
        self.assertEqual(original, upgrade.review_fingerprint(report))
        render["sha256"] = "different"
        self.assertNotEqual(original, upgrade.review_fingerprint(report))
        render["sha256"] = "pixels"
        report["candidate"]["metadata"]["library_sha256"] = "different"
        self.assertNotEqual(original, upgrade.review_fingerprint(report))

    def test_dimensions_change_fails(self):
        candidate = sample()
        candidate["deck.pdf"]["pages"][0]["renders"]["640"]["height"] = 101
        self.assertTrue(upgrade.compare(sample(), candidate)[0])

    def test_small_timing_noise_does_not_block_but_large_regression_does(self):
        candidate = sample()
        render = candidate["deck.pdf"]["pages"][0]["renders"]["640"]
        render["median_ms"] = 30
        self.assertEqual(upgrade.compare(sample(), candidate), ([], []))
        render["median_ms"] = 61
        self.assertTrue(upgrade.compare(sample(), candidate)[1])


if __name__ == "__main__":
    unittest.main()

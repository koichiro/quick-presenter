#!/usr/bin/env python3
"""Verify source statistics classification without requiring cloc."""

import importlib.util
from pathlib import Path
import unittest


SPEC = importlib.util.spec_from_file_location(
    "stats", Path(__file__).resolve().parents[1] / "scripts/stats.py"
)
stats = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stats)


class SourceStatsTests(unittest.TestCase):
    def test_inline_module_does_not_consume_following_implementation(self):
        source = '''fn before() {}
#[cfg(test)]
mod tests {
    #[test]
    fn example() { assert_eq!(1, 1); }
}
fn after() {}
'''
        implementation, tests = stats.split_rust(source)
        self.assertIn("fn before()", implementation)
        self.assertIn("fn after()", implementation)
        self.assertNotIn("fn example()", implementation)
        self.assertIn("fn example()", tests)
        self.assertNotIn("fn after()", tests)

    def test_comments_and_literals_do_not_change_item_boundaries(self):
        source = '''// #[cfg(test)] fn fake() {}
#[cfg(test)]
fn example<'a>() {
    /* nested /* } */ comment */
    let raw = r##"} #[cfg(test)] {"##;
    let escaped = "\\\"}\\\"";
    let character = '}';
}
fn after() {}
'''
        implementation, tests = stats.split_rust(source)
        self.assertIn("fn after()", implementation)
        self.assertIn("let character", tests)
        self.assertNotIn("fn after()", tests)
        self.assertEqual(len(source), len(stats.rust_structure(source)))

    def test_test_only_import_fields_and_methods(self):
        source = '''#[cfg(test)]
use std::{fs, io};
struct Example {
    #[cfg(test)]
    calls: usize,
    value: usize,
}
impl Example {
    #[cfg(test)]
    fn calls(&self) -> usize { self.calls }
    fn value(&self) -> usize { self.value }
}
'''
        implementation, tests = stats.split_rust(source)
        self.assertNotIn("use std", implementation)
        self.assertNotIn("calls:", implementation)
        self.assertIn("value:", implementation)
        self.assertIn("fn value", implementation)
        self.assertIn("use std::{fs, io};", tests)
        self.assertIn("calls: usize,", tests)
        self.assertIn("fn calls", tests)

    def test_compound_predicates_require_test(self):
        for expression in ("test", "all(test, unix)", "all(unix, test)",
                           "all(test, any(unix, windows))", "not(not(test))"):
            self.assertFalse(stats.cfg_can_be_true_without_test(expression), expression)
        for expression in ("unix", "any(test, unix)", "not(test)",
                           "any(test, not(unix))", "all()"):
            self.assertTrue(stats.cfg_can_be_true_without_test(expression), expression)
        source = '''#[cfg(all(test, target_os = "windows"))]
fn helper() {}
#[cfg(any(test, unix))]
fn production() {}
'''
        implementation, tests = stats.split_rust(source)
        self.assertIn("fn helper", tests)
        self.assertIn("fn production", implementation)
        self.assertNotIn("fn production", tests)


if __name__ == "__main__":
    unittest.main()

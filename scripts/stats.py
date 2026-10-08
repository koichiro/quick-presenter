#!/usr/bin/env python3
"""Count source lines with cloc, separating Rust test-only items."""

import argparse
import collections
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
EXTENSIONS = {".rs", ".c", ".slint", ".py", ".sh", ".ps1", ".mjs", ".html", ".css"}
DIRECTORIES = {"src", "ui", "tests", "scripts", "website", "docker"}
CFG_START = re.compile(r"#\s*\[\s*cfg\s*\(")
# Mask comments and literals before matching item boundaries. Lifetimes remain
# visible; character literals, escaped strings, and raw strings are masked.
TOKENS = re.compile(
    r'//[^\n]*|/\*|(?:br|cr|r)(?P<hashes>\#*)"|(?:b|c)?"'
    r"|(?:b)?'(?:\\(?:u\{[^}]*\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'"
)


def rust_structure(source):
    masked = list(source)
    position = 0
    while match := TOKENS.search(source, position):
        start, end = match.span()
        token = match.group()
        if token == "/*":
            depth = 1
            while depth:
                next_token = re.search(r"/\*|\*/", source[end:])
                if next_token is None:
                    raise ValueError("unterminated Rust block comment")
                depth += 1 if next_token.group() == "/*" else -1
                end += next_token.end()
        elif match.group("hashes") is not None:
            closing = '"' + match.group("hashes")
            closing_at = source.find(closing, end)
            if closing_at < 0:
                raise ValueError("unterminated Rust raw string")
            end = closing_at + len(closing)
        elif token.endswith('"'):
            closing = re.search(r'\\[\s\S]|"', source[end:])
            while closing and closing.group() != '"':
                end += closing.end()
                closing = re.search(r'\\[\s\S]|"', source[end:])
            if closing is None:
                raise ValueError("unterminated Rust string")
            end += closing.end()
        for index in range(start, end):
            if source[index] != "\n":
                masked[index] = " "
        position = end
    return "".join(masked)


def cfg_can_be_true_without_test(expression):
    """Conservatively evaluate cfg with test=false and other atoms unknown."""
    def values(expression):
        expression = expression.strip()
        if expression == "test":
            return {False}
        call = re.fullmatch(r"(all|any|not)\s*\(([\s\S]*)\)", expression)
        if not call:
            return {False, True}
        arguments = []
        depth = 0
        start = 0
        body = call[2]
        for index, char in enumerate(body):
            depth += (char == "(") - (char == ")")
            if char == "," and depth == 0:
                arguments.append(body[start:index])
                start = index + 1
        if body[start:].strip():
            arguments.append(body[start:])
        parts = [values(arg) for arg in arguments]
        operator = call[1]
        if operator == "not":
            if len(parts) != 1:
                raise ValueError("invalid not() cfg predicate")
            return {not value for value in parts[0]}
        result = {operator == "all"}
        for part in parts:
            result = {(left and right) if operator == "all" else (left or right)
                      for left in result for right in part}
        return result
    return True in values(expression)


def test_attributes(structure):
    for match in CFG_START.finditer(structure):
        depth = 1
        for index in range(match.end(), len(structure)):
            depth += (structure[index] == "(") - (structure[index] == ")")
            if depth == 0:
                closing = re.match(r"\s*\]", structure[index + 1:])
                if closing is None:
                    raise ValueError("invalid cfg attribute")
                if not cfg_can_be_true_without_test(structure[match.end():index]):
                    yield match.start(), index + 1 + closing.end()
                break
        else:
            raise ValueError("unterminated cfg attribute")


def split_rust(source):
    """Extract Rust items whose cfg predicate requires test=true."""
    structure = rust_structure(source)
    ranges = []
    end = 0
    for start, attribute_end in test_attributes(structure):
        if start < end:
            continue
        # Skip balanced signatures/attributes, then consume the item body or
        # semicolon. Test-only fields end at a top-level comma.
        stack = []
        body = False
        for index in range(attribute_end, len(structure)):
            char = structure[index]
            if char in "([{":
                if char == "{" and not stack:
                    body = True
                stack.append(char)
            elif char in ")]}":
                if not stack:
                    raise ValueError("unbalanced test-only Rust item")
                stack.pop()
                if body and not stack:
                    end = index + 1
                    if structure[end:end + 1] == ";":
                        end += 1
                    break
            elif char in ";," and not stack:
                end = index + 1
                break
        else:
            raise ValueError("unterminated test-only Rust item")
        ranges.append((start, end))
    implementation = list(source)
    tests = []
    for start, end in ranges:
        tests.append(source[start:end])
        for index in range(start, end):
            if source[index] != "\n":
                implementation[index] = " "
    return "".join(implementation), "\n".join(tests)


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ref", help="Git revision to count (default: working tree)")
    parser.add_argument("--cloc", default="cloc", help="cloc executable or Perl script")
    args = parser.parse_args()
    executable = shutil.which(args.cloc)
    if executable is None and args.cloc.endswith(".pl") and Path(args.cloc).is_file():
        executable = str(Path(args.cloc).resolve())
    if executable is None:
        parser.error("cloc is required; on macOS run: brew install cloc")
    if args.ref:
        revision = git("rev-parse", "--verify", f"{args.ref}^{{commit}}").decode().strip()
        paths = git("ls-tree", "-rz", "--name-only", revision).decode().split("\0")
        label = f"{args.ref} ({revision[:7]})"
    else:
        paths = git("ls-files", "-z", "--cached", "--others", "--exclude-standard").decode().split("\0")
        label = "working tree (tracked and non-ignored source files)"
    selected = sorted({p for p in paths if p and
                       (p == "build.rs" or p.split("/")[0] in DIRECTORIES) and
                       (Path(p).suffix in EXTENSIONS or Path(p).name == "Dockerfile")})
    groups = collections.defaultdict(lambda: {"files": set(), "code": 0, "comment": 0, "blank": 0})
    with tempfile.TemporaryDirectory(prefix="quick-presenter-stats-") as temporary:
        metadata = {}
        for name in selected:
            source = (git("show", f"{revision}:{name}").decode() if args.ref
                      else (ROOT / name).read_text())
            if name.startswith("tests/"):
                parts = {"Tests": source}
            elif name.startswith("website/"):
                parts = {"Website": source}
            elif name.startswith(("src/", "ui/")):
                if name.endswith(".rs"):
                    implementation, tests = split_rust(source)
                    parts = {"Implementation": implementation, "Tests": tests}
                else:
                    parts = {"Implementation": source}
            else:
                parts = {"Build and tools": source}
            for group, content in parts.items():
                if not content.strip():
                    continue
                path = Path(temporary) / group / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
                metadata[str(path)] = (group, name)
        command = (["perl", executable] if executable.endswith(".pl") else [executable])
        report = json.loads(subprocess.check_output(
            [*command, "--json", "--by-file", "--quiet", "--skip-uniqueness", temporary]
        ))
        for path, (group, name) in metadata.items():
            if path not in report:
                raise ValueError(f"cloc did not report source file: {name}")
            row = groups[group]
            row["files"].add(name)
            for key in ("code", "comment", "blank"):
                row[key] += report[path][key]
    total = sum(row["code"] for row in groups.values())
    print(f"Source: {label}")
    print("Code lines exclude comments and blanks. Vendor, documents and assets are excluded.")
    print(f"{'Category':<20} {'Files':>6} {'Code':>9} {'Code %':>8}")
    for group in ("Implementation", "Tests", "Build and tools", "Website"):
        row = groups[group]
        percent = row["code"] / total * 100 if total else 0
        print(f"{group:<20} {len(row['files']):>6} {row['code']:>9,} {percent:>7.2f}%")
    print(f"{'Total':<20} {len(selected):>6} {total:>9,}")
    implementation = groups["Implementation"]["code"]
    tests = groups["Tests"]["code"]
    if implementation + tests:
        print(f"Tests / (implementation + tests): {tests / (implementation + tests):.2%}")
    if implementation:
        print(f"Test-to-implementation ratio: {tests / implementation:.2f}:1")
    print("Rust cfg predicates requiring test=true and tests/ belong to Tests; file counts can overlap.")


if __name__ == "__main__":
    main()

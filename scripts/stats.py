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
GROUPS = (
    "Rust application", "Native bridges", "Slint UI", "Build and tools",
    "Website HTML", "Website CSS", "Website JavaScript",
    "Rust unit tests", "Rust integration tests", "Python tests", "Test fixtures",
)
TEST_GROUPS = {"Rust unit tests", "Rust integration tests", "Python tests", "Test fixtures"}
APPLICATION_GROUPS = {"Rust application", "Native bridges", "Slint UI"}
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


def rust_test_ranges(source):
    """Find Rust items whose cfg predicate requires test=true."""
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
    return ranges


def split_rust(source, ranges=None):
    """Separate implementation from test-only items without changing syntax."""
    if ranges is None:
        ranges = rust_test_ranges(source)
    implementation = list(source)
    tests = []
    for start, end in ranges:
        tests.append(source[start:end])
        for index in range(start, end):
            if source[index] != "\n":
                implementation[index] = " "
    return "".join(implementation), "\n".join(tests)


def source_parts(name, source):
    """Return category, source text and original physical line count."""
    lines = len(source.splitlines())
    if name.startswith("tests/fixtures/"):
        return [("Test fixtures", source, lines)]
    if name.startswith("tests/"):
        group = "Rust integration tests" if name.endswith(".rs") else "Python tests"
        return [(group, source, lines)]
    if name.startswith("website/"):
        group = {".html": "Website HTML", ".css": "Website CSS", ".mjs": "Website JavaScript"}[Path(name).suffix]
        return [(group, source, lines)]
    if name.startswith("src/") and name.endswith(".rs"):
        ranges = rust_test_ranges(source)
        implementation, tests = split_rust(source, ranges)
        test_lines = set()
        for start, end in ranges:
            first = source.count("\n", 0, start)
            last = source.count("\n", 0, end - 1)
            test_lines.update(range(first, last + 1))
        return [("Rust application", implementation, lines - len(test_lines)),
                ("Rust unit tests", tests, len(test_lines))]
    if name.startswith("src/"):
        return [("Native bridges", source, lines)]
    if name.startswith("ui/"):
        return [("Slint UI", source, lines)]
    return [("Build and tools", source, lines)]


def format_report(groups, label):
    """Render a Rails-stats-style table and explicit LOC ratio denominators."""
    total_loc = sum(row["code"] for row in groups.values())
    total_lines = sum(row["lines"] for row in groups.values())
    files = set().union(*(row["files"] for row in groups.values()))
    rows = []
    for group in GROUPS:
        row = groups.get(group, {"files": set(), "lines": 0, "code": 0})
        percent = row["code"] / total_loc * 100 if total_loc else 0
        rows.append([group, str(len(row["files"])), f"{row['lines']:,}",
                     f"{row['code']:,}", f"{percent:.2f}%"])
    total = ["Total", str(len(files)), f"{total_lines:,}", f"{total_loc:,}",
             "100.00%" if total_loc else "0.00%"]
    headers = ["Name", "Files", "Lines", "LOC", "LOC %"]
    widths = [max(len(row[i]) for row in [headers, *rows, total])
              for i in range(len(headers))]
    border = "+" + "+".join("-" * (width + 2) for width in widths) + "+"

    def line(row):
        return "| " + " | ".join(value.ljust(width) if index == 0 else value.rjust(width)
                                  for index, (value, width) in enumerate(zip(row, widths))) + " |"

    output = [f"Source: {label}", border, line(headers), border]
    output.extend(line(row) for row in rows)
    output.extend([border, line(total), border])
    tests = sum(row["code"] for group, row in groups.items() if group in TEST_GROUPS)
    code = total_loc - tests
    ratio = f"1:{tests / code:.2f}" if code else "n/a (no code LOC)"
    output.append(f"  Code LOC: {code:,}     Test LOC: {tests:,}     Code to Test Ratio: {ratio}")
    application = sum(row["code"] for group, row in groups.items() if group in APPLICATION_GROUPS)
    if application + tests:
        output.append(f"  Application LOC: {application:,}     Test share (application + tests): {tests / (application + tests):.2%}")
    output.extend([
        "Lines include comments and blanks; LOC excludes them. LOC % uses total LOC.",
        "Code LOC includes application, build/tools and website; Test LOC includes fixtures.",
        "Vendor, documents, configuration and assets are excluded. File counts can overlap.",
        "Rust test-only cfg items are tests; physical lines touching those items belong to tests.",
    ])
    return "\n".join(output)


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
    groups = collections.defaultdict(lambda: {"files": set(), "code": 0, "lines": 0})
    with tempfile.TemporaryDirectory(prefix="quick-presenter-stats-") as temporary:
        metadata = {}
        for name in selected:
            source = (git("show", f"{revision}:{name}").decode() if args.ref
                      else (ROOT / name).read_text())
            for group, content, lines in source_parts(name, source):
                groups[group]["lines"] += lines
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
            row["code"] += report[path]["code"]
    print(format_report(groups, label))


if __name__ == "__main__":
    main()

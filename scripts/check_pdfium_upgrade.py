#!/usr/bin/env python3
"""Compare two checksum-pinned PDFium builds through Quick Presenter's helper.

Exit 0: host compatibility gate passed; 1: failure; 2: review required.
This debug-helper gate does not certify signed packages or other platforms.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import platform
import queue
import statistics
import struct
import subprocess
import sys
import threading
import time

import fetch_pdfium

ROOT = Path(__file__).resolve().parents[1]
HEADER = struct.Struct("<4sHII")
WIDTHS = (640, 1920)
REPEATS = 3


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def native_path(path):
    return {"encoding": "Windows" if os.name == "nt" else "Unix",
            "bytes": list(str(path).encode("utf-16-le") if os.name == "nt" else os.fsencode(path))}


class Helper:
    def __init__(self, binary, library, log):
        env = {k: v for k, v in os.environ.items()
               if not k.startswith("QUICK_PRESENTER_") and k != "PDFIUM_DYNAMIC_LIB_PATH"}
        env.update(PDFIUM_DYNAMIC_LIB_PATH=str(library),
                   QUICK_PRESENTER_HELPER_TEST_RAW_PROTOCOL="1",
                   QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE="1")
        self.log = log.open("wb")
        self.process = subprocess.Popen([str(binary), "--renderer-helper"], cwd=ROOT,
                                        env=env, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=self.log)
        self.responses = queue.Queue(maxsize=2)
        self.request_id = 0
        self.reader = threading.Thread(target=self.read_loop, daemon=True)
        self.reader.start()

    def read_exact(self, count):
        data = bytearray()
        while len(data) < count:
            chunk = self.process.stdout.read(count - len(data))
            require(chunk, "helper exited or truncated its response")
            data.extend(chunk)
        return bytes(data)

    def read_loop(self):
        try:
            while True:
                magic, version, control, pixels = HEADER.unpack(self.read_exact(HEADER.size))
                require(magic == b"QPRP" and version == 4, "renderer protocol changed")
                require(0 < control <= 1024 * 1024 and pixels <= 64 * 1024 * 1024,
                        "oversized helper response")
                body = json.loads(self.read_exact(control))
                self.responses.put((body, self.read_exact(pixels)))
        except Exception as error:
            self.responses.put(error)

    def frame(self, request_id, session):
        try:
            response = self.responses.get(timeout=30)
        except queue.Empty as error:
            raise RuntimeError("helper response exceeded 30 seconds") from error
        if isinstance(response, Exception):
            raise response
        body, pixels = response
        require(body["request_id"] == request_id and body["session_id"] == session,
                "uncorrelated helper response")
        return body, pixels

    def request(self, kind, session=1, **fields):
        self.request_id += 1
        body = json.dumps(dict(request_id=self.request_id, session_id=session,
                               message=dict(kind=kind, **fields))).encode()
        self.process.stdin.write(HEADER.pack(b"QPRP", 4, len(body), 0) + body)
        self.process.stdin.flush()
        response, pixels = self.frame(self.request_id, session)
        if response["message"]["kind"] == "Transfer":
            assembled = bytearray()
            total = response["message"]["chunk"]["total_bytes"]
            require(0 < total <= 8 * 1024 * 1024, "oversized text transfer")
            sequence = 0
            while True:
                chunk = response["message"]["chunk"]
                require(not pixels and chunk["sequence"] == sequence
                        and chunk["total_bytes"] == total and chunk["data"], "invalid transfer")
                assembled.extend(chunk["data"])
                require(len(assembled) <= total, "transfer overflow")
                if len(assembled) == total:
                    break
                sequence += 1
                response, pixels = self.frame(self.request_id, session)
            response = json.loads(assembled)
            require(response["request_id"] == self.request_id
                    and response["session_id"] == session, "uncorrelated text")
        return response["message"], pixels

    def expect(self, kind, expected, **fields):
        message, pixels = self.request(kind, **fields)
        require(message["kind"] == expected, f"{kind}: expected {expected}, got {message}")
        return message, pixels

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=10)
        self.process.stdin.close()
        self.reader.join(timeout=5)
        self.process.stdout.close()
        self.log.close()


def bundle_metadata(directory, manifest):
    version = dict(line.split("=", 1) for line in (directory / "VERSION").read_text().splitlines())
    require(manifest["version"] == "chromium/" + version["BUILD"], "bundle version mismatch")
    flags = dict(line.split(" = ", 1) for line in (directory / "args.gn").read_text().splitlines())
    require(flags.get("pdf_enable_v8") == "false" and flags.get("pdf_enable_xfa") == "false",
            "unexpected V8/XFA profile")
    library_name = {"Darwin": "lib/libpdfium.dylib", "Linux": "lib/libpdfium.so",
                    "Windows": "bin/pdfium.dll"}[platform.system()]
    library = directory / library_name
    files = [directory / "LICENSE", *sorted((directory / "licenses").glob("*"))]
    require(len(files) > 1 and all(p.is_file() and p.stat().st_size for p in files),
            "missing license files")
    return library, dict(version=version, flags=flags, library_sha256=digest(library.read_bytes()),
                         licenses={p.name: digest(p.read_bytes()) for p in files})


def probe(binary, library, output):
    output.mkdir()
    helper = Helper(binary, library, output / "helper.stderr")
    result = {}
    try:
        helper.expect("Hello", "Ready", session=0)
        fixtures = sorted((ROOT / "tests/fixtures").glob("*.pdf"))
        fixtures += [ROOT / "docs/samples/quick-presenter-demo.pdf"]
        invalid = output / "invalid.pdf"
        invalid.write_bytes(b"%PDF-1.7\ninvalid truncated input\n")
        for path in [invalid, ROOT / "tests/fixtures/password-protected.pdf"]:
            helper.close()
            helper = Helper(binary, library, output / f"{path.stem}.stderr")
            helper.expect("Hello", "Ready", session=0)
            message, _ = helper.expect("Open", "Failed", path=native_path(path))
            require(message["code"] == "InvalidPdf", "unexpected PDF rejection")
        for path in fixtures:
            if path.name == "password-protected.pdf":
                continue
            helper.close()
            helper = Helper(binary, library, output / f"{path.stem}.stderr")
            helper.expect("Hello", "Ready", session=0)
            print(f"  {output.name}: {path.name}", flush=True)
            opened, _ = helper.expect("Open", "Opened", path=native_path(path))
            count = opened["page_count"]
            require(0 < count <= 10000, "invalid page count")
            pages = []
            for page in range(count):
                notes, _ = helper.expect("NotesPage", "NotesLoaded", page_index=page)
                text, _ = helper.expect("TextPage", "TextLoaded", page_index=page, full=True)
                renders = {}
                for width in WIDTHS:
                    samples = []
                    first = None
                    for _ in range(REPEATS):
                        start = time.perf_counter()
                        pixels, rgba = helper.expect("Render", "Pixels", page_index=page, width=width)
                        samples.append((time.perf_counter() - start) * 1000)
                        require(pixels["format"] == "Rgba8" and pixels["width"] == width
                                and 0 < pixels["height"] <= 16384
                                and len(rgba) == width * pixels["height"] * 4, "invalid pixels")
                        require(first is None or first == rgba, "nondeterministic repeated render")
                        first = rgba
                    name = f"{path.stem}-{page}-{width}.rgba"
                    (output / name).write_bytes(first)
                    renders[str(width)] = dict(sha256=digest(first), width=width,
                                               height=pixels["height"], file=name,
                                               median_ms=statistics.median(samples))
                pages.append(dict(notes_sha256=digest(json.dumps(notes, sort_keys=True).encode()),
                                  text_sha256=digest(json.dumps(text, sort_keys=True).encode()),
                                  renders=renders))
            for page in reversed(range(count)):
                _, rgba = helper.expect("Render", "Pixels", page_index=page, width=WIDTHS[0])
                require(digest(rgba) == pages[page]["renders"][str(WIDTHS[0])]["sha256"],
                        "backward navigation changed pixels")
            helper.expect("Render", "Failed", page_index=count, width=640)
            helper.expect("TextPage", "Failed", page_index=count, full=False)
            helper.expect("Render", "Pixels", page_index=0, width=640)
            helper.expect("Close", "Closed")
            helper.expect("Shutdown", "Stopped", session=0)
            require(helper.process.wait(timeout=10) == 0, "helper shutdown failed")
            helper.close()
            helper = Helper(binary, library, output / f"{path.stem}-reopen.stderr")
            helper.expect("Hello", "Ready", session=0)
            reopened, _ = helper.expect("Open", "Opened", path=native_path(path))
            require(reopened == opened, "reopen changed metadata")
            _, rgba = helper.expect("Render", "Pixels", page_index=0, width=WIDTHS[0])
            require(digest(rgba) == pages[0]["renders"][str(WIDTHS[0])]["sha256"],
                    "reopen changed rendered pixels")
            helper.expect("Close", "Closed")
            result[path.name] = dict(page_count=count, pages=pages,
                                     input_sha256=digest(path.read_bytes()))
        helper.expect("Shutdown", "Stopped", session=0)
        require(helper.process.wait(timeout=10) == 0, "helper shutdown failed")
        return result
    finally:
        helper.close()


def compare(baseline, candidate):
    failures, review = [], []
    if baseline.keys() != candidate.keys():
        return ["fixture set changed"], []
    for name, old in baseline.items():
        new = candidate[name]
        if len(old["pages"]) != old["page_count"] or len(new["pages"]) != new["page_count"]:
            failures.append(f"{name}: incomplete page evidence")
            continue
        if old["page_count"] != new["page_count"] or old["input_sha256"] != new["input_sha256"]:
            failures.append(f"{name}: page count or input changed")
            continue
        for page, (a, b) in enumerate(zip(old["pages"], new["pages"]), 1):
            label = f"{name} page {page}"
            for field in ("notes_sha256", "text_sha256"):
                if a[field] != b[field]:
                    review.append(f"{label}: {field} changed")
            for width in map(str, WIDTHS):
                x, y = a["renders"][width], b["renders"][width]
                if (x["width"], x["height"]) != (y["width"], y["height"]):
                    failures.append(f"{label} width {width}: dimensions changed")
                elif x["sha256"] != y["sha256"]:
                    review.append(f"{label} width {width}: pixels changed; inspect RGBA artifacts")
                if y["median_ms"] > max(x["median_ms"] * 2, x["median_ms"] + 50):
                    review.append(f"{label} width {width}: render latency regression")
    return failures, review


def pixel_diagnostics(report, output):
    differences = []
    for name, fixture in report["baseline"]["fixtures"].items():
        other = report["candidate"]["fixtures"][name]
        for page, (a, b) in enumerate(zip(fixture["pages"], other["pages"]), 1):
            for width, old in a["renders"].items():
                new = b["renders"][width]
                if old["sha256"] == new["sha256"] or old["height"] != new["height"]:
                    continue
                before = (output / "baseline" / old["file"]).read_bytes()
                after = (output / "candidate" / new["file"]).read_bytes()
                delta = bytes(abs(x - y) for x, y in zip(before, after))
                changed = sum(any(delta[i:i + 4]) for i in range(0, len(delta), 4))
                heat = bytearray()
                for i in range(0, len(delta), 4):
                    heat.extend(min(value * 32, 255) for value in delta[i:i + 3])
                heat_name = old["file"].replace(".rgba", "-diff.ppm")
                (output / heat_name).write_bytes(
                    f"P6\n{old['width']} {old['height']}\n255\n".encode() + heat)
                differences.append(dict(fixture=name, page=page, width=int(width),
                                        max_channel_delta=max(delta),
                                        mean_channel_delta=sum(delta) / len(delta),
                                        changed_pixel_fraction=changed / (len(delta) // 4),
                                        heatmap=heat_name))
    return differences


def review_fingerprint(report):
    evidence = {label: copy.deepcopy(report[label]) for label in ("baseline", "candidate")}
    for build in evidence.values():
        for fixture in build["fixtures"].values():
            for page in fixture["pages"]:
                for render in page["renders"].values():
                    render.pop("median_ms")
    return digest(json.dumps(evidence, sort_keys=True).encode())


def apply_review(report, records):
    # Neither hard failures nor noisy timing measurements can be waived.
    if report["decision"] != "REVIEW" or any("latency" in issue for issue in report["review"]):
        return
    for record in records:
        if (record.get("fingerprint") == report["review_fingerprint"]
                and isinstance(record.get("rationale"), str) and record["rationale"].strip()):
            report.update(decision="PASS", accepted_review=record)
            return


def command(arguments, log, environment=None):
    with log.open("wb") as stream:
        subprocess.run(arguments, cwd=ROOT, env=environment, stdout=stream,
                       stderr=subprocess.STDOUT, check=True, timeout=1800)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-manifest", required=True, type=Path)
    parser.add_argument("--candidate-manifest", type=Path, default=fetch_pdfium.MANIFEST_PATH)
    parser.add_argument("--output", required=True, type=Path, help="new result directory")
    parser.add_argument("--review-record", type=Path, help="reviewed exact evidence fingerprints with rationale")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = dict(schema_version=1, decision="FAIL", platform=platform.platform(),
                  scope="host debug helper; signed package and other platforms require separate gates",
                  widths=WIDTHS, repeats=REPEATS)
    try:
        report["git_commit"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        report["dirty"] = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT))
        command(["cargo", "fmt", "--check"], output / "fmt.log")
        command(["cargo", "check", "--locked", "--all-targets"], output / "check.log")
        target = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version=1"], cwd=ROOT))['target_directory']
        binary = Path(target) / "debug" / ("quick-presenter.exe" if os.name == "nt" else "quick-presenter")
        for label, path in [("baseline", args.baseline_manifest), ("candidate", args.candidate_manifest)]:
            manifest = fetch_pdfium.load_manifest(path)
            (output / f"{label}-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
            directory = output / f"{label}-bundle"
            print(f"Fetching and testing {label}: {manifest['version']}", flush=True)
            command([sys.executable, str(ROOT / "scripts/fetch_pdfium.py"), "--manifest",
                     str(path.resolve()), "--out", str(directory)], output / f"{label}-fetch.log")
            library, metadata = bundle_metadata(directory, manifest)
            environment = {k: v for k, v in os.environ.items() if not k.startswith("QUICK_PRESENTER_")}
            environment["PDFIUM_DYNAMIC_LIB_PATH"] = str(library)
            environment["QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE"] = "1"
            command(["cargo", "test", "--locked"], output / f"{label}-test.log", environment)
            # Integration tests build the executable. Hash it after cargo test,
            # which may replace an earlier dev-profile executable in this path.
            binary_sha256 = digest(binary.read_bytes())
            require(report.get("binary_sha256", binary_sha256) == binary_sha256,
                    "executable changed between baseline and candidate")
            report["binary_sha256"] = binary_sha256
            # A bad override must fail, proving this executable honors library selection.
            negative = Helper(binary, output / "missing-library", output / f"{label}-negative.stderr")
            try:
                negative.expect("Hello", "Ready", session=0)
                try:
                    negative.expect("Open", "Failed", path=native_path(ROOT / "tests/fixtures/marp-speaker-notes.pdf"))
                except RuntimeError:
                    require(negative.process.wait(timeout=10) != 0, "bad override did not fail closed")
            finally:
                negative.close()
            report[label] = dict(manifest=manifest, metadata=metadata,
                                 fixtures=probe(binary, library, output / label))
        failures, review = compare(report["baseline"]["fixtures"], report["candidate"]["fixtures"])
        if report["baseline"]["metadata"]["licenses"] != report["candidate"]["metadata"]["licenses"]:
            review.append("license inventory changed; review distribution obligations")
        if report["baseline"]["metadata"]["flags"] != report["candidate"]["metadata"]["flags"]:
            review.append("build flags changed")
        report.update(failures=failures, review=review,
                      decision="FAIL" if failures else "REVIEW" if review else "PASS")
        report["pixel_differences"] = pixel_diagnostics(report, output)
        report["review_fingerprint"] = review_fingerprint(report)
        report["unreviewed_decision"] = report["decision"]
        if args.review_record:
            apply_review(report, json.loads(args.review_record.read_text(encoding="utf-8")))
    except (Exception, SystemExit, KeyboardInterrupt) as error:
        report.update(decision="FAIL", error=str(error) or type(error).__name__)
    finally:
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"{report['decision']}: {output / 'report.json'}")
    return {"PASS": 0, "FAIL": 1, "REVIEW": 2}[report["decision"]]


if __name__ == "__main__":
    sys.exit(main())

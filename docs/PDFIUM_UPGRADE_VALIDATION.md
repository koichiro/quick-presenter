# PDFium upgrade validation

## Reproducible host gate

Preserve the accepted manifest before changing the pin, then run:

```sh
git show origin/main:scripts/pdfium_manifest.json > /tmp/accepted-pdfium.json
python3 scripts/fetch_pdfium.py --update-manifest --version chromium/8165
python3 -m unittest tests/test_pdfium_upgrade.py tests/test_fetch_pdfium.py
python3 scripts/check_pdfium_upgrade.py \
  --baseline-manifest /tmp/accepted-pdfium.json \
  --review-record scripts/pdfium_upgrade_reviews.json \
  --output artifacts/pdfium-upgrade-8165
```

Use a new output directory for each run. Both manifests are copied into the
result directory. Both native bundles are freshly downloaded using the existing
checksum and safe-extraction checks; the development `pdfium/` is untouched.
The runner uses the same freshly built debug executable and locked Rust
dependencies for both libraries. A deliberately missing library override must
fail, preventing a silently ignored override from producing a false pass.

The JSON report records the commit, working-tree status, executable and native
library hashes, manifests, platform, input hashes, build flags, license inventory,
per-page content hashes, dimensions, pixel hashes, and median render timings.
Cargo and helper logs and raw RGBA images are retained beside the report.
Only repository fixtures are processed; PDF content is not embedded in the JSON.

## Criteria

The script always runs `cargo fmt --check`, `cargo check --locked --all-targets`,
and a debug binary build, followed by `cargo test --locked` separately against
each selected native library. It exercises the actual Quick Presenter helper
protocol and PDF implementation rather than a separate PDF wrapper.

Every unencrypted PDF in `tests/fixtures/` and the README demo is opened and all
pages are rendered at widths 640 and 1920. Each render repeats three times,
requiring identical pixels within a build; the median includes IPC and RGBA
transfer time. Reverse navigation must reproduce the earlier pixels. Closing,
helper shutdown, reopening in a fresh helper, and rerendering page one must
preserve metadata and pixels. A fresh helper per PDF also respects Linux's
single-document confinement. Full page text and speaker notes are compared;
existing Rust tests provide known expected note content. Password protection,
truncated input, out-of-range operations, and subsequent successful rendering
are checked. The Rust suite includes helper failure and recovery tests.

| Decision | Exit | Meaning |
| --- | --- | --- |
| PASS | 0 | All automated host checks passed with identical content and pixels. |
| FAIL | 1 | Download, version/profile, build/test, protocol, timeout, crash, dimensions, deterministic rendering, or lifecycle check failed. |
| REVIEW | 2 | Pixels, text, notes, license inventory, or build flags changed; or render latency exceeded both twice the baseline and baseline + 50 ms. |

Any pixel or content change needs review, even if upstream describes it as a
fix. There is no automatic fuzzy tolerance. Review paired `.rgba` files using
the width/height in `report.json` (RGBA, 8 bits per channel), retain the visual
assessment and upstream rationale in the update record, and obtain reviewer
acceptance. Do not refresh expected results merely to make the gate green.
Timing review requires a repeat on an otherwise idle runner. Missing evidence
or an interrupted run is not a pass; a populated report with a non-PASS decision
cannot be treated as upgrade approval.

For an intentionally accepted content, pixel, license, or build-profile change,
add an object with `fingerprint` (the report's `review_fingerprint`) and a
nonempty `rationale` to `scripts/pdfium_upgrade_reviews.json`. Review that record
in the update PR together with the evidence. The fingerprint binds both full
manifests, native library hashes, metadata, fixture hashes and per-page content
and pixel hashes. Timing samples are excluded because they vary on every run.
The gate accepts only that exact evidence and keeps `unreviewed_decision`, the
differences, and the accepted rationale in the report. A new native build, input,
or render result requires a new review. Hard failures and latency regressions
cannot be overridden by a review record. A reviewed host PASS still leaves the
other platform and package gates mandatory.

## Main acceptance and release scope

The `PDFium upgrade compatibility` PR workflow compares the PR base commit's
accepted manifest to the candidate on macOS ARM64, Linux x64, and Windows x64.
Manual dispatch requires a baseline ref that still contains the old pin.
Require all matrix results plus ordinary CI, review upstream parser/rendering
and security changes and license differences, and record the decision before
merging to main. Branch protection must require these checks if enforcement is
desired; adding a workflow does not itself configure branch protection.

This is a compatibility gate, not an exhaustive vulnerability audit or a claim
that every PDF renders correctly. The corpus covers Marp, Beamer, Google Slides,
Keynote, PowerPoint, Japanese notes, and comment filtering. It does not cover
every font, codec, encryption mode, malformed input, or performance workload.
Add a reproducible regression PDF when an actual failure is found.

Debug-helper passes do not establish signed package sandbox compatibility,
fullscreen behavior, or display correctness. Run the existing signed macOS XPC,
Windows AppContainer/MSIX, Linux confinement and packaged smoke gates from
the release workflows for the candidate before distribution. macOS x64,
Linux ARM64, and Windows x86 assets are checksum-pinned but are not runtime
tested by this matrix; validate them on native runners before claiming support
for a release using those architectures. Preserve the process isolation and
V8/XFA-disabled policy in [PDF Rendering Security](PDF_RENDERING_SECURITY.md).

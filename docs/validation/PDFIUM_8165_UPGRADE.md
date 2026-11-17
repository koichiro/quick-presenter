# PDFium 8165 upgrade assessment

Date: 2026-11-17. Target: `main`; implementation branch:
`codex/validate-pdfium-8165`, based on `52af12f`.

## Candidate and source review

- Accepted pin: `chromium/8111`, PDFium 158.0.8111.0.
- Candidate: [chromium/8165, PDFium 160.0.8165.0](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium/8165),
  published 2026-11-16 17:35 UTC; latest release at review time.
- Candidate source: `1f02bc7d908ab301fa2a2babd87226a19311ff38`.
- The [complete upstream range](https://pdfium.googlesource.com/pdfium/+log/refs/heads/chromium/8111..refs/heads/chromium/8165)
  contains 172 commits. The latest release notes alone cover only 8152 to 8165.
- All six existing platform assets remain available with GitHub release SHA-256
  digests. The candidate manifest retains the same non-V8/non-XFA asset names.

Relevant changes include:

| Area | Changes and Quick Presenter impact |
| --- | --- |
| Parsing and resource handling | [Stream read failures](https://pdfium.googlesource.com/pdfium/+/4bd20bd51662ef0ffca72ccd6d1ac41999f2d009) return failure rather than CHECK-aborting; [OpenJPEG allocation failures](https://pdfium.googlesource.com/pdfium/+/0a42f6e64f28603f3aee118a1c140e9bd0647b71) propagate. These paths remain relevant to standalone PDF parsing and image rendering. |
| Path rendering | Saturating conversions in [DrawPath](https://pdfium.googlesource.com/pdfium/+/8fdce8e1b14def5d34f217c5099936ddcebe8b81) and [CheckSimpleLinePath](https://pdfium.googlesource.com/pdfium/+/4e04b1315dc856dd9d006f1f337eecc05b2ee3ba) address float-to-integer overflow. |
| Text extraction | [Repeated spaces collapse in linear time](https://pdfium.googlesource.com/pdfium/+/43d55e429cf72c21e9a1e60475b50feefa16fda9); supplementary-plane mirroring and RTL/ActualText handling also change. Relevant to CLI slide text as well as note extraction regression checks. |
| Visual output and decoding | [Incremental axial shading](https://pdfium.googlesource.com/pdfium/+/2f8880782ee79e24466642ccb7309c0a74ccc483), SIMD scanline compositing, Lab ICC handling, and Fax/Flate/JPEG rewind changes require image comparisons, not just successful opens. |
| Encryption | [AES key validation](https://pdfium.googlesource.com/pdfium/+/982029caf385f3dca3ba8ed41760699472c9775f) rejects unsupported 192-bit keys. Quick Presenter supplies no passwords; the existing protected-input fixture must continue to fail cleanly. This is not a test of every encrypted PDF variant. |
| Optional engines | XFA/V8 fixes are present upstream, but the selected macOS archives still disable both engines. Do not equate excluded optional paths with a comprehensive security guarantee. |

The four standalone parser/render/text changes previously identified as pending
in [the October security review](SECURITY_REVIEW_20261023.md) are in the candidate
range (OpenJPEG, DrawPath, parser reads, repeated spaces). This establishes
inclusion of those identified changes, not exhaustive CVE affectedness or a
complete vulnerability audit.

## Local evidence

macOS ARM64, Rust `pdfium-render = 0.9.1`, Slint 1.19.0:

- `cargo fmt --check`, `cargo check --locked --all-targets`, and debug build pass.
- `cargo test --locked`: 504 tests pass separately with 8111 and 8165; no
  ignored or filtered tests in the run summaries.
- Python fetch/upgrade decision tests: 25 pass, including interruption handling.
- Nine unencrypted PDFs / 28 pages, including all repository PDF fixtures and
  the README demo: page counts, full text, speaker notes, and dimensions match.
- 56 page/width comparisons at widths 640 and 1920. Repeated renders,
  reverse navigation, clean shutdown, close/reopen in a fresh helper,
  out-of-range requests, and subsequent valid renders pass.
- Truncated and password-protected PDFs are rejected. A missing native-library
  override fails, establishing that the executable honors explicit selection.
- No host latency threshold breach in the initial comparison.
- Public C headers, `args.gn`, the top-level license and third-party license
  inventories are byte-identical between the macOS ARM64 archives.

The initial strict comparison returned REVIEW for 18 demo renders, with all
other renders identical. Quantitative comparison of every changed image found
maximum channel delta **1/255** and changed pixel fraction below **0.1%** per
image. Side-by-side old/new and amplified-difference images for pages 1, 3, 6,
and 12 show no visible content or layout regression. The differences follow
gradient regions and are consistent with the upstream axial shading change;
this attribution is an inference, not a bisect result.

An exact-evidence review record in `scripts/pdfium_upgrade_reviews.json`
accepts this macOS ARM64 rounding drift. It binds the complete manifests,
native-library hashes, input hashes, licenses, flags and output hashes.
Other platforms need their own results; no fuzzy global tolerance was added.

Raw local evidence is under `artifacts/pdfium-upgrade/` (ignored by Git).
The reproducible command and decision policy are in
[PDFium Upgrade Validation](../PDFIUM_UPGRADE_VALIDATION.md).

## Hosted validation

Initial [compatibility matrix](https://github.com/koichiro/quick-presenter/actions/runs/44092577742):

- macOS 26.7.2 ARM64: PASS with the exact same reviewed evidence fingerprint
  as the local run.
- Linux x64: 495 Rust tests pass with each native library. The strict comparison
  stops at REVIEW for the same 18 changed demo renders, with maximum channel
  delta 1/255 and changed pixel fraction below 0.1%. The baseline and candidate
  demo images respectively match the reviewed macOS output byte for byte.
  A Linux-specific exact-evidence review record has been added; the workflow
  must evaluate that record before main acceptance. Two PowerPoint renders
  differ between operating systems but remain identical between versions on
  Linux; these are not upgrade regressions.
- Windows x64: 476 Rust tests pass with each native library. The strict
  comparison also stops at REVIEW for exactly the same 18 changed demo renders.
  Maximum channel delta is 1/255 and changed pixel fraction is below 0.1%; both
  old and new demo image hashes match the reviewed macOS output. A separate
  Windows evidence fingerprint accepts these exact differences. Text, notes,
  dimensions, repeat/reverse/reopen, error handling and timing gates pass.
- The [ordinary Linux CI](https://github.com/koichiro/quick-presenter/actions/runs/44092577593)
  passes (495 Rust tests) with the candidate.
- [macOS audience regression](https://github.com/koichiro/quick-presenter/actions/runs/44092577789)
  passes all 180 checks, including native fullscreen transitions, rendered
  window edges, restored geometry and note layout.
- The Linux job in [Build Binaries](https://github.com/koichiro/quick-presenter/actions/runs/44092577558)
  passes renderer process-boundary tests, control/CLI tests, Debian package
  verification, staged-artifact PDF playback, sandbox denial checks and SBOM
  generation/verification.
- All three platform jobs in that Build Binaries run succeed. Windows includes
  MSI/MSIX validation and installed MSI and Store-identity MSIX renderer sandbox
  verification. macOS includes native document bridge and process-boundary
  tests, app/DMG validation and optional Store control compilation; this does
  not establish a trusted signed-XPC runtime validation.

The runner was subsequently refined to hash the executable after `cargo test`,
which can replace an earlier dev-profile binary, and to require the same hash
across both native runs. The refined runner passes locally with the same
reviewed image evidence. It also avoids a redundant separate dev-profile build.

## Acceptance boundary

No blocking compatibility regression was found in the three host comparisons,
ordinary CI, GUI regression or package checks. Main acceptance remains conditional on the dedicated macOS,
Linux and Windows compatibility matrix and ordinary PR CI. Signed macOS XPC,
Windows MSIX/AppContainer and Linux packaged sandbox/release smoke validation
remain separate release gates. No production release or main merge is claimed
by the local comparison. Secondary asset architectures have not been runtime
validated in this environment.

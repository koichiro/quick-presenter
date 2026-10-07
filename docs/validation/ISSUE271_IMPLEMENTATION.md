# Issue #271 implementation validation

This implements the sizing and cache policy in [the design](ISSUE271_DISPLAY_AWARE_RENDERING.md). The implementation PR is stacked on design PR #398.

The package version is now 1.4.0 for internal releases before v1.5.0. The signed artifacts and manual observations recorded below predate this version adjustment and reported 1.5.0; their hashes and results are historical evidence. After the version change, `cargo fmt --check`, `cargo check`, and `cargo test --quiet` passed again (398 unit tests and 13 integration tests). App metadata and packaging scripts derive their version from Cargo.toml.

## Behavior

- Rust samples the visible Slint slide window's physical surface width every 250 ms. Two consecutive samples in the same bucket select a new policy; hidden and zero-size surfaces do not select one. Slint's physical size is not multiplied by its scale factor again.
- Current slides retain the 1600 px baseline, round larger widths upward in 128 px buckets, and cap at 2560 px. Previews remain 600 px and thumbnails 180 px.
- Unknown page geometry renders at baseline first. A larger bitmap is requested only when its dimensions fit the renderer limits, with two height pixels reserved for pixel-derived aspect-ratio rounding. Tall pages retain their baseline image.
- Cache budgets keep 64 entries and use the design's working-set formula, bounded to 96–192 MiB. The 2560 px policy selects 151,554,600 bytes (about 144.5 MiB). Width changes immediately enforce the new budget and make obsolete widths evictable.
- Both windows retain the last successful slide while replacements render. Only the current page's exact desired width can replace that fallback or report a visible failure. Queued requests for obsolete widths are coalesced. Reload preparation records its actual bitmap width before the adaptive upgrade.

## Automated validation

The final source passed:

```sh
cargo fmt --check
cargo check
cargo test --quiet
cargo build
scripts/check_windows_cross.sh
```

The Rust suite passed 398 unit tests and 13 integration tests. Added cases cover width buckets, stable samples, tall geometry, bounded budgets, stale completions and failures, cache eviction, queue coalescing, navigation, and reload width identity. The Windows check compiles all targets; it is not a Windows runtime qualification. The existing macOS atomic `fetch_update` deprecation warning remains.

The final debug build was staged and Developer ID signed using the repository scripts. The signed XPC render/notes and private-file/network/child denial gate passed. Its GUI smoke test passed **39 checks with zero failures**; [the raw report](ISSUE271_IMPLEMENTATION_GUI_SMOKE.txt) is retained. This smoke entry point checks the existing GUI flow and does not start the new adaptive sizing timer.

Final signed executable SHA-256:

```text
f430f874fe5cff4f6d2add184cdfd877f9708b871cac4eac9a494184331250a2
```

## Manual adaptive rendering validation

Environment: macOS 26.7 (25G229), Apple M4 Pro, DELL U4021QW at 5120×2160 / 60 Hz, scale 1, main display, mirroring off. Tests used the normal application entry point and signed helper, rather than the GUI smoke entry point.

- Opening the sample deck rendered the first page at 1600 px. Entering fullscreen sampled 5120 physical pixels, selected 2560 px, and committed the larger bitmap. Audience output remained correctly letterboxed.
- Forward/backward navigation and fullscreen exit preserved the page state and visible output. The 1024 px window selected 1600 px and restored the 96 MiB cache budget. Re-entering fullscreen selected 2560 px again.
- Replacing the open PDF during fullscreen retained page index 1 and loaded the replacement's speaker notes. The audience displayed the replacement's second page; the log recorded its successful 2560 px bitmap.
- A separate fresh launch opened a two-page 200×400 pt portrait PDF. Its 1600×3200 baseline image rendered successfully. Fullscreen selected the 2560 px policy while page geometry kept the effective bitmap at baseline; navigation to page 2 and audience output remained usable.

Raw adaptive events are retained in [the navigation/reload log](ISSUE271_IMPLEMENTATION_ADAPTIVE.txt) and [the fresh portrait launch log](ISSUE271_IMPLEMENTATION_TALL.txt). Cache values in these logs measure retained RGBA images, not process RSS or all copies of a bitmap.

These manual tests used the signed candidate with SHA-256 `568f88be47499302b8b1465d5c18daeca09ac1329f98e8d909ce4ff83ab41e2e`. Afterwards, the final source tightened failure-status recovery when an effective visible request changes or succeeds. That error-path adjustment passed the complete Rust suite and final signed GUI smoke; the manual successful rendering sequence was not repeated on the final artifact.

## Limits and observations

- After closing/hiding the slide window, replacing the PDF, and raising the slide through the native window menu, one attempt showed the old backing image while the presenter showed the new document. Hidden-window recovery is not qualified by this validation and is tracked in [#400](https://github.com/koichiro/quick-presenter/issues/400) as a v1.5.0 release blocker pending reproduction and resolution or an explicit release decision. The portrait test above was repeated in a fresh launch with a visible slide window and passed.
- Mixed-DPI display movement, monitor disconnect/reconnect, and Windows/Linux runtime behavior remain unverified. The scale-1 external display does not qualify Retina behavior.
- No sustained latency, process-RSS, or long-running presentation measurements were taken. Coverage was not requested and was not run.
- User recent-document and startup settings were backed up before manual testing and restored afterwards.

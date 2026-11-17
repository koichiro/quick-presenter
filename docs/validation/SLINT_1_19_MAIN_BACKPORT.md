# Slint 1.19.0 backport to main

## Source and scope

This backport starts from main `3d73ea2` and adapts the source changes in
[PR #548](https://github.com/koichiro/quick-presenter/pull/548), merged into
v3.0.0 as `af4157f2c8fc1b9dd6c8aa00ed5703453dce307e`.
The application version remains 2.5.0; this is a source update, not a release.

- Pin both Slint and slint-build to 1.19.0. Compile the normal and generated
  Linux UI with Slint edition 1.18. Rust 1.95 or later is required by Slint.
- Remove the complete Slint 1.18.1 backend override and its private-module
  integration test. Registry 1.19.0 includes stale native menu rejection
  (Slint #14331), macOS hidden-window frame reset (#14357), early native icon
  assignment (#14359), and one-time Linux activation-token consumption (#14360).
- Preserve the independent local winit 0.30.13 override, including private
  CoreGraphics API removal, fullscreen input/intent handling, and sandboxed
  file-URL delivery. PDFium's pin and ownership boundary are unchanged.
- Use native About and Quit roles. Both Presenter and Slide windows route
  Quit through the same recoverable Rust event-loop exit as presenter close,
  preserving post-loop cleanup and startup-state persistence. Slide About
  reveals the presenter. The software-platform test covers both Quit callbacks,
  presenter close, failures/retry, and About forwarding.
- Make About license text content-sized inside a scrollable body, with an
  independent Close footer.
- Fit the Windows presenter after native window creation to the monitor work
  area using physical coordinates. Preserve main's 800x560 logical minimum;
  do not shrink controls below that minimum if the monitor cannot accommodate it.
- Start GUI resize settling after the initial layout sample. Select the
  intermediate notes fixture from real text measurements and retain strict
  largest-fitting-size, native grow/shrink, and presentation-state assertions.

Main has neither Audience Live nor Settings. This backport retains its existing
Slide Window terminology and excludes v3-only comment measurement, audience
panel/disclosure geometry, Settings callbacks, relay/browser dependencies, and
their tests. The dependency lock starts from #548's reviewed versions, uses
main's application dependencies, and is pruned by Cargo; it does not introduce
v3 application features. Earlier security/dependency updates on main remain.

## Linux activation token timing

The former patch consumed `XDG_ACTIVATION_TOKEN` and `DESKTOP_STARTUP_ID` during
backend initialization. Upstream 1.19.0 consumes them when the first native
window is created. A startup PDF can launch the renderer before deferred Linux
window creation. The existing renderer environment allowlist excludes both
tokens; the environment test now explicitly verifies this. No additional
process-global environment mutation is introduced. The deleted integration test
included the old private vendored module and cannot test the registry backend.

## Validation

The backport PR's final head and check runs are authoritative for this branch.
Required checks are `cargo fmt --check`, `cargo check --all-targets --locked`,
`cargo test --locked`, the macOS Store-feature build check, the SBOM/CLI
packaging tests, current-head CI/package builds, and CodeRabbit review.
Local execution uses native macOS arm64 and pinned PDFium 8111.
Results are recorded in the backport PR rather than relabeling #548's results
as tests of this different source.

The upstream cross-OS records remain attached to #548 and its v3.0.0 closeout
document. They provide provenance, not fresh native Windows/Linux results for
main. CI's native package tests and macOS GUI regression provide backport
coverage. Multi-monitor/mixed-DPI hardware, native Xorg, signed/Store installed
distribution, and public-release qualification remain separate work.

# macOS fullscreen input investigation — 2026-10-21

Tracking: #491. Baseline: main `430cef0` (application 2.5.0), macOS arm64.
This work does not alter v2.0.0 or its accepted release limitation.

## Confirmed boundary

A temporary instrumented debug build logged NSApplication key-down/up delivery
and winit KeyboardInput delivery separately. With the presenter image focused:

- F followed immediately by Right: AppKit logged F and Right, but winit logged
  only F pressed. The matching isolated `qp status --json` remained at page 1.
- Escape followed immediately by Right: AppKit logged both; winit logged only
  Escape pressed. The page remained unchanged.

These observations use native injected input, not physical keyboard input.
They locate the observed loss between NSApplication and winit, separately from
#479's focused-widget interception. Tool timestamps are not animation timings.

## Implementation

The existing vendored winit sendEvent override forwards non-Command key-down/up
once to the current WinitWindow during native fullscreen transitions and returns
without calling the original application dispatcher. Ordinary first-responder
handling is retained. No input is queued or replayed. Normal dispatch resumes
when all transitions finish, fail, or their windows are destroyed.

The first forwarding prototype delivered immediate navigation and Escape, but a
slide-window rapid-reversal check exposed an additional synchronization defect:
logical fullscreen became false while the native window remained fullscreen.
Intermediate native size notifications could report the old fullscreen state to
Slint and overwrite the pending reversal. The final getter therefore reports the
latest queued target until winit applies it.

## Validation

- `cargo fmt --check`: passed.
- `cargo check`: passed; existing macos_renderer.rs fetch_update deprecation warning.
- `cargo test` with the documented PDFium development path: passed, including
  three pure regression tests for transition lifecycle, overlapping windows,
  destruction/failure cleanup, and queued reversal reporting. The initial
  restricted run could not create an IPC socket; the complete run used normal
  local IPC permissions.
- Release build: passed.
- Developer ID signed release-layout app, bundled PDFium 156.0.8076.0: signed XPC
  PDF render/notes and unrelated-file/network/child denial gate passed.
  Signing timestamp was disabled for this local validation artifact. This is not
  a notarized DMG or a published v2.5.0 release.
- Forwarding prototype native checks: presenter and slide entry/exit Right
  delivered exactly one page change. Presenter rapid Escape returned logical
  state to windowed. The later slide test found the synchronization defect above;
  do not treat the prototype as fully qualified.
- Final native entry/exit/reversal and toolbar/notes interaction: pending; the
  macOS desktop locked during the final signed-app validation.
- Physical rapid-key reproduction and final distributed-artifact qualification:
  pending. The issue remains open until its remaining acceptance checks are met.

# macOS fullscreen input investigation — 2026-10-21

Tracking: #491. Baseline: main `430cef0` (application 2.5.0), macOS 27.0.1 arm64.
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
- Signed candidate GUI binary SHA-256:
  `8e595e76fb10167a584a3f424e33ee7bc2179ac6a42ba6ee6f19489e3fc14372`.
- Signed bundled libpdfium SHA-256:
  `04c27b9d25a8537b977c028f687aafdff840f66d4558ca8da3061cfde872d8f5`.
- GitHub Actions macOS audience-window regression on PR #504: passed.
- Forwarding prototype native checks: presenter and slide entry/exit Right
  delivered exactly one page change. Presenter rapid Escape returned logical
  state to windowed. The later slide test found the synchronization defect above;
  do not treat the prototype as fully qualified.
- Final native entry/exit/reversal and toolbar/notes interaction: pending; the
  macOS desktop locked during the final signed-app validation.
- Physical rapid-key reproduction and final distributed-artifact qualification:
  pending. The issue remains open until its remaining acceptance checks are met.

## Follow-up after unlocking the desktop

The 14484d4 signed candidate still left the real slide window fullscreen after
rapid F/Escape, despite `qp status` reporting false. The intermediate getter fix
alone was insufficient. Native instrumentation found that AppKit ignored a
reverse toggle called synchronously from `windowDidEnterFullScreen:`.

The follow-up records fullscreen intent in winit immediately (before Slint's
window properties are deferred), and applies a queued reversal on the next
AppKit run-loop turn after completion. Requests arriving before that deferred
application replace the queued target. Application state borrows end before
native calls, which may deliver resize notifications synchronously.

On macOS 27.0.1 arm64, a Developer ID signed instrumented copy using a unique
UI bundle identifier (same renderer/proxy IDs) verified:

- Presenter F/Right: page 1 → 2, once; Escape/Right: 2 → 3, once.
- Slide F/Right: 3 → 4; Escape/Right: 4 → 5, once each.
- Presenter and slide F/Escape: real AppKit did-enter followed by did-exit
  notifications, logical fullscreen false, and windowed slide controls restored.
- Slide F/Escape/F: last intent true, no delayed exit and no page change.
- Normal Escape after settling exited fullscreen and restored window controls.

The final sources remove instrumentation and add GUI smoke checks against
AppKit's real FullScreen style bit (entry cancellation, settled entry, exit
cancellation and cleanup), rather than trusting the queued winit getter.
`cargo fmt --check`, `cargo check`, the complete Cargo suite and release build
passed. The clean signed release-layout candidate passed the XPC renderer gate.

GUI smoke qualification is still incomplete: a software-backend run aborted on
a 1024-pixel buffer versus 1200x1000 window-size mismatch; a default-backend run
reported 65 passes and two failures (logical toggle and rapid cancellation)
while other native automation was active. An isolated retry after the desktop
locked reported 67 passes, including rapid entry cancellation, then failed to
reach native settled fullscreen. Preserve these failures; neither run is a clean
GUI pass. Repeat on an unlocked, uninterrupted desktop and check the PR's macOS
CI result before removing the remaining qualification gate. Physical-key and
final notarized/distributed-artifact checks remain outstanding.

### Same-turn request/notification gap found in CI

The isolated CI check on run 39574602040 passed its windowed baseline but failed
rapid entry cancellation with AppKit=true, Slint=false and winit intent=false.
This ruled out hidden-window-test contamination as the complete explanation.
The same-turn true/false sequence can precede AppKit's will-enter notification.
Mark a native transition before calling toggleFullScreen, rather than only in
will-enter/will-exit, so the second request is queued across that gap. The
existing lifecycle guard makes the later notification idempotent and clears on
completion/failure/destruction. The full Cargo suite and release build passed
with this additional fix; repeat the actual-style regression before declaring
native qualification complete.

The software-renderer buffer mismatch was independently reproduced in the
14484d4 signed baseline without these follow-up changes, with the same
737280-byte/1024-pixel buffer and 1200x1000 size diagnostic.

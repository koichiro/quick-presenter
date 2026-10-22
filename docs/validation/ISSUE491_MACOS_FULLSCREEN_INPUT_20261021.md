# macOS fullscreen input validation — 2026-10-21/22

Tracking: #491, PR #504. Baseline: main `430cef0` (application 2.5.0).
Native investigation and local qualification: macOS 27.0.1 arm64.
The published v2.0.0 tag and its accepted release limitation are unchanged.

## Confirmed cause and implementation

Native injected F/Right and Escape/Right reached NSApplication during a native
fullscreen animation, but Right never became a winit KeyboardInput event and
the page did not advance. This boundary is separate from #479's focused-widget
handling; physical-key reproduction was not claimed.

The vendored winit sendEvent override forwards non-Command key-down/up once to
the current WinitWindow during tracked fullscreen transitions, preserving its
first-responder handling. It returns without calling the original application
dispatcher again. No input is queued or replayed. A lifecycle guard restores
normal delivery after overlapping transitions finish, fail, or are destroyed.

The forwarding prototype (`14484d4`) still left the real slide window fullscreen
after rapid F/Escape while logical state reported false. Instrumentation found
that AppKit could ignore a reverse toggle called synchronously inside
windowDidEnterFullScreen. The final implementation records native intent before
Slint defers property synchronization, reports the latest pending target through
intermediate resize notifications, and applies pending reversals on the next
AppKit run-loop turn after completion. Newer requests replace the pending target.
AppState RefCell borrows end before native calls that may deliver resize events.
Temporary instrumentation has been removed.

## Native input results

A Developer ID signed instrumented copy ran in a continuous application loop:

- Presenter F/Right: page 1 → 2, once; Escape/Right: 2 → 3, once.
- Slide F/Right: 3 → 4; Escape/Right: 4 → 5, once each.
- Presenter and slide F/Escape: actual did-enter followed by did-exit,
  logical fullscreen false, and windowed slide controls restored.
- Slide F/Escape/F: final intent true, no delayed exit or page movement.
- Normal Escape after settling restored windowed controls.

These are native injected-input results, not physical-key or exact timing claims.
The instrumented UI used a unique bundle identifier with unchanged renderer/proxy
identifiers. No shared root cause with #479 is assumed; toolbar/notes validation
remains separate.

## Review corrections

CodeRabbit correctly identified two gaps in the original 178-check result:
a dropped initial request could satisfy a reversal's final-state assertion,
and exiting the reversal loop closed native windows used by later checks.
The original result did not establish those missing properties.

The complete GUI suite now runs in one event loop, including asynchronous PDF
opening, note-layout settling, native window actions, and the IPC smoke worker.
Only suite completion exits that loop. Native state helpers reject windows
without a delegate. Unrelated-timer and deadline checks remain within the loop.

A window-scoped NSNotificationCenter observer leaves Winit's delegate intact.
Each reversal requires the initial will-enter/will-exit notification before
sending the opposite request, rejects a reverse request sent after completion,
and requires both native directions' will/did notifications plus the original
AppKit/Slint/winit final-state assertions. A dropped or failed first transition
cannot satisfy the revised test.

Keeping real native windows also exposed CI fixtures that exceeded its small
desktop: the first corrected CI run passed the fullscreen cases but failed two
note-resize and twelve edge checks after native size clamping. Fixtures now use
actual presenter sizes with constant width, scale edge-test windows within the
display, and settle fullscreen before hidden-window actions. Assertions remain
about actual notes growth and slide edge pixels; none were skipped or relaxed.

## Revised qualification results — 2026-10-22

- `cargo fmt --check`, `cargo check --locked`, complete `cargo test --locked`,
  and release build: passed. Check retains the existing macos_renderer.rs
  fetch_update deprecation warning. A stale PID-based test directory caused one
  initial AlreadyExists failure; the complete suite passed with a fresh TMPDIR.
- Three pure regression tests cover transition lifecycle, overlapping windows,
  destruction/failure cleanup, and queued reversal reporting.
- Clean Developer ID signed release-layout app: **178 GUI checks passed, zero
  failed**, including actual native start/completion notifications for entry
  cancellation, settled entry, exit cancellation, and cleanup. Later titlebar,
  geometry, restoration, notes, and IPC checks retained live native windows.
- Signed XPC PDF render/notes and private-file/network/child denial gate: passed.
- Revised signed GUI executable SHA-256:
  `712fc8f24e1020ade42c8fde6cecf1ba4f9413a4d088018e62a80c831f763674`.
  Bundled PDFium: 156.0.8076.0. Local signing used timestamp=none; this is not
  a notarized or published artifact. Raw smoke output is not committed.

Earlier local failures are not reclassified as passes: locked-desktop runs
failed native fullscreen entry, an unbundled debug run started but did not
complete entry, and a software-renderer buffer/window-size mismatch reproduced
in the pre-follow-up baseline. The clean signed result above is the revised
qualification run.

## Issue closure and release qualification

Per the maintainer's 2026-10-22 instruction, PR #504 closes #491 on merge once
the review fixes and corrected regression validation are accepted. Physical
rapid-key checks and final notarized/distributed-artifact checks remain release
qualification work, not additional code-fix closure conditions. They have not
been reported as completed.

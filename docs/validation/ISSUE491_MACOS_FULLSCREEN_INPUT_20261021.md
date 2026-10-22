# macOS fullscreen input investigation — 2026-10-21

Tracking: #491. Baseline: main `430cef0` (application 2.5.0), macOS 27.0.1 arm64.
This does not alter v2.0.0 or its accepted release limitation.

## Confirmed defects and implementation

Native injected F/Right and Escape/Right reached NSApplication during a native
fullscreen animation, but Right never became a winit KeyboardInput event and
the page did not advance. This boundary is separate from #479's focused-widget
handling. Physical keyboard input was not tested.

The vendored winit sendEvent override forwards non-Command key-down/up once to
the current WinitWindow during tracked fullscreen transitions, preserving its
first-responder handling. It returns without invoking the original application
dispatcher again. No input is queued or replayed. A lifecycle guard restores
normal delivery after overlapping transitions finish, fail, or are destroyed.

A signed forwarding prototype (`14484d4`) still left the real slide window
fullscreen after rapid F/Escape while logical state reported false. Native
instrumentation established that AppKit can ignore a reverse toggle called
synchronously inside windowDidEnterFullScreen. The final implementation:

- Records native fullscreen intent before Slint defers window-property updates.
- Reports the latest queued target through intermediate resize notifications.
- Applies pending reversals on the next AppKit run-loop turn after completion;
  newer requests replace the pending target before it is applied.
- Releases AppState RefCell borrows before native calls that may synchronously
  deliver resize notifications.

## Native evidence

A Developer ID signed instrumented copy, with a unique UI bundle identifier
and the same renderer/proxy identifiers, ran in a continuous application loop:

- Presenter F/Right: page 1 → 2, once; Escape/Right: 2 → 3, once.
- Slide F/Right: 3 → 4; Escape/Right: 4 → 5, once each.
- Presenter and slide F/Escape: actual did-enter followed by did-exit,
  logical fullscreen false, and windowed slide controls restored.
- Slide F/Escape/F: final intent true, no delayed exit or page movement.
- Normal Escape after settling restored windowed controls.

The final sources remove temporary instrumentation.

## Automated validation

- `cargo fmt --check`, `cargo check`, complete `cargo test`, and release build:
  passed. Check retains the existing macos_renderer.rs fetch_update deprecation
  warning. Tests used the documented development PDFium override and normal
  local IPC permissions after a restricted run could not create its socket.
- Three pure regression tests cover transition lifecycle, overlapping windows,
  destruction/failure cleanup, and queued reversal reporting.
- macOS CI [run 39577626888](https://github.com/koichiro/quick-presenter/actions/runs/39577626888)
  at PR head `dc24721`: **178 GUI checks passed, zero failed**. New checks compare
  AppKit's real FullScreen style bit, Slint state, and winit intent for a windowed
  baseline, rapid entry cancellation, settled entry, rapid exit cancellation,
  and cleanup. Rust verification and dependency advisory audit also passed.
- The clean Developer ID signed release-layout candidate, bundled PDFium
  156.0.8076.0, passed XPC PDF render/notes and unrelated-file/network/child denial
  gates. The implementation also passed the private-CGS import check.
  Local signing used timestamp=none; this is not a notarized release artifact.
- Clean live-loop candidate GUI SHA-256:
  `3b27bff04f37e2fde791039d969677ccbed9734c71a09674ef3b2b29eb98fcdb`.

### Test-harness correction and unsuccessful local runs

The legacy GUI smoke quits and restarts its event loop between checks. Winit's
macOS run_app_on_demand closes native windows on exit, and windowWillClose clears
the delegate. Retained handles can still expose geometry but cannot qualify
later native transition callbacks. Earlier failures using those handles did
not establish a request/notification gap; the speculative pre-notification
transition marking was removed.

The new regression runs on fresh adapters before the first event-loop exit,
advances all cases in one uninterrupted loop, and quits only on completion or
failure. The manual evidence above also used a continuous loop.

Preserve these local limitations separately from the passing CI result:

- Software-renderer smoke aborted on a 737280-byte/1024-pixel buffer versus
  1200x1000 window-size mismatch. The same error reproduced in the `14484d4`
  baseline without the follow-up implementation.
- Earlier default-renderer checks were contaminated by closed native handles
  or concurrent automation and were not clean qualification runs.
- The clean live-loop signed app passed baseline and entry cancellation, then
  failed settled entry. Native trace showed windowDidFailToEnterFullScreen.
  Computer-use inventory confirmed the desktop was locked. A retry after the
  user's unlock reply failed similarly, and inventory again reported locked.
  These are failures, not a local GUI pass; no production workaround is inferred.

## Unlocked signed-app qualification

After the user unlocked the desktop again, the same clean signed live-loop
candidate (SHA-256 above) completed **178 checks, zero failures** on
2026-10-21 at approximately 14:32 UTC. All five real AppKit fullscreen
checks passed: windowed baseline, rapid entry cancellation, settled entry,
rapid exit cancellation, and cleanup.

PR #504 is ready for code review. Physical rapid-key checks and final
notarized/distributed-artifact qualification remain outstanding; keep #491 open
until those release acceptance checks are satisfied. Earlier locked-desktop
failures remain documented above rather than being reclassified as passes.

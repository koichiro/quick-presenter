# Slow PDF Open Behavior

## Problem

The v1.0.0 decision below records the original thread-only design. Since #372,
native open/render runs in a separate helper. A replacement is committed only
after open and initial-page render succeed; shutdown can terminate and reap a
blocked helper. Normal broker dispatch still waits synchronously for IPC, so
newer opens remain coalesced until that exchange finishes or its deadline expires.
Since #373, handshake is capped at 5 seconds, open at 30 seconds, and initial
visible render at 5 seconds. Candidate timeout preserves the committed deck;
only active-document crash/timeout is eligible for bounded automatic recovery.
See [Render Scheduling](RENDER_SCHEDULING.md). The presenter-facing delayed status
and last-good-deck behavior below remain in effect.

PDF opens run on the single render worker because loaded `PdfDocumentState`
must stay worker-local. `PdfDocumentState::open()` is a synchronous PDFium
boundary. While that call is running, the worker cannot process newer `Open`,
render, speaker-note, or `Shutdown` commands.

Quick Presenter therefore must not promise that a slow PDF open can be
interrupted immediately. The v1.0.0 goal is deterministic presenter-facing
behavior while preserving the currently committed deck until a replacement PDF
has opened successfully.

## v1.0.0 Decision

Do not implement a hard open timeout for v1.0.0.

A hard timeout would require either forcefully interrupting PDFium or abandoning
a blocked worker and starting another one. Forceful interruption is not exposed
by the current PDFium integration, and abandoned workers would violate the
current single-worker ownership model by allowing more than one worker to be
inside PDFium at the same time. Until a broader PDFium threading and process
isolation design exists, the app should keep one render worker and use
cooperative cancellation only at boundaries where the worker regains control.

Instead, v1.0.0 implements a delayed slow-open status message and keeps the
latest selected PDF as the deterministic winner.

This is an explicit in-process release tradeoff. It does not protect the app
from a native PDFium crash or provide a security sandbox. The approved
helper-process timeout and recovery direction is defined in
[PDF Rendering Security and Isolation Policy](PDF_RENDERING_SECURITY.md); it
becomes the runtime contract only after #372 and #373 are implemented and
validated.

## User-Facing Behavior

Normal open path:

- When a PDF is selected, the presenter status changes to `Opening PDF...`.
- If no deck is currently committed, the windows show the opening placeholder
  for the selected filename.
- If a deck is already committed, the current slide, notes, page label,
  thumbnails, and audience slide remain visible while the new PDF opens.
- When the new PDF opens and its initial render succeeds, it replaces the current
  deck atomically.
- If the new PDF fails to open, the current committed deck remains available and
  the presenter status changes to `Could not open PDF. Choose another file.`

Slow open path:

- If the pending open has not completed after a short delay, the presenter
  status changes to `Still opening PDF. The current deck remains available.`
- The delay should be long enough to avoid flicker on normal PDFs. Use a named
  constant, for example `SLOW_OPEN_STATUS_DELAY`, rather than embedding the
  duration in UI code.
- The delayed status is informational only. It does not cancel the open and does
  not change committed presentation state.
- If there is no current deck, the same delayed status is shown while the
  opening placeholder remains visible.

Repeated opens:

- Selecting another PDF while an open is pending starts a new pending open
  session and replaces `pending_open`.
- The newest selected PDF is the only pending open that UI state may commit.
- If an older open succeeds or fails after it was superseded, its event is
  ignored by `RenderSessionTracker`.
- After the worker returns from the slow synchronous open, the command mailbox
  coalesces pending `Open` commands so the worker opens the newest selected PDF
  next.
- The slow-open status timer is reset for the newest pending session, so stale
  timers cannot overwrite the status for a newer open.

Shutdown:

- Shutdown remains cooperative. If the worker is inside PDFium, shutdown is
  requested and completes only after `PdfDocumentState::open()` returns.
- The app must not replace a `Running` or `ShutdownRequested` scheduler while
  the worker may still be inside PDFium.

## State Design

Keep the PDFium-owning document state in `src/render_scheduler.rs`. `AppState`
stores only UI progress for the pending open:

```rust
pub struct PendingOpenState {
    pub session_id: RenderSessionId,
    pub path: PathBuf,
    pub requested_at: Instant,
    pub slow_status_shown: bool,
}
```

This state is updated only through `session_controller` helpers so tests can
cover transitions without constructing Slint windows.

The helper shape is:

- `begin_open_pdf_state_at(state, path, now)` creates a new session, stores
  pending open progress, sets status to `Opening PDF...`, and preserves the
  committed deck.
- `mark_pending_open_slow(state, session_id, now, delay)` updates status only
  when the matching pending session is still open, the delay has elapsed, and
  the slow status was not already shown.
- `commit_render_opened_state(...)` commits only the matching pending session,
  clears pending open progress, and replaces the deck.
- `commit_render_open_failed_state(...)` clears only the matching pending
  session and leaves any committed deck intact.
- `commit_render_worker_failed_state(...)` clears matching pending state when
  the worker failure applies to the pending or committed session.

## Timer Design

Use a UI-thread `slint::Timer` for slow-open status because the render worker may
be blocked and cannot emit progress events. The implementation uses a repeated
timer, similar to the render-event and clock timers, that checks pending open
progress every 250 ms.

When `mark_pending_open_slow` returns true, sync only the presenter status:

- use `set_presenter_message(...)` or an equivalent small helper;
- do not replace the slide image;
- do not clear thumbnails, notes, page labels, or cached pages for an existing
  committed deck.

## Render Scheduler Contract

No new render command is required for v1.0.0.

The existing command mailbox behavior is the desired contract:

- `Open` clears pending render work.
- `Open` replaces earlier pending non-shutdown control commands.
- A worker already inside `PdfDocumentState::open()` cannot see the newer
  command until the synchronous call returns.
- Once the worker can drain commands again, only the newest pending `Open`
  remains.

Document this limitation near `open_document_on_worker()` or in
`docs/RENDER_SCHEDULING.md` so future maintainers do not infer that session IDs
provide immediate cancellation inside PDFium.

## Test Plan

Unit tests in `src/session_controller.rs` cover:

- starting an open records the pending session, path, request time, and
  `Opening PDF...` without clearing the current deck;
- slow status does not appear before the delay;
- slow status appears once after the delay for the matching pending session;
- repeated open replaces pending path/session and resets slow status timing;
- stale slow-status attempts do not overwrite the newest pending open;
- stale opened and failed events still do not replace or clear the newest
  pending session;
- failed replacement open preserves the committed deck.

Unit tests in `src/render_scheduler.rs` cover:

- pending `Open` commands still coalesce so the newest pending open wins;
- a comment or test name makes clear that an in-progress synchronous open is not
  interrupted until the worker regains control.

The timer glue stays thin in `src/main.rs`; behavior is tested through the pure
session-controller helpers rather than by constructing Slint windows.

## Future Helper Process Behavior

The selected direction for a real open timeout is a supervised helper process
that the broker can terminate and reap without cancelling a thread inside
PDFium. #371 defines its IPC,
#372 moves production PDFium work into the helper, #373 implements deadlines and
bounded recovery, and #374 through #376 add platform sandboxing. Until those
issues ship, the v1.0.0 behavior above remains authoritative.

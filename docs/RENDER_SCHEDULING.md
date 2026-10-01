# Render Scheduling

Quick Presenter keeps PDFium document access on one render worker. Rendering is
scheduled in two stages so the UI can remain responsive under backlog while the
worker keeps a stable, deduplicated view of pending work.

## Two-stage model

The UI thread sends `RenderCommand` values through the command mailbox. The
mailbox has a bounded work backlog and may coalesce or drop render work before
the worker receives it. This protects presentation input and window updates from
unbounded thumbnail or preload bursts.

The render worker then places render work into `RenderQueue`. The worker queue is
the final execution order for PDFium access. It deduplicates requests again
because different UI paths can request the same page while the worker is busy.

Both stages use the same work policy:

- Work identity is `(session_id, RenderRequest)` for page renders and
  `(session_id)` for speaker-note extraction.
- Duplicate page work is not queued twice; the existing work keeps the highest
  requested priority.
- Duplicate speaker-note work is not queued twice.
- Execution order is `BlockingVisible`, `VisibleAux`, `Warm`, then `Background`.
- Work with the same priority runs in FIFO order.
- When the command mailbox work backlog is full, it drops the lowest-priority
  work. If priorities are equal, it drops the oldest work first.

## Priorities

`BlockingVisible` is for the currently visible slide. It should be the most
protected render work because it directly affects the speaker and audience view.

`VisibleAux` is for visible supporting material such as the next-slide preview.
It should run after the current slide but before preload and thumbnail work.

`Warm` is for presentation preloading around the current page. It improves page
turn latency but can be regenerated if it is dropped.

`Background` is for thumbnails and speaker-note extraction. Speaker-note
extraction is intentionally processed in batches and requeued as background work
so visible rendering can interrupt long documents between pages.

## Control commands

Control commands are not part of the bounded render-work capacity:

- `Shutdown` wins over all pending commands and clears pending work.
- After `Shutdown` is queued, later commands are ignored except another
  `Shutdown`.
- `PrepareReload` opens and renders a replacement candidate without changing the
  active document or session. A newer preparation replaces older pending
  prepare/discard commands.
- `CommitReload` activates a matching prepared candidate. It remains ordered
  ahead of a later `Open`, so the UI and worker cannot disagree after the UI has
  accepted the candidate.
- `DiscardReload` removes a matching stale candidate without affecting the
  active document.
- `Open` clears pending work and replaces pending control commands except
  `Shutdown` and an already queued `CommitReload`.

There is no standalone close command in the supported scheduler contract. The
app replaces decks with `Open` and stops the worker with `Shutdown`; a future
user-facing "Close PDF" action should define app-state cleanup before adding a
new scheduler command.

The worker repeats session cleanup when opening, committing, or shutting down
because commands may already have crossed the mailbox boundary. A prepared
reload owns at most one candidate document on the same worker, preserving the
single-threaded PDFium ownership model.

`Open` cancellation is cooperative. A newer `Open` replaces older pending open
commands in the mailbox, but it cannot interrupt a worker that is already inside
the synchronous `PdfDocumentState::open()` PDFium call. Slow-open presenter
behavior is defined in [Slow PDF Open Behavior](SLOW_OPEN_BEHAVIOR.md).

## Worker lifecycle

`RenderScheduler` owns the render worker thread handle and tracks its lifecycle
explicitly:

- `Running`: the worker may own a PDFium document and may be inside a PDFium
  call.
- `ShutdownRequested`: shutdown has been requested, but the worker has not
  necessarily finished. This state is not safe for replacement.
- `Stopped`: the worker exited after an intentional shutdown.
- `Failed`: the guarded worker caught a panic or unexpected return.

Quick Presenter may construct a replacement render worker only after the
previous scheduler is terminal (`Stopped` or `Failed`) and its thread handle has
finished. The replacement path attempts to join the finished worker before
installing a new scheduler. It must not create a new worker while the previous
worker is still `Running` or `ShutdownRequested`, because that worker may still
be inside a long-running PDFium call.

Shutdown and session changes use cooperative cancellation. The app does not try
to forcefully interrupt PDFium; if a PDFium call is slow, the old worker remains
the only worker until it reaches the cancellation boundary and exits.

## Render events

Rendered events are also bounded while the UI waits to drain them. Event delivery
coalesces by event identity:

- Open success and failure replace earlier open results for the same session.
- Reload prepare success and failure replace earlier reload results for the same
  session.
- Speaker-note results replace earlier speaker-note results for the same session.
- Page render success and failure replace earlier page results for the same
  `(session_id, RenderRequest)`.

When the event mailbox is full, it drops the least protected event. The most
protected events are worker failures, then open and reload results, speaker-note
results, current-slide results, next-preview results, and thumbnails. Thumbnail
events are the easiest to regenerate; worker failure and document-transition
results are needed to keep the presenter state understandable.

## Cache budgets

Render cache size limits, representative fixed-width memory estimates, and the
v1.0.0 budget decision are documented in [Render Cache Budgets](CACHE_BUDGETS.md).

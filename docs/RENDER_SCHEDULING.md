# Render Scheduling

The helper transport and broker adapter are documented in
[Renderer IPC Protocol](RENDERER_PROTOCOL.md). They preserve the scheduler's
request/session identities while moving all native calls out of the UI process.

Quick Presenter keeps scheduling on one broker worker. Rendering is
scheduled in two stages so the UI can remain responsive under backlog while the
worker keeps a stable, deduplicated view of pending work.

PDFium runs serially in helper processes, not in this worker. Process separation
contains native crashes but does not restrict a compromised helper's authority.
See [PDF Rendering Security and Isolation Policy](PDF_RENDERING_SECURITY.md).

## Two-stage model

The UI thread sends `RenderCommand` values through the command mailbox. The
mailbox has a bounded work backlog and may coalesce or drop render work before
the worker receives it. This protects presentation input and window updates from
unbounded thumbnail or preload bursts.

The render worker then places render work into `RenderQueue`. The worker queue is
the final execution order for PDFium access. It deduplicates requests again
because different UI paths can request the same page while the worker is busy.
Dispatch uses one synchronous, correlated IPC exchange at a time. Notes use
`NotesPage` so existing background batching yields between pages.

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
reload owns at most one candidate proxy alongside the active proxy. Each proxy
owns a separate helper; replacement opens validate page zero before emitting
`Opened`, and reload validates the requested/clamped current page before
`ReloadPrepared`. Failed candidates do not replace or terminate the active
helper. Commit/discard drops and reaps the obsolete helper.

`Open` cancellation is cooperative. A newer `Open` replaces older pending open
commands in the mailbox, but it cannot interrupt a worker that is already inside
the synchronous IPC exchange waiting for a helper's PDFium call. Slow-open presenter
behavior is defined in [Slow PDF Open Behavior](SLOW_OPEN_BEHAVIOR.md).

## Worker lifecycle

`RenderScheduler` owns the render worker thread handle and tracks its lifecycle
explicitly:

- `Running`: the worker owns remote proxies and may be waiting for helper IPC.
- `ShutdownRequested`: shutdown has been requested, but the worker has not
  necessarily finished. This state is not safe for replacement.
- `Stopped`: the worker exited after an intentional shutdown.
- `Failed`: the guarded worker caught a panic, unexpected return, or active
  helper transport failure (EOF, exit, or invalid protocol). Idle helpers are
  checked for exit at the mailbox's 250 ms wake boundary.

Quick Presenter may construct a replacement render worker only after the
previous scheduler is terminal (`Stopped` or `Failed`) and its thread handle has
finished. The replacement path attempts to join the finished worker before
installing a new scheduler. It must not create a new worker while the previous
worker is still `Running` or `ShutdownRequested`.

Session changes remain cooperative at IPC boundaries, now bounded by operation
deadlines. Scheduler shutdown first cancels work, then
kills and reaps all registered helpers without waiting for a protocol reply.
Registration after shutdown kills the newly spawned helper as well. Healthy
proxy drop allows a bounded graceful shutdown before killing/reaping its child.
Closing the parent control pipe independently exits
the helper, including while its native execution thread is blocked. No PDFium
thread is asynchronously cancelled inside the UI process.

## Helper deadlines and recovery (v1.5.0)

`renderer_limits::Operation` defines separate hard deadlines for the whole pipe
exchange, including blocked writes, partial replies, and payload transfer:

| Operation | Deadline |
| --- | --- |
| Handshake | 5 seconds |
| Open | 30 seconds |
| Visible render (`BlockingVisible`) | 5 seconds |
| Auxiliary render (preview, thumbnail, warm preload) | 10 seconds |
| Notes (one page, or the legacy whole-document request) | 5 seconds |
| Graceful shutdown | 1 second |

These are conservative initial product budgets, not performance guarantees.
Handshake, open, and initial render are separately budgeted; selecting a PDF
does not imply a single 30-second end-to-end deadline. A broker-owned watchdog
is independent of synchronous IO and kills/reaps the helper at expiry. Deadline
validation also rejects late replies if the watchdog thread was delayed. OS
scheduling and process-reaping latency can add overhead; shutdown never waits
for a PDFium call to finish or for a protocol acknowledgment from a hung child.

Only committed/activated proxies may automatically recover from a clean EOF,
native exit, or timeout. After 500 ms backoff, reopen the same document and render
the tracked visible current page. Warm preload must not replace that page in
recovery state. File size/mtime and title/page-count changes reject recovery
rather than silently committing a different on-disk document. Session and job
identities are retained; replacement transport request IDs belong to the new
connection, so the old connection cannot provide results after restart.

Allow at most one restart in a rolling 60-second window. Another failure within
the window, or failure during restart preparation, suppresses automatic retries
until an explicit open creates a new proxy/session. Protocol/version/truncation
failures and observed memory-limit failures are never replayed. Candidate
failures do not consume the active document's restart budget. Hot reload stops
retrying a crashed/timed-out/malformed candidate for that file revision;
ordinary incomplete-write/PDF errors retain the existing bounded reload retry
policy, and a later filesystem revision can prepare a new candidate.

If recovery fails, `WorkerFailed` invalidates the failed session, keeps cached
pixels and the last-good audience image, and offers `Open the PDF again`. A later
valid PDF can replace the failed scheduler after its worker finishes.

## Product resource limits

Named shared limits live in `renderer_limits.rs`; wire-specific limits live in
`renderer_protocol.rs`. Both helper preflight and broker validation apply them.

| Resource | Limit |
| --- | --- |
| PDF input bytes | 1 GiB, regular nonempty `%PDF-` file |
| Pages | 10,000 |
| Target/output dimension | 1–4,096 pixels per side |
| Decoded pixels / RGBA payload | 16,777,216 pixels / 64 MiB |
| Control frame / encoded path / title or error | 1 MiB / 32 KiB / 4 KiB UTF-8 |
| Notes per page / document | 64 KiB / 512 KiB UTF-8 |
| Pending command work / worker work / events | 64 each |
| Queued event pixels (including prepared reload pages) | 128 MiB total |
| Helper memory | 1 GiB per helper, platform-specific below |

Geometry must be finite and positive. Checked calculations validate target and
rounded output sizes before native bitmap allocation, verify returned bitmap
dimensions before image conversion, and validate RGBA lengths before copying.
Note accumulation and aggregate event bytes use checked arithmetic. Byte-pressure
eviction retains the event priority policy below; the worker queue is bounded as
well as the UI command mailbox. Existing 96 MiB render-cache policy is unchanged.
Native annotation APIs may allocate a string before its length can be checked;
OS helper limits are the backstop, not a claim that PDFium allocates only output
pixels or text.

Platform memory controls:

- Linux: hard and soft `RLIMIT_AS` are set to 1 GiB before PDFium initialization.
  This caps virtual address space, including mappings, not just resident pages.
  Failure to install the control prevents helper startup.
- Windows: the broker assigns the child to a non-inherited Job Object before
  sending any document work, with 1 GiB process committed-memory limit,
  kill-on-job-close, and native-error-dialog suppression. Unsupported/failed job
  assignment fails closed, rather than running without the requested limit.
- macOS: sample `proc_pid_rusage` resident bytes every 50 ms and kill/reap an
  over-budget helper. This is a **best-effort sampled fallback**, not a hard
  pre-allocation cap: short bursts can overshoot and unavailable sampling leaves
  only geometry/wire caps and operation deadlines. There is no portable strict
  macOS address-space cap suitable for this runtime's mappings. Do not describe
  sampled RSS as equivalent to Linux address-space or Windows commit limits.

Unix helpers disable native core dumps. Timeout/crash/protocol diagnostics contain
operation, fixed failure classification, and exit status only; helper-controlled
error chains, paths, notes, and pixels are not logged for those failures. The
application does not enable full-memory dumps; OS crash-reporting policy is
outside this logging boundary. None of these controls restrict filesystem or
network authority: sandbox work remains #374–#376.

References: [Linux resource limits](https://man7.org/linux/man-pages/man2/getrlimit.2.html),
[Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
and [Apple resource definitions](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/resource.h).

Debug-only executable tests inject hang, abort, EOF, version mismatch, truncated
and oversized replies, untrusted diagnostic text, and excessive allocation
attempts. They cover transactional candidate failure, bounded restart, clean
recovery with a later valid PDF, and scheduler shutdown during a hung render.
Release builds ignore all `QUICK_PRESENTER_HELPER_TEST_*` hooks and omit internal
recovery/scheduler smoke entry points.

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

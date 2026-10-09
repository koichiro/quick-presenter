# Architecture

## Layers

Current runtime:

```text
Slint UI -> Rust broker state/scheduler -> bounded IPC -> renderer helper -> PDFium
```

The current and target trust boundaries, failure guarantees, and rollout stages
are defined in [PDF Rendering Security and Isolation Policy](PDF_RENDERING_SECURITY.md).

## Design direction

The app should keep PDF rendering, page caching, file IO, and presentation state in Rust. Slint should stay focused on UI layout, events, and display.

Display discovery and native window placement follow the same boundary. The
proposed `X` shortcut for exchanging the presenter and slide displays is defined
in [Presenter and Slide Display Swap](DISPLAY_SWAP.md). Slint forwards the key
event; Rust owns monitor snapshots, the two-window swap transaction, focus
recovery, and platform limitations.

Restoration of audience window placement and the last active PDF
is defined in [Slide Window and PDF Startup Restoration](SLIDE_WINDOW_PLACEMENT.md).
Rust owns settings validation, safe placement, capture after verified display
swaps, and startup PDF selection through the existing asynchronous open pipeline.
The saved PDF path is separate from page, timer, black-screen, and fullscreen
state; a reopened PDF starts a new windowed session at its first page.
Smoke execution modes bypass these settings before store creation, preserve
their explicit test PDF and harness geometry, and never save startup state.

Diagnostic file creation, permissions, byte accounting, and rotation remain in
the Rust diagnostics boundary. The v1.0.0 retention design is documented in
[Diagnostic Log Retention](DIAGNOSTIC_LOG_RETENTION.md); Slint and presentation
state do not manage log lifecycle.

## Local presentation control

The stable [Presentation Control Protocol v1](CONTROL_PROTOCOL.md) exposes
GUI-owned presentation state through local IPC. Typed, bounded requests are
dispatched on the existing event loop, using shared navigation/black-screen
commands and the existing asynchronous PDF open pipeline. The server owns no
independent presentation state. `qp` uses the protocol library without Slint
initialization or PDFium access. Renderer IPC and presentation control IPC
remain separate boundaries.

## PDFium ownership

`src/pdf.rs` keeps PDFium initialization behind a helper-process-global
`OnceLock<PdfiumRuntime>` containing the `Pdfium` owner.
This gives each loaded `PdfDocument<'static>` a real long-lived owner without
placing a self-referential owner and borrowed document in the same Rust struct.

`PdfDocumentState` owns the loaded document plus presentation-facing metadata
such as the file path and page count. It does not own PDFium bindings directly,
and it must not extend `PdfDocument` lifetimes with `unsafe` transmutation.

This boundary keeps the current open/render path predictable while leaving room
for page caching, preloading, and multi-window rendering to reuse the same loaded
document safely.

## PDFium process boundary

Quick Presenter treats PDFium document access as worker-confined. `shared_pdfium()`
is only the process-global initialization boundary; it does not make loaded
documents safe to use from multiple threads.

On macOS, bundled release builds keep each `PdfDocumentState` in an independently
App-Sandboxed XPC service. A PDF-specific nested proxy app authenticates that
service and transfers the broker-opened read-only FD and protocol pipes. Distinct
proxy clients preserve active/candidate process independence. The service uses
PDFium's owned reader API; it never reopens the display path. Its document still
borrows the real process-global PDFium owner; no unsafe lifetime extension is
used. See [macOS Renderer Sandbox](../packaging/macos/RENDERER-SANDBOX.md) for
signature requirements, package support and release gates.

Other platforms and the unbundled debug regression harness keep each
`PdfDocumentState` in a renderer helper launched
from `current_exe()` with the internal `--renderer-helper` argument. This mode is
selected before CLI parsing, diagnostics, or Slint initialization. The UI
process never initializes or calls PDFium, including About and headless smoke
mode. About reads version metadata without loading a native library.

`RenderScheduler` retains its broker thread and mailbox/priority policy, but owns
`RemoteDocument` proxies rather than native documents. Helpers execute open,
render, and note requests serially over bounded anonymous pipes. The broker
validates responses before sending metadata, pixels, or errors to the UI, which
continues to own session acceptance, caches, and Slint images.

Render command backlog, worker queue, and render event drop behavior are defined
in [Render Scheduling](RENDER_SCHEDULING.md).

Replacement opens and hot reload prepare a separate candidate helper. They keep
the active helper until open and the initial/current-page render both succeed;
discarding a candidate terminates and reaps only that process. Each helper has
its own PDFium runtime, with no cross-process native document sharing.

Helper EOF, abnormal exit, and protocol errors are transport failures, not Rust
panics in PDFium. Unrecoverable active-helper failures become `WorkerFailed`; candidate
failures remain open/reload failures and preserve the current deck. Scheduler
shutdown kills and reaps registered helpers even if native work is blocked.
A helper's input guardian exits on parent-pipe closure even during native work.
Only unit tests retain synchronous in-process PDF loading.

Non-macOS helpers on this branch still run unsandboxed; #375/#376 supply their
platform boundaries separately. Independent broker watchdogs enforce operation
deadlines and supervise helper memory; a crash/timeout may trigger one bounded
restart, while malformed IPC is not replayed. Numeric budgets, platform controls,
and fallback guarantees are documented in [Render Scheduling](RENDER_SCHEDULING.md).
XPC startup/connection loss complements rather than replaces these watchdogs.

## Native file dialogs

The macOS and Windows open-file paths intentionally keep using the synchronous
native `rfd::FileDialog` API from the UI callback. Those platforms already
provide modal dialog behavior that integrates with the desktop environment
without triggering the Linux-specific not-responding warning.

Linux is different: the synchronous `rfd::FileDialog` path can wait for an XDG
Desktop Portal or fallback dialog response while the Slint/winit event loop is
not being serviced. Quick Presenter therefore opens Linux file dialogs on a
short-lived worker thread and polls the selected path back on the UI thread. The
UI thread remains the only place that mutates `AppState` or schedules the PDF
open, while the worker thread owns only the blocking dialog call. The Linux
dialog is still built on the UI thread so it can capture the presenter window as
its native parent before the blocking picker work moves to the worker thread.

## PDF hot reload

The active PDF is automatically watched for external changes. Reload candidates
must be opened and have their current page rendered in a separate candidate
helper before they can replace the last good document. The watcher boundary,
debounce and retry policy, worker transaction, cache invalidation, and platform
verification plan are defined in [PDF Hot Reload](PDF_HOT_RELOAD.md).

### On-demand presentation source queries

The local control adapter requests current/next page source text through
`RenderCommand::ExtractSlideText`. The scheduler places this work at background
priority, deduplicates pending page requests, and sends it to the existing
isolated PDFium helper. Private renderer IPC v4 adds bounded, correlated text
request/reply types; public Presentation Control Protocol remains v1.

The helper extracts PDF text without OCR. The GUI owner caches at most 32 compact
source results and two full results, keyed by page within the committed renderer
session and separated by extraction mode. Open,
reload, and close invalidate the cache; stale results are rejected. Pending
control queries record the page and document revision, and return a typed
cancellation if those change. The adapter assembles current/next text, existing
`SpeakerNotes`, and presentation state on the event loop without blocking it.
`qp` only parses requests and formats responses.

Full source queries opt into bounded response transfer frames at both IPC
boundaries. Each carries typed sequence/length metadata and at most 64 KiB of
serialized response bytes. The client reconstructs one typed logical response,
validating identity, ordering, and aggregate size before output. Native character
and full-page byte guards, bounded caches, and existing watchdogs remain in
force. Ordinary queries keep the compact single-frame response contract.


### Control event delivery

Committed session commands and open/reload/close transitions publish typed
control events directly from the existing presentation owner. The event hub
stores sequence numbers and bounded subscribers, not a second presentation
state. Watch registration captures an atomic status/sequence baseline on the
same UI event loop. IPC worker threads deliver frames and invisible heartbeats;
slow watchers are removed without blocking GUI transitions. `qp watch --json`
flushes NDJSON events. `qp timer elapsed` only reads the existing timer; no
state-changing timer command is exposed in this version.

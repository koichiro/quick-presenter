# Architecture

## Layers

Current runtime:

```text
Slint UI -> Rust app state -> in-process render worker -> pdfium-render -> PDFium
```

Target isolated runtime:

```text
Slint UI -> Rust broker state -> bounded IPC -> renderer helper -> PDFium
```

The current and target trust boundaries, failure guarantees, and rollout stages
are defined in [PDF Rendering Security and Isolation Policy](PDF_RENDERING_SECURITY.md).

## Design direction

The app should keep PDF rendering, page caching, file IO, and presentation state in Rust. Slint should stay focused on UI layout, events, and display.

Diagnostic file creation, permissions, byte accounting, and rotation remain in
the Rust diagnostics boundary. The v1.0.0 retention design is documented in
[Diagnostic Log Retention](DIAGNOSTIC_LOG_RETENTION.md); Slint and presentation
state do not manage log lifecycle.

## PDFium ownership

`src/pdf.rs` keeps PDFium initialization behind a process-global `OnceLock<Pdfium>`.
This gives each loaded `PdfDocument<'static>` a real long-lived owner without
placing a self-referential owner and borrowed document in the same Rust struct.

`PdfDocumentState` owns the loaded document plus presentation-facing metadata
such as the file path and page count. It does not own PDFium bindings directly,
and it must not extend `PdfDocument` lifetimes with `unsafe` transmutation.

This boundary keeps the current open/render path predictable while leaving room
for page caching, preloading, and multi-window rendering to reuse the same loaded
document safely.

## PDFium threading boundary

Quick Presenter treats PDFium document access as worker-confined. `shared_pdfium()`
is only the process-global initialization boundary; it does not make loaded
documents safe to use from multiple threads.

The normal runtime path keeps each loaded `PdfDocumentState` on the single render
worker owned by `RenderScheduler`. The UI thread sends open, render, speaker-note,
thumbnail, and preload requests to that worker and receives metadata, rendered
pixels, and presenter-facing errors through render events. UI-thread state should
not hold PDFium-owning types in the production path.

Render command backlog, worker queue, and render event drop behavior are defined
in [Render Scheduling](RENDER_SCHEDULING.md).

Test and GUI smoke helpers may open a temporary synchronous PDF session, but that
session is kept outside `AppState` so production state still reflects the
worker-local ownership model.

This single-worker model is intentional until a future design proves a broader
thread-safety strategy. Any parallel rendering change must make an explicit
decision about PDFium's thread-safety guarantees, whether documents are opened
per worker or shared behind a synchronization boundary, how cancellation works
across workers, and how rendered pages are merged back into the shared cache.

The approved direction is process isolation rather than multiple PDFium threads
inside the UI process. The UI process remains the broker and owns scheduling,
session acceptance, caches, and Slint objects. A supervised helper owns PDFium;
per-platform sandboxing is a separate layer. Until #372 replaces the production
path, the single-worker description above remains the implemented architecture.

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
must be opened and have their current page rendered on the existing PDFium
worker before they can replace the last good document. The watcher boundary,
debounce and retry policy, worker transaction, cache invalidation, and platform
verification plan are defined in [PDF Hot Reload](PDF_HOT_RELOAD.md).

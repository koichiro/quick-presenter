# Architecture

## Layers

```text
Slint UI
  ↓ callbacks
Rust app state
  ↓ render requests
pdfium-render
  ↓ dynamic linking
PDFium native library
```

## Design direction

The app should keep PDF rendering, page caching, file IO, and presentation state in Rust. Slint should stay focused on UI layout, events, and display.

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

This single-worker model is intentional until a future design proves a broader
thread-safety strategy. Any parallel rendering change must make an explicit
decision about PDFium's thread-safety guarantees, whether documents are opened
per worker or shared behind a synchronization boundary, how cancellation works
across workers, and how rendered pages are merged back into the shared cache.

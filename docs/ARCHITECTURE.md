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

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

## Near-term technical debt

`src/pdf.rs` contains a clearly marked lifetime shortcut to keep the initial scaffold compact. Before serious development, refactor `PdfDocumentState` to store PDFium bindings in a long-lived owner and keep `PdfDocument` lifetime strictly tied to that owner, or use a loader abstraction that reopens documents safely.

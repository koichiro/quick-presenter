# Quick Presenter

Quick Presenter is a Rust + Slint + PDFium scaffold for a presentation playback app focused on presenting PDF slide decks.

## Run

```sh
cargo run --bin qp
```

Load a PDF at startup:

```sh
cargo run --bin qp -- --pdf path/to/slides.pdf
```

The Cargo package remains `quick-presenter`, while the runtime executable is named `qp`.

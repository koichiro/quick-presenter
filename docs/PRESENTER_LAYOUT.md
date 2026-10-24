# Presenter window layout

The presenter window keeps the current slide and notes on the left, the next
slide and keyboard reference in the middle, and slide thumbnails on the right.
Audience Live stays below the first two columns; thumbnails extend to the bottom
of its URL row.

Sizing follows the available window space:

- The current and next columns share width at approximately 2:1. The next
  column is bounded to 260–480 logical pixels so that compact windows retain a
  usable preview and wide windows do not grow the keyboard reference indefinitely.
- Thumbnails use 10% of the main area width, bounded to 128–192 logical pixels.
- The slide row uses up to 66% of the main area height, while preserving at least
  200 logical pixels for notes below the heading. A slide that fits at its natural
  aspect ratio releases the remaining height to notes.
- Notes use the entire current-slide column, including when a portrait slide is
  narrower than its column. Existing Rust note fitting and scrolling still apply.
- The next preview scales with its column and retains its aspect ratio within the
  slide row. The keyboard reference remains at most 200 logical pixels high.
- The Audience Live footer uses 28% of window height excluding the inline menu,
  bounded to 282–360 logical pixels, including its controls and URL row.

These are logical pixels; display scaling is still handled by Slint. This layout
introduces no changes to PDF ownership, rendering requests, navigation, or window
placement.

## Validation

`cargo test --test presenter_layout` renders the compiled Slint component with a
software window. It covers six sizes (800×842 through 2560×1440), three slide
aspect ratios (16:9, 4:3, portrait), and both menu modes: 36 combinations. Assertions
protect usable notes space, bounded preview/thumbnail widths, proportional column
sizing, and separation of the footer and thumbnails. Unlike a native resize,
this test can exercise windows larger than the attached display.

To save the rendered layouts for inspection:

```sh
QP_PRESENTER_LAYOUT_SNAPSHOTS=/tmp/qp-presenter-layouts \
  cargo test --test presenter_layout
```

The PPM files show layout with placeholder slides and sample notes, not PDF
rendering or native menu/window chrome. The native GUI smoke also checks the six
window sizes when it reaches the Audience Live layout checks.

Local validation on 2026-10-24: formatting, compilation, the complete Cargo test
suite, and the 36 software-rendered layouts passed. The 1920×1080 landscape and
800×842 portrait/inline-menu images were visually inspected. The signed renderer
PDF/notes and sandbox denial gate passed during bundle signing. The full macOS
GUI smoke stopped before layout checks at rapid fullscreen-entry cancellation
(`will-enter` without completion), both from a direct bundled launch and through
Launch Services. This does not establish native fullscreen, Windows, or Linux
runtime qualification for the layout change.

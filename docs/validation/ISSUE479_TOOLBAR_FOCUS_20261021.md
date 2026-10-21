# Presenter toolbar shortcut precedence — 2026-10-21

Tracking: #479, macOS toolbar observation. Baseline: main `430cef0`, macOS 27.0.1 arm64.
The Windows notes-focus fix is handled separately in the Windows session.

The existing presentation KeyBindings run while bubbling from the focused item.
A native toolbar button can accept the event first. Add a capture-only FocusScope
around toolbar descendants and classify presentation shortcuts in Rust before
native button handling. The scope is excluded from click and Tab focus, avoiding
an extra focus stop. Dispatch accepted shortcuts through the existing keyboard
callbacks so page/timer/blackout/fullscreen semantics remain shared.

Space and Return retain normal focused-button activation. Tab/Shift+Tab keep
normal traversal. Other unmodified presentation keys and physical Ctrl+A/E have
presentation precedence. Shift/Alt and unrelated Command/Control shortcuts are
not captured. Notes are outside the scope and retain their independent rules.

Validation:

- Rust unit tests cover button/traversal exceptions, all toolbar presentation
  actions, macOS physical Control mapping, and modified-key rejection.
- `cargo fmt --check`, `cargo check`, `cargo test`: passed. The existing
  macos_renderer.rs fetch_update deprecation warning remains unrelated.
- Release build and a Developer ID signed release-layout app with bundled
  PDFium 156.0.8076.0 passed the XPC PDF render/notes and unrelated-file/network/
  child denial gate. Local signing used no timestamp; this is not a notarized
  DMG or a Store installation.
- Native macOS toolbar reproduction/regression and notes interaction: pending;
  the desktop locked while the signed fullscreen fix was under validation.
- Physical-key and final distributed-artifact acceptance remain open.

This implements the toolbar-specific precedence path. Do not close #479 until
its separate Windows notes, macOS toolbar and release-artifact checks are
recorded. It is not a claim that the two native observations share a cause.

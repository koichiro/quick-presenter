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

## Native baseline comparison after unlocking

A toolbar-unmodified signed baseline, built from #504 source 14484d4 (main
430cef0 plus fullscreen changes, without the toolbar capture), was tested with
native injected input. Its binary had no toolbar_shortcut symbol; the #506
candidate did. Seven Tabs from the current-slide image focused Black, confirmed
by AX. Space activated blackout. After setting page 5 with the isolated CLI,
Home returned to page 1 and B cleared blackout while AX still reported Black
focused. Thus the old v2.0.0 toolbar observation was **not reproduced in this
specific baseline/focus state**. Do not claim an isolated shared cause, a
reproduction on physical keys, or that all focus states work.

The capture change establishes explicit toolbar precedence, but its necessity
for the current macOS observation is not proven. Candidate native macOS
comparison was interrupted when the UI automation call stalled and the desktop
locked again. Keep #506 draft pending a reproducible affected artifact/focus
state and its candidate check. Windows cross-platform toolbar regression was
requested from the Windows session. The separate Windows notes reproduction
and repair in #503 do not establish the macOS toolbar cause.

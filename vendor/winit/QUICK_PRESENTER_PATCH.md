# Quick Presenter patch for winit 0.30.13

This directory contains the upstream winit 0.30.13 crate, distributed under
Apache-2.0 (see `LICENSE`). Cargo overrides crates.io through the repository's
`[patch.crates-io]` entry so all Slint and application users resolve to the same
version.

Quick Presenter does not request winit window blur. On macOS, this copy makes
`WindowDelegate::set_blur` a no-op and removes its private CoreGraphics imports
`CGSMainConnectionID` and `CGSSetWindowBackgroundBlurRadius`. The Slide window's
transparent title bar uses public AppKit calls in `src/macos_window.rs`, separate
from winit blur. Keep the override until a compatible stable winit release
implements blur using public APIs and the Store binary check passes.

Upstream issue: https://github.com/rust-windowing/winit/issues/4205

## Keyboard delivery during native fullscreen transitions (#491)

AppKit can receive key events in `NSApplication::sendEvent:` while suppressing
normal delivery to windows during a native fullscreen animation. This was
reproduced separately for presenter entry and exit: Right reached the application
but never produced a winit `KeyboardInput` event.

During a winit fullscreen transition, route non-Command key-down/up events once
to the current key window when it is a `WinitWindow`, then skip the original
application dispatcher. The first responder still handles notes and controls.
Command shortcuts, other window classes, and normal dispatch outside transitions
retain the existing path. Events are neither queued nor replayed. Winit's existing
`target_fullscreen` mechanism handles a rapid Escape/reversal request. The
`fullscreen()` getter reports that queued intent, including an explicit exit,
so an intermediate resize cannot make Slint overwrite it with the old state.

A per-thread transition count covers multiple windows, duplicate notifications,
failed entry, and destruction during an animation. Its lifecycle is tested by
`tests/macos_fullscreen_transition.rs` without creating AppKit windows. Native
input and distribution qualification remain separate checks.

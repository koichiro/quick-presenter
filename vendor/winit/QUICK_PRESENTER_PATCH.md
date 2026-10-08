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

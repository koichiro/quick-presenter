# Slide Fullscreen

Quick Presenter fullscreen playback currently targets the audience-facing
`SlideWindow` only. The presenter window remains a normal control window.

The first implementation uses Slint's window API:

- `Window::set_fullscreen(true)` to enter fullscreen
- `Window::set_fullscreen(false)` to leave fullscreen
- `Window::is_fullscreen()` when the native window state needs to be queried

With the current Slint winit backend, this maps to borderless fullscreen on the
current display. Monitor selection is intentionally deferred so the first
fullscreen workflow can stay small and predictable.

Platform notes:

- macOS, Windows, and Linux window managers can differ in focus handling,
  animation, and how fullscreen windows are assigned to displays.
- The Escape key path is handled by the slide window UI. It requires the slide
  window to have keyboard focus.
- Presenter controls also provide a fullscreen toggle so the presenter can leave
  fullscreen without relying on slide-window focus.
- Future monitor selection should be implemented behind the Rust fullscreen
  helper boundary instead of embedding platform-specific logic in Slint UI code.

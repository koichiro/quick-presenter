# Audience Fullscreen

Quick Presenter fullscreen playback currently targets the audience-facing
`AudienceWindow` only. The presenter window remains a normal control window.

The first implementation uses Slint's window API:

- `Window::set_fullscreen(true)` to enter fullscreen
- `Window::set_fullscreen(false)` to leave fullscreen
- `Window::is_fullscreen()` when the native window state needs to be queried

With the current Slint winit backend, the normal fullscreen toggle maps to
borderless fullscreen on the current display. The `X` display-swap command uses
a narrow Winit boundary to retarget borderless fullscreen without changing the
existing fullscreen intent; see
[Presenter and Audience Display Swap](DISPLAY_SWAP.md).

Platform notes:

- macOS, Windows, and Linux window managers can differ in focus handling,
  animation, and how fullscreen windows are assigned to displays.
- The Escape key paths in both windows exit audience fullscreen. The window
  receiving the shortcut must have keyboard focus.
- Presenter controls also provide a fullscreen toggle so the presenter can leave
  fullscreen without relying on audience-window focus.
- Monitor-aware fullscreen and window placement must stay behind a Rust window
  management boundary instead of embedding platform-specific logic in Slint UI
  code.

## macOS Audience Window Chrome

The audience window keeps native window buttons but hides its title and draws its
content behind a transparent title bar. The macOS window factory configures
Winit's full-size content view, transparent title bar, and hidden title before
the native window and renderer surface are created. The presenter retains its
standard title bar. Runtime chrome synchronization still restores the audience-window
style after fullscreen transitions.

Do not restore the old negative 28-point Slint offset or subtract that height
from the fitted window size. With full-size content enabled at creation, the
viewport already includes the title bar; an additional offset clips the slide.
On macOS, startup restoration fits the saved window within its previous bounds
to the deck's aspect ratio, and opening a PDF also refits a preserved window.
This removes unused bands from older saved sizes while retaining display
placement and avoiding slide cropping. Manually resizing to another aspect
ratio or using a differently shaped fullscreen display still letterboxes.
The macOS GUI smoke check verifies AppKit full-size content, titlebar
transparency, hidden title, and matching native inner/outer dimensions before
runtime chrome synchronization. Native chrome attributes are rechecked after
each lifecycle operation. A synthetic image
has distinct colored bands at all four edges; three samples per edge must reach
the complete native window without cropping or black padding. The checks cover
windowed playback, resizing, fullscreen return, and the production startup
restoration planner with tall and short saved dimensions with widescreen and standard page ratios. A
separate async PDF-open check verifies that a preserved tall window is refitted.
Fixtures use in-memory placement records and do not modify user settings.

The `macOS audience window regression` CI job runs these checks on every pull
request and main push, and uploads the report even on failure. Run them locally
with a working WindowServer session and development PDFium library:

```sh
cargo build --locked
target/debug/quick-presenter \
  --gui-smoke tests/fixtures/marp-speaker-notes.pdf \
  --gui-smoke-report /tmp/macos-audience-regression.txt
```

macOS resize actions run inside the active event loop so synchronous AppKit
notifications reach Winit, and fullscreen animations settle before inspection.
The tests assert dimensions and pixels rather than platform-dependent screenshot
baselines. A missing native display is an error, not a skipped success.

## Windows Audience Window Chrome

On Windows, Quick Presenter keeps the audience window as a standard
decorated native window. The audience window title, window buttons, resizing,
moving, snapping, task switching, and native fullscreen behavior remain owned by
Windows.

Windows 11 Build 22000 and newer can reduce title bar contrast through DWM
window attributes. The Windows-specific boundary in `src/windows_window.rs`
uses Slint's raw window handle to obtain the audience window `HWND`, then applies
`DWMWA_CAPTION_COLOR`, `DWMWA_BORDER_COLOR`, and `DWMWA_TEXT_COLOR` with
`DwmSetWindowAttribute`.

Unsupported Windows versions should keep the normal native title bar. DWM
attribute failures are logged for diagnostics and are not treated as presenter
visible errors.

## Placement Between Runs

Normal startup restores the Audience Window's windowed size and placement and reopens the
last active PDF at its first page. Fullscreen intent is not saved. Quitting in
fullscreen preserves the last captured windowed dimensions. A verified `X` swap
in fullscreen updates the remembered destination without saving fullscreen
bounds as the windowed size. If displays changed, startup falls back to an
available display. See [startup restoration](AUDIENCE_WINDOW_PLACEMENT.md).

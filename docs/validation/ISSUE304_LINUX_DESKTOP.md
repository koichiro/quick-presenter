# Issue #304: Linux desktop icon validation

Date: 2026-10-07. PR: [#405](https://github.com/koichiro/quick-presenter/pull/405).

## Environment and scope

- Ubuntu GNOME Shell 50.1, Linux 7.0.0-38-generic.
- The desktop session is Wayland. The X11 runs below use XWayland, not a separate
  native Xorg session.
- Debug build of the PR with the installed PDFium chromium/8076 library, matching
  the pinned version. Additional smoke validation sets the same desktop app ID
  as normal startup before showing windows.
- Automated runs use temporary configuration/state directories. The reporter's
  manual checks use the normal startup settings.

## Results relevant to #304

| Check | Result and evidence |
| --- | --- |
| Direct startup: taskbar icon delay | The reporter confirmed that the icon appears clearly earlier and considers the reported problem fixed on this desktop. This is qualitative evidence, not a measured latency. |
| Alt+Tab icon and application grouping | The reporter confirmed the correct icon and that both windows belong to the same application. |
| Wayland `.desktop` startup, three launches | All launches succeeded through GIO DesktopAppInfo with a GDK Wayland launch context. Actual launcher tokens were delivered. Both initial toplevels sent `quick-presenter` as their app ID; exactly one activation request was sent per process. |
| XWayland `.desktop` startup, three launches | All launches succeeded through GIO DesktopAppInfo with a GDK X11 launch context. Both windows had the expected `WM_CLASS` and a 256 x 256 `_NET_WM_ICON`. The first window received the launcher's startup ID; the presenter did not reuse it. |
| XWayland initial presenter focus | `_NET_ACTIVE_WINDOW` matched the presenter window after each of the three `.desktop` launches. |
| Windowed and fullscreen hide/show/raise | Both backend smoke runs passed all eight hide/reveal scenarios, including close requests and the Window menu callback paths. The restored slide rendered a new frame with the image changed while hidden. |
| Wayland identity after native window recreation | The smoke trace contained ten `set_app_id("quick-presenter")` requests across initial windows and recreated slides, with only one activation request throughout the process. |

The `.desktop` test copies the installed entry into a temporary directory and
changes only its executable command to launch the PR binary through a diagnostic
wrapper. It preserves the `quick-presenter.desktop` basename, `Icon`,
`StartupWMClass`, and `StartupNotify`. The wrapper preserves real launcher tokens
and captures their presence without publishing their values.

Three-launch runs cover an initial process in an isolated configuration followed
by repeats. They do not establish cold-boot or cold-icon-cache performance, and
they do not reproduce clicking GNOME's installed launcher with a rebuilt package.
The contribution of the icon declaration versus startup-token handling has not
been isolated.

## Automated checks

- `cargo fmt --check`: passed.
- `cargo check --locked -j 2`: passed.
- `PDFIUM_DYNAMIC_LIB_PATH=/usr/lib/quick-presenter/pdfium/lib/libpdfium.so cargo test --locked -j 2`:
  415 passed (396 application unit tests, 4 startup metadata tests, 14
  renderer-process tests, and 1 slide layout test).
- GitHub CI for the initial PR implementation passed:
  [run 37580172032](https://github.com/koichiro/quick-presenter/actions/runs/37580172032).

Each GUI smoke run has 62 passing checks and one failing assertion:
`wrapped notes use the largest fitting measured size (with a PDF open)`.
The viewport is 199 px; the measured heights for 12 px and 13 px text are 184 px
and 200 px. Including the test's 10 px margin, only the minimum size fits, while
the smoke assertion requires a size greater than 12 px. The selected 12 px size
is consistent with the measured constraints. This is a separate notes-smoke
fixture/assertion issue; the icon, lifecycle, navigation, and rendering checks
above passed. The full GUI suite is not claimed to pass.

## Closure assessment

The original taskbar delay, Alt+Tab icon, and application grouping have positive
manual confirmation on the reporter's desktop. Repeated `.desktop` launches and
native window recreation satisfy the relevant identity and startup-notification
contracts. These results support resolving #304 when #405 is merged.

Native Xorg, KDE and other shells, quantitative cold-start timing, and the shared
window icon on Windows remain outside this validation. Those results are not
claimed here. The separate notes-smoke assertion failure remains recorded above.

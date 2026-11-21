# Slint 1.19.0 native Linux step 5

## Outcome

The available native desktop checks were executed on 2026-11-17, approximately
15:50–16:36 JST (Asia/Tokyo, UTC+09:00). The Wayland comment-measurement
regression from step 4 was reproduced, compared with Slint 1.18.1, and worked
around with a small application change. After that change:

- Formatting and `cargo check --all-targets --locked` passed.
- `cargo test --locked`: **630 passed, 0 failed, 2 ignored**, none filtered.
- Complete debug GUI: **215 passed / 0 failed on native Wayland**, and
  **215 passed / 0 failed on XWayland**.
- Rebuilt release GUI on native Wayland: **215 passed / 0 failed**, with
  actual rendered bubble snapshots saved.
- Normal release launches, real compositor input, native file selection,
  launch-token handling, notes scrolling, Recent selection and persistence
  checks were performed separately from automatic GUI smoke.

This is not unconditional step-5/release approval. Physical multiple-monitor
and mixed-DPI checks remain blocked by the available single display. An existing
Linux Bring-to-Front limitation failed native focus checks. The About clipping was subsequently repaired; see the
[About follow-up](SLINT_1_19_LINUX_ABOUT.md) for the separate patch, matched
1.18/1.19 comparison, native renderer evidence and completed normal/minimum
unlocked-desktop pointer checks. No About-specific blocker remains.

## Source and host identity

- Branch: `codex/slint-1-19-migration`.
- HEAD before/after: `deee3b6731e631c86309b8e681a2c3fd20afdaf6`.
- Existing migration implementation: `23648de76b2a4136a5d2f587e0f8353eb6e4e335`;
  the committed resize-clock fix in `deee3b6` is included.
- Starting dirty state: the step-4 migration-index edit, untracked Linux step-4
  report, and the eight pre-existing untracked validation documents recorded in
  [step 4](SLINT_1_19_LINUX_RESULTS.md). They were preserved. No fetch, reset,
  merge, commit, push, signing, upload, release or coverage measurement occurred.
- New source/test edits: `src/audience_ui.rs`, `ui/app.slint`,
  `tests/comment_measurement_layout.rs`. The shared migration index was not
  edited again during step 5.
- OS: native Ubuntu 26.04.1 LTS, Linux `7.0.0-43-generic`, x86_64,
  Intel Core i5-8400T; GNOME Shell 50.1.
- Rust/Cargo 1.96.0, Python 3.14.4; toolchain/build-library details remain in
  the step-4 report. The lockfile, edition-1.18 Slint compiler configuration,
  registry Slint/backend 1.19.0, and local winit 0.30.13 override are unchanged.
- Actual session: Wayland, also exposing XWayland `DISPLAY=:0`.
  X.Org/XWayland reports server 24.1.10. No native Xorg session was available.
- Actual physical/logical display: one HDMI-2 Dell U4021QW, 2560×1080,
  scale 1.0. The final monitor query still found one physical and one logical
  display. No monitor mode, compositor scale, or desktop layout was changed.
- Backend: winit Wayland and winit X11 through XWayland, tested separately.
  `SLINT_BACKEND` was unset: the compiled default is FemtoVG/OpenGL. The native
  runs did not force software GL; GPU vendor/driver was not separately queried.
  None of the step-5 native results below comes from Xvfb.
- Initial runs used the extracted step-4 release payload. All checks after the
  fix are identified as rebuilt debug/release. The new standalone release
  payload contains bundled, verified x86_64 PDFium 158.0.8111.0; overrides are
  removed from its child environment. Debug uses the explicit verified 8111
  library from step 4. Existing installed Quick Presenter 2.5.0 and repository
  PDFium 8076 were preserved.

## Evidence and reproducibility

Raw commands, exit statuses, stdout/stderr, scripts, snapshots, fixtures, and
machine-readable command metadata are under:

[`artifacts/slint-1-19-linux-step5-20261117-1550/`](../../artifacts/slint-1-19-linux-step5-20261117-1550/).

`command-statuses.json` indexes logged commands. Each top-level command log has
its exact command, timestamp, output, exit code and elapsed time. Bounded GUI
commands retain separate raw stdout/stderr. The directory is ignored by Git;
no evidence was published or force-added. Screenshots retained for the report
are restricted to the test app or its switcher icons. Temporary full-desktop
captures were removed after inspection/cropping.

The integration patch is
`linux-comment-measurement-fix.patch` (a local historical artifact)
(SHA-256 `a5e6aedf9b9f1258f704ca997eb3a85995ff6adc016f5d802586a1ceec574f78`).
It includes only the three source/test files above, not earlier documentation
edits. It was produced before the later authorized source/report transfer described
below. The integrated source is available in
[fd67ef1](https://github.com/koichiro/quick-presenter/commit/fd67ef12f76464d2d63c4c1ee78cf30f8c51b2f9).
Other operating systems must review the integrated delta and rerun the relevant menu/layout/comment checks before
combining their results with this Linux fix.

Rebuilt payload SHA-256:

- `quick-presenter`: `c2298bd04bb5f6f5e3699f5ce7c99a8045fe088e7ce370e007e2c91e8087414c`.
- `qp`: `69f82d514243a5292b636efaae8831b7ed44aa0c3f8ce1f90b43b0d7f7db363b`.

## Isolation and native input

`launcher.py` creates a temporary `quick-presenter.desktop` with the installed
entry's basename, icon, WM_CLASS, MIME declaration and StartupNotify preserved;
only Exec changes to a wrapper for the selected test binary. GIO DesktopAppInfo
and a real GDK Wayland/X11 launch context supply real launcher metadata. Tokens
are never fabricated; values are redacted from protocol output. Presence and
one-way hashes are used for comparisons.

Each case has independent XDG config/cache/data directories. Short owner-only
`/tmp/qp119s5-wayland` and `/tmp/qp119s5-x11` runtime directories isolate control
sockets; an absolute Wayland socket path still reaches the real compositor.
Startup restoration deliberately reuses only the test's saved configuration.
The new PDF and Recent fixtures live entirely under the evidence directory.
The native file dialog used keyboard path input and its real Select button;
the user's clipboard was not changed.

The desktop was initially locked. GNOME rejected RemoteDesktop session creation
with `Session creation inhibited`; this was retained as a host restriction.
After the user unlocked Linux, the same session API succeeded. Native keyboard
and pointer tests use temporary GNOME Mutter RemoteDesktop sessions, with a
linked monitor stream for absolute coordinates. They pass through the real
compositor and application event handling, rather than calling Rust/Slint action
callbacks directly. These are injected OS events, not a human operating a
physical keyboard. Screenshot/Eval/FocusApp private API attempts were denied;
no GNOME safety setting was disabled. A temporary supported ScreenCast session
provided screenshots. All sessions and test applications were stopped afterward.

## Checklist results

PASS means the evidence for the specified backend/path was observed. It does not
extend to another display server, installed distribution, physical keyboard, or
untested hardware. Automatic GUI coverage is explicitly distinguished below.

| Check | Status | Evidence and limit |
| --- | --- | --- |
| Native Wayland desktop-entry launch and app ID | PASS | `launch-wayland-unlocked.log`, `normal-wayland-*/protocol-redacted.txt`; both initial toplevels set `quick-presenter` |
| Wayland repeated launches | PASS | Unlocked, repeat2, repeat3 and final cases; real tokens supplied per launch; final cases use rebuilt release |
| First-window activation and no reuse on recreation | PASS | `wayland-close-recreation-verified.log`: 3 created native windows, 1 activation; all app IDs retained |
| Renderer child token isolation in normal startup | PASS | `fixed-renderer-token-check.log`: actual helper discovered across all GUI threads; neither launcher-token variable inherited |
| XWayland WM_CLASS and icon | PASS | `xwayland-metadata.log`, repeat4/repeat5 metadata: both windows `quick-presenter`, both have 256×256 `_NET_WM_ICON` |
| XWayland startup ID and initial focus | PASS | Exactly one native window has the matching real startup ID; presenter is `_NET_ACTIVE_WINDOW` |
| XWayland independent repeated launch/restore/close | PASS | `normal-x11-repeat4-*`, `normal-x11-repeat5-*`; prior instance fully closed before each launch |
| Alt+Tab icon/identity | PASS, bounded | `xwayland-alt-tab-icons.png`: correct icon on both app windows in the real GNOME window switcher; Wayland app identity/overview also recorded; no cold-icon-latency claim |
| Native PDF picker | PASS | `native-picker-confirm.log`: exact Japanese test PDF selected and opened through the normal Open button and GNOME file chooser |
| Previous/next, Home/End, document boundaries | PASS | `native-wayland-controls.log`, `fixed-release-controls.log`, `native-xwayland-controls.log`; state checked through isolated CLI |
| Fast repeated native keys | PASS | 30 immediate Right events clamp at the last page, 30 Left events at page 1, both backends |
| Black screen and recovery | PASS | Native B/B on both backends, explicit state assertions |
| Fullscreen, cancellation, keys during transitions | PASS, single display | Native F/Escape and six repeated transition sequences with navigation; both backends; real compositor, not Xvfb |
| Live PDF reload | PASS | `native-recents-reload-audience-close.log`: only a test-owned PDF changed, document revision advances, current page retained |
| Hidden audience page update/reveal | PASS | `native-wayland-hide-update-show.log`; normal release menu/keys, then updated source GUI lifecycle coverage |
| Audience native close/recreate and keys | PASS | Wayland final visual focus plus protocol recreation proof; `native-xwayland-final.log` uses confirmed native active-window ID |
| OS focus recovery between presenter/audience | PASS, via OS | Native Alt+Tab/click on Wayland; EWMH activation on XWayland, followed by actual native navigation |
| Bring Audience to Front menu focus | FAIL, pre-existing | Both backends leave presenter focused for an already visible audience; see diagnosis below |
| Inline Linux Settings/About menus | PASS for opening/actions | `wayland-settings-absolute.png`, `wayland-settings-bubbles.png`, `wayland-about.png`; no native menubar substitution |
| About license paragraph readability and Close | PASS after separate repair | Original failure image retained; full text and actual Close at normal/minimum release sizes confirmed in [the follow-up](SLINT_1_19_LINUX_ABOUT.md) |
| Quit/close persistence and restoration | PASS | Native presenter Alt+F4, saved startup JSON, next launcher restores PDF at page 1; Wayland and XWayland |
| Linux native Quit menu role/Command-Q | NOT APPLICABLE | Linux retains its inline menu; no native Quit item/Command-Q is exposed there; window close was tested |
| Open Recent repeated rebuild/index correctness | PASS | A–G opened, H loaded while the inline menu is open; clicking displayed Recent-F opens F, then Right works immediately; `wayland-recent-updated.png` |
| Japanese short/long notes, wrap, automatic sizing | PASS, tested fixture | Short note screenshot and complete native GUI sizing assertions; no claim for every font/locale |
| Japanese notes scroll at minimum size | PASS | Actual compositor configure 800×608; real wheel reaches `最後の行・検証終了`; `wayland-japanese-bottom.png` |
| Settings at minimum size | PASS | `wayland-settings-minimum.png`: usable scroll area and Close button; Escape closes normally |
| Thumbnails | PASS | Native first-thumbnail selection returns the short first-page note; screenshot and subsequent page-state assertions |
| Flowing comments and bubble cards | PASS, automatic native GUI | Updated complete Wayland/XWayland smoke; rebuilt release native bubble snapshots, including combined/five-card/Japanese-long cases; native Settings toggle also persisted |
| Font measurement after audience recreation | PASS after fix | New software regression and actual Wayland/XWayland native probes; full previously failing GUI assertions now pass |
| Presenter minimize/restore and differing logical scales | PASS for probe conditions | 19 asserted samples per backend, including native minimize/restore requests and presenter 2.0/audience 1.0 simulated scale; completion timing of compositor minimization not independently measured |
| Physical multi-monitor placement/display swap | BLOCKED | Only one connected monitor, also confirmed at cleanup; requires a second real display |
| Physical mixed-DPI movement/fullscreen | BLOCKED | Requires displays at different actual scale factors; logical-event probe is supporting evidence only |
| Native Xorg/another compositor | BLOCKED | Current session is GNOME Wayland; XWayland is not native Xorg |
| Installed 3.0.0 package-manager/desktop-entry acceptance | BLOCKED / not substituted | Existing 2.5.0 installation retained; temporary entry and standalone/extracted payload do not establish installation/upgrade/MIME-cache behavior |
| Signed/Store/other-OS distribution approval | NOT APPLICABLE | Outside this Linux task |

## Comment-width regression and repair

The original step-4 Wayland failures measured `Hello 世界` as width 9 after
hiding/recreating the audience, whereas the complete X11/Xvfb run passed.
A temporary probe using the actual compiled UI measured 156 and 319 for short
and long text before hiding. After Wayland recreation the width stayed 319 even
when text changed. XWayland kept updating. Logs: `comment-probe-*-recreation.log`.

A second independent, identical minimal Text/Window UI was built with Slint
1.18.1 and 1.19.0. The 1.18.1 Wayland probe keeps alternating 156/319 after
recreation; 1.19.0 freezes at 319. Making the Text transparent instead of invisible
also freezes, so that candidate was rejected without changing production UI.
Logs: `compare-118-wayland.log`, `compare-119-wayland.log`,
`compare-119-opacity-wayland.log`. Probe source and separate lockfiles are saved.
These compare the two pinned dependency stacks; they are not a source-level
upstream root-cause bisect.

The repair moves the invisible 32px/700/no-wrap measurement Text and its two
properties to the persistent presenter, and reads its measured width there in
`AudienceUi::refresh`. The displayed audience text, flow physics, fonts, width
thresholds, fitting assertions, and Slint/backend/vendor dependencies are
unchanged. Rust still owns comment admission/state. This avoids the audience
renderer-recreation boundary rather than weakening a GUI assertion.

`persistent_presenter_measures_new_comments_after_audience_hide_and_show` verifies
short/long widths across repeated audience hide/show using compiled UI on the
software platform. The temporary native probe adds real audience recreation,
presenter minimize/restore requests, and simulated differing window scales.
All 19 samples on each backend remain correct. Native fullscreen/recreation
and full comments tests then passed on both backends, including release.

Boundary: the product currently does not call `hide_presenter_window` and exposes
no presenter-hide menu. That unused helper still exists. Hiding/recreating the
presenter itself could cross the upstream failure boundary; supporting a future
presenter-hide workflow requires revalidation or an upstream fix. Minimized and
occluded native windows are not equivalent to explicit Slint hide/recreation.
The move relies on equal logical font settings; simulated 2.0/1.0 scaling passed,
but physical mixed-DPI/font-environment qualification remains pending. This is
an application workaround, not a claim that registry Slint was repaired.

## Existing focus limitation and other observations

The native Bring Audience to Front test left the presenter focused. An immediate
Alt+F4 consequently closed the presenter in the Wayland test; the failure log is
preserved. XWayland's active-window assertion independently reproduced the
focus mismatch (`native-xwayland-close-focus.log`). This is not treated as a
successful audience-close test. Later tests explicitly verified actual focus
through the native switcher/image or X11 active-window ID before closing, and
those separate close/recreation checks passed.

`src/main.rs::bring_audience_window_to_front` delegates to
`window_controller::show_audience_window`. On Linux the latter calls `show()`
without an explicit native raise/focus operation for an already visible window.
The same implementation exists before migration (`23648de^`). Additionally,
winit's Wayland `focus_window()` is a no-op. Therefore adding a generic
`focus_window()` call alone would not solve native Wayland activation. This
pre-existing OS focus issue was not expanded into this migration workaround.
Using normal OS window selection or clicking the window recovers operation.

During initial step 5, the About dialog's fixed 54px license paragraph clipped
a final line. The follow-up reproduced the insufficient height on both 1.18.1
and 1.19.0 under matched text/font/width conditions, repaired the layout, and
completed native normal/minimum desktop checks. The original image remains
historical evidence; see the separate About report and patch.

Several retained failures are harness/environment diagnostics, not new app bugs:

- Initially locked desktop rejected temporary input sessions; user unlock fixed it.
- The first isolated control socket path exceeded Unix `SUN_LEN`; a short private
  runtime fixed it. Initial launcher/token observations are kept separately.
- File-path entry initially selected the PDF without confirming GNOME's Select
  button. Confirming that actual button completed the normal native open.
- GNOME keyboard resize initially chose left/top edges and enlarged the window.
  Selecting right/bottom edges first reached the verified 800×608 minimum.
- Early XWayland metadata repeats ran while the first test instance remained
  after the failed focus assertion. Repeat4/repeat5 were rerun with a cleanly
  closed predecessor and confirmed live CLI restoration; these are the independent
  lifecycle repeat evidence.
- The first helper-environment sample looked only at the main thread and found
  no children. The later all-thread sample found the real renderer and checked it.
- GNOME ScreenshotWindow/GetWindows/Eval/FocusApp API restrictions were retained;
  the supported temporary ScreenCast/input path was used instead.
- A status message to another chat was rejected by automatic approval review
  because explicit authorization to send that message was not established. No
  message was sent or retried. This did not block local validation.

## Validation commands and remaining work

Source changed, so the required Rust validation was rerun once after the fix:

```sh
cargo fmt --check
cargo check --all-targets --locked
PDFIUM_DYNAMIC_LIB_PATH=/tmp/qp-slint119-pdfium-20261117-1440-host/lib/libpdfium.so cargo test --locked
cargo build --locked --bins
cargo build --release --locked --bins
```

The full test suite has library 24, main binary 555 passed/2 ignored,
audience layout 1, new comment layout 1, control contract 5/events 3/IPC 8/
launch 12/lock 2, pure macOS tracker 3, presenter layout 1, renderer processes 15.
All other cfg-gated targets, qp binary, example and doc tests are 0. They do not
certify Windows/macOS native behavior. The two ignored Audience tests retain their
sustained-local-load and external-relay-fixture reasons from step 4.
The unchanged Python suites, dependency audit and no-default-features suite use
step-4 results; no successful check was repeated just for a separate log.

To finish full desktop qualification, connect a second physical display (with a
different actual scale for mixed DPI), and run the display-move/swap/fullscreen
and disconnect/recovery checklist on both native Wayland and the available X11
path. A separate native Xorg session is required for Xorg-specific claims.
Resolve or explicitly accept the existing Bring-to-Front focus limitation.
The separate About repair and its new unlocked-desktop confirmation are complete. Installed-package acceptance must be performed
without replacing the user's installation on this workstation, or on a disposable
host. None of these checks is converted to PASS by the software scale probe,
Xvfb, or automatic GUI success.

## Cleanup and final state

All test GUI process IDs recorded under `normal-*/pid` were checked stopped;
isolated control sockets were removed by normal exit. Temporary input/cast
sessions ended and original Codex X11 focus was requested. Test settings/fixtures
remain only in the evidence directory/short private runtimes. Existing installed
2.5.0 and repository PDFium 8076 remain unchanged. Temporary diagnostic examples
were removed from `examples/` after their sources were saved with the logs.
At validation completion, `cargo fmt --check` and whitespace checks passed.
HEAD was unchanged; only
the three repair files and this new report are additions for step 5. The earlier
step-4 documentation dirty state is preserved. No shared branch was pushed.

## About follow-up (2026-11-17, after the initial step-5 report)

The original About defect observation above is historical. The fixed-height
allocation was insufficient on both 1.18.1 and 1.19.0 in a matched text probe.
A separate About-only repair now uses content-based heights, a scrollable body
and an independent Close footer. See [the dedicated follow-up](SLINT_1_19_LINUX_ABOUT.md)
for the exact patch, updated binary, 630-pass suite and native snapshots. The
new compositor-pointer check completed after user unlock at normal and actual
800×608 minimum sizes; both full paragraphs and real Close actions passed.
Earlier release hashes and 215-check reports predate this addition; the dedicated
follow-up records the new release payload and its evidence.

## Authorized Linux transfer after validation

The results above describe the validation checkout and execution-time dirty
state. After validation, the user authorized a transfer branch based on
`deee3b6731e631c86309b8e681a2c3fd20afdaf6`:
`codex/slint-1-19-linux-validation`. It carries the comment-measurement and About
repairs, the comment-layout regression test, these three Linux reports, and
Linux links in the migration index. The existing migration/base branches are
not pushed or merged by this transfer. Raw logs, screenshots, payloads, private
configuration and the pre-existing user documents are excluded from the commit.
Artifact references in these reports remain local-only evidence paths; no
GitHub artifact upload is implied. No tests were rerun solely for the transfer.

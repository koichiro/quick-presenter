# GUI Release Smoke Checklist

Use this checklist before publishing a v1.0.0 release. The packaged artifact
smoke mode validates bundled PDFium lookup and first-page rendering without
creating Slint windows. This checklist covers the interactive presenter workflow
that must be checked in a real desktop session.

Run the checklist on the final release artifacts for each supported platform:

- macOS: `QuickPresenter-<version>.dmg` or `Quick Presenter.app`
- Windows x64: `QuickPresenter-<version>.msi`
- Ubuntu x64: `quick-presenter_<version>_amd64.deb`

Use `docs/samples/quick-presenter-demo.pdf` or
`tests/fixtures/marp-speaker-notes.pdf` as the known-good deck. Keep
`PDFIUM_DYNAMIC_LIB_PATH` and `QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE` unset
unless you are diagnosing a package layout failure.

## Release Gate

Do not publish the release until:

- `CI` passes on the release branch.
- `Build Binaries` passes and uploads all supported platform artifacts.
- The successful `Build Binaries` workflow run URL is recorded with the release
  checklist.
- The packaged artifact smoke tests pass for macOS, Windows, and Ubuntu Linux.
- This GUI smoke checklist passes on macOS, Windows, and Ubuntu Linux desktop
  sessions, or every failing item is documented in the release notes as a known
  release blocker or limitation.

Record the successful `Build Binaries` workflow run URL, tested artifact name,
app version, operating system version, display setup, and test PDF for each
platform.

## Semi-Automated Path

The first v1.0.0 gate can remain manual, but repeatable app-observable checks
can run through the packaged app's semi-automated GUI smoke mode:

```sh
quick-presenter --gui-smoke docs/samples/quick-presenter-demo.pdf \
  --gui-smoke-report /tmp/quick-presenter-gui-smoke.txt
```

Unlike `--smoke-open-pdf`, this mode creates the presenter and slide Slint
windows, opens the PDF in those windows, and drives the same Rust presentation
commands used by buttons and keyboard callbacks. It verifies app-observable
state after each step: opened document metadata, current page, next preview
availability, speaker-note handling, black-screen state, fullscreen state, and
presenter/slide synchronization. It writes a text report and exits non-zero on
failed checks.

A maintainer must still confirm OS-owned behavior that the app cannot reliably
assert itself: file-picker usability, launcher icons, taskbar or dock surfaces,
native focus behavior, and whether text remains visually readable on the tested
display.

The automated portion stays inside the existing Rust ownership boundary:
state transitions stay in Rust, Slint remains responsible for windows and
events, and platform-specific shell checks remain outside the core application.

Avoid making the first version depend on heavyweight cross-platform GUI drivers.
They are useful later for screenshots or accessibility assertions, but they can
make release validation more fragile than the behavior being tested. If external
drivers are introduced, keep them as platform-specific release scripts that run
against packaged artifacts after `Build Binaries`, not as normal unit tests.

## Shared Checks

Run these checks on every platform.

| Check | Expected result |
| --- | --- |
| Launch the packaged app from the normal platform entry point. | Quick Presenter starts without requiring a terminal or PDFium path. |
| Open the known-good PDF from the app UI. | The PDF opens without a presenter-visible error. |
| Confirm both windows are visible. | The presenter window and audience-facing slide window appear. |
| Press next-page controls until the end of the deck. | The current slide, next preview, page label, timer, clock, and notes stay readable. |
| Press previous-page controls until the start of the deck. | Both windows stay synchronized and page navigation clamps at page 1. |
| Use first-page and last-page controls. | Both windows jump to the expected page. |
| Use keyboard navigation in the presenter window. | Keys in `docs/KEYBOARD.md` update both windows. |
| Use keyboard navigation in the slide window. | Keys in `docs/KEYBOARD.md` update both windows when the slide window has focus. |
| Toggle slide fullscreen from presenter controls. | The slide window enters fullscreen and the presenter window remains usable. |
| Exit slide fullscreen from presenter controls. | The slide window returns to windowed mode. |
| Toggle slide fullscreen with `F5` or `F`. | The slide window enters or exits fullscreen from the focused window. |
| Press `Escape` while the slide window has focus and is fullscreen. | The slide window exits fullscreen without changing pages. |
| Toggle black screen mode with the UI and `B`. | The slide window blanks and restores while presenter controls remain usable. |
| Navigate while black screen mode is active, then restore. | The restored slide shows the current page after the hidden navigation. |
| Close the app from normal window controls. | The app exits without hanging windows or crash dialogs. |

## macOS

Test on a normal signed-in desktop session, not a headless CI session.

- Mount the DMG and launch `Quick Presenter.app` from the mounted volume or
  from `Applications`.
- If the build is unsigned or unnotarized, record any Gatekeeper prompt shown
  before continuing.
- Confirm the app name and icon appear in Cmd+Tab.
- Confirm `File > Open PDF...` opens the file picker and can load the test PDF.
- Confirm window menu actions for showing or focusing the presenter and slide
  windows work where present.
- Confirm fullscreen entry and exit do not strand the slide window on a hidden
  Space.

## Windows

Test from the installed MSI, not only from the raw `quick-presenter.exe` diagnostic artifact.

- Install `QuickPresenter-<version>.msi`.
- Launch `Quick Presenter` from the Start Menu.
- If the build is unsigned, record any SmartScreen or installer trust prompt
  shown before continuing.
- Confirm the app uses the Quick Presenter icon in the Start Menu, taskbar, and
  Alt+Tab.
- Confirm `File > Open PDF...` opens the file picker and can load the test PDF.
- Confirm window show, hide, or bring-to-front menu actions work where present.
- Confirm fullscreen entry and exit work with the slide window on the intended
  display.
- Uninstall the MSI after testing and confirm the normal uninstall path works.

## Ubuntu Linux

Test on the supported Ubuntu desktop target with a real display server session.

- Install the Debian package:

```sh
sudo apt-get install ./quick-presenter_<version>_amd64.deb
```

- Launch `Quick Presenter` from the desktop launcher.
- Confirm the app name and icon appear in launcher search, the app switcher, and
  the desktop environment's dock, taskbar, or panel.
- Confirm `File > Open PDF...` opens the file picker and can load the test PDF.
- Confirm window show, hide, or bring-to-front menu actions work where present.
- Confirm fullscreen entry and exit behave predictably under the tested desktop
  environment.
- Remove the package after testing:

```sh
sudo apt-get remove quick-presenter
```

## Failure Handling

Treat any of these as release blockers unless explicitly accepted in the release
notes:

- The packaged app cannot launch from the platform entry point.
- A supported packaged artifact cannot open the known-good PDF.
- Presenter and slide windows become unsynchronized.
- Fullscreen cannot be exited through a documented control path.
- Keyboard navigation stops working in both windows.
- Black screen mode cannot be restored.
- Presenter-facing status text, page labels, timer, clock, next preview, or
  notes become unreadable during the normal workflow.

For every failure, capture the platform, artifact name, exact PDF, reproduction
steps, expected behavior, actual behavior, and whether the issue reproduces from
the raw binary as well as from the packaged artifact.

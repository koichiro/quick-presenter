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

For notes-specific readability checks, use
`tests/fixtures/long-speaker-notes.pdf`. It opens directly on a page with
speaker notes long enough to overflow the presenter notes area at the minimum
supported presenter window size.

## Release Gate

Do not publish the release until:

- `CI` passes on the release branch.
- `Build Binaries` passes and uploads all supported platform artifacts.
- The successful `Build Binaries` workflow run URL is recorded with the release
  checklist.
- The packaged artifact smoke tests pass for macOS, Windows, and Ubuntu Linux.
- The uploaded `quick-presenter-ubuntu-x64-gui-smoke` report passes, and the
  macOS/Windows GUI smoke skip-reason reports are reviewed.
- This GUI smoke checklist passes on macOS, Windows, and Ubuntu Linux desktop
  sessions, or every failing item is documented in the release notes as a known
  release blocker or limitation.
- Manual GUI smoke reports for macOS, Windows, and Ubuntu Linux are attached to
  the GitHub release, or each missing platform report has an explicit waiver in
  the release notes.

Record the successful `Build Binaries` workflow run URL, tested artifact name,
app version, operating system version, display setup, and test PDF for each
platform.

## Manual Report Artifacts

Store completed manual GUI smoke reports with the GitHub release that contains
the tested binaries. Use one Markdown report per platform:

- `quick-presenter-<version>-macos-manual-gui-smoke.md`
- `quick-presenter-<version>-windows-x64-manual-gui-smoke.md`
- `quick-presenter-<version>-ubuntu-x64-manual-gui-smoke.md`

The CI-generated `quick-presenter-ubuntu-x64-gui-smoke` report and the
macOS/Windows skip-reason reports remain useful release evidence, but they do
not replace the manual desktop-session reports. If a platform cannot be tested
for a release, record the waiver in the release notes with the reason, impact,
and follow-up issue.

Each manual report must identify the exact tested artifact. Include the
following fields before the checklist results:

```markdown
# Quick Presenter Manual GUI Smoke Report

- Release: v1.0.0
- Platform: macOS | Windows x64 | Ubuntu x64
- Result: pass | fail | waived
- Tester:
- Test date:
- Build Binaries run:
- Artifact archive:
- Tested package file:
- Tested package SHA-256:
- Source commit or tag:
- App version:
- Signing/notarization state:
- Operating system version:
- Desktop environment/display server:
- Display setup:
- Test PDF:
- Semi-automated GUI smoke report:

## Checklist Results

- Shared checks:
- Focus and window recovery checks:
- Platform-specific checks:

## Failures, Waivers, and Notes

```

Use `shasum -a 256 <file>` on macOS or Linux, or
`Get-FileHash -Algorithm SHA256 <file>` on Windows, to record the tested package
hash. The hash should be for the final package file that a user installs or
opens, such as the DMG, MSI, or Debian package.

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

The `--gui-smoke` command opens the PDF through the same asynchronous render
scheduler used by production startup and file-open flows, then drains render
events until the first visible slide render and speaker-note check are reflected
in app state. After that async open/render check passes, it drives the normal
presentation commands against the resulting windows.

`Build Binaries` runs this command from the installed Ubuntu package under Xvfb
and uploads the text report as `quick-presenter-ubuntu-x64-gui-smoke`. The same
workflow uploads explicit skip-reason reports for macOS and Windows because
GitHub-hosted runners do not provide the normal desktop sessions needed to make
platform GUI behavior a reliable release gate.

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
| Open `tests/fixtures/long-speaker-notes.pdf` at the minimum presenter window size. | The notes area scrolls so the full speaker note can be read without resizing the app, while the current slide, next preview, and thumbnails remain usable. |
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
| Confirm the Window menu does not offer a presenter-hide action. | The presenter controls cannot be hidden from the app menu. |
| Hide the slide window from the Window menu, then show it again. | The presenter window remains visible and can recover the slide window. |
| Bring the presenter window to front from the Window menu after focusing or covering it with the slide window. | The presenter window becomes usable again without changing the current slide, fullscreen state, or black-screen state. |
| Bring the slide window to front from the Window menu after focusing or covering it with the presenter window. | The slide window becomes usable again and stays synchronized with the presenter window. |
| Move focus between the presenter window, slide window, and another application, then return through the normal app switcher or Window menu. | Keyboard navigation still works in the focused Quick Presenter window, and presenter controls remain reachable. |
| Close the app from normal window controls. | The app exits without hanging windows or crash dialogs. |

## Focus and Window Recovery Checks

Run these focused recovery scenarios on every platform after opening the
known-good PDF. They cover the highest-risk live-presentation paths where
operating-system focus, fullscreen, hiding, or app switching can interrupt the
normal presenter workflow.

| Scenario | Expected result |
| --- | --- |
| Give focus to the presenter window and press next, previous, first, and last keyboard shortcuts from `docs/KEYBOARD.md`. | Both windows update together, and page navigation clamps at the document bounds. |
| Open a PDF from the file picker, then immediately press presenter keyboard navigation shortcuts without clicking either Quick Presenter window. | The presenter window receives keyboard input and both windows advance or go back as expected. |
| Open a PDF from the Recent PDF menu, then immediately press presenter keyboard navigation shortcuts without clicking either Quick Presenter window. | The presenter window receives keyboard input and both windows advance or go back as expected. |
| Give focus to the slide window and press next, previous, first, and last keyboard shortcuts from `docs/KEYBOARD.md`. | Both windows update together even when the audience-facing window has focus. |
| Enter slide fullscreen from the presenter controls, switch focus away from Quick Presenter, then return to Quick Presenter. | The presenter window remains reachable, and the slide window stays fullscreen until a documented exit control is used. |
| Enter slide fullscreen from the slide window with `F5` or `F`, then exit with `Escape`. | The slide window returns to windowed mode without changing pages. |
| Toggle black screen, switch focus away from Quick Presenter, return, then toggle black screen off. | The slide window restores to the current page and both windows remain synchronized. |
| Hide the slide window, continue navigating from the presenter window, then show or bring the slide window to front. | The recovered slide window shows the current page, not the page that was visible before hiding. |
| Cover or background each Quick Presenter window, then use the Window menu to bring the presenter and slide windows back to front. | Each window can be recovered without reopening the PDF or restarting the app. |
| After accidental focus changes, recover to a normal presenter state: presenter window visible, slide window visible or fullscreen, keyboard navigation working in the focused Quick Presenter window. | The presenter can continue the deck without losing page position, black-screen state, or fullscreen state. |

## Accessibility Checks

Run the full keyboard path and inspect the accessibility tree on every platform
where the relevant assistive technology is available. Follow the baseline and
recording requirements in [ACCESSIBILITY.md](ACCESSIBILITY.md).

| Check | Expected result |
| --- | --- |
| Starting with no PDF open, use only the keyboard to open the File menu and file picker, then open the known-good PDF. | The PDF opens and focus returns to the presenter window's presentation shortcut scope without requiring a pointer click; the thumbnail pane is not focused until the user navigates to it. |
| Use `Tab` and `Shift+Tab` through presenter controls. | Each focused custom control has a visible focus border, and focus does not become trapped or disappear. |
| With an inline menu heading focused, press `Return` or `Space`, then activate an enabled menu item from the keyboard. | The menu opens and the requested action runs once. Disabled recent-file items cannot be activated. |
| Focus the thumbnail pane, then press the up and down arrow keys repeatedly in both directions. | Every key press selects the adjacent slide, the pane retains focus after each model refresh, both windows stay synchronized, and the visible `CURRENT` label follows the selected slide. |
| Inspect custom menu headings, menu items, the thumbnail list, and thumbnail items with the platform accessibility inspector or screen reader. | Controls have meaningful roles and names; enabled, expanded, item-count, and selected/current states are exposed where supported. |
| Read the speaker notes with the platform screen reader. | The notes region is announced as `Speaker notes`, and the current note or `No speaker notes` is available. |
| Complete previous, next, first, last, fullscreen, and black-screen actions without a mouse. | The primary presentation workflow remains operable and both windows stay synchronized. |
| Repeat the shared presenter workflow at 125%, 150%, and 200% display scaling. | Text, focus borders, current-state labels, menus, thumbnails, notes, and dialogs remain visible and usable without overlap or clipping. |

Use Windows Narrator and Accessibility Insights, macOS VoiceOver and
Accessibility Inspector, and Linux Orca over AT-SPI where feasible. Record any
Slint backend or platform limitation rather than treating an untested state as
verified.

## macOS

Test on a normal signed-in desktop session, not a headless CI session.

- Mount the DMG and launch `Quick Presenter.app` from the mounted volume or
  from `Applications`.
- If the build is unsigned or unnotarized, record any Gatekeeper prompt shown
  before continuing.
- Confirm the app name and icon appear in Cmd+Tab.
- Confirm `File > Open PDF...` opens the file picker and can load the test PDF.
- Confirm window menu actions for showing or focusing the presenter and slide
  windows work where present, and that the presenter window cannot be hidden
  from the app menu.
- Confirm fullscreen entry and exit do not strand the slide window on a hidden
  Space.
- Record any Space, Dock, or Cmd+Tab behavior that changes how the presenter
  window is recovered while the slide window is fullscreen.

## Windows

Test from the installed MSI, not only from the raw `quick-presenter.exe` diagnostic artifact.

- Install `QuickPresenter-<version>.msi`.
- Launch `Quick Presenter` from the Start Menu.
- If the build is unsigned, record any SmartScreen or installer trust prompt
  shown before continuing.
- Confirm the app uses the Quick Presenter icon in the Start Menu, taskbar, and
  Alt+Tab.
- Confirm `File > Open PDF...` opens the file picker and can load the test PDF.
- Confirm window show, hide, or bring-to-front menu actions work where present,
  and that the presenter window cannot be hidden from the app menu.
- Confirm fullscreen entry and exit work with the slide window on the intended
  display.
- Record any taskbar, Alt+Tab, or display-selection behavior that changes how
  the presenter and slide windows are recovered after focus moves away.
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
- Confirm Quick Presenter appears as an available PDF handler in the desktop
  environment's "Open With" UI, without becoming the default PDF viewer unless
  the tester explicitly chooses it.
- Open the test PDF through the desktop environment's "Open With" UI and confirm
  it loads in Quick Presenter.
- Confirm `File > Open PDF...` opens the file picker and can load the test PDF.
- Confirm window show, hide, or bring-to-front menu actions work where present,
  and that the presenter window cannot be hidden from the app menu.
- Confirm fullscreen entry and exit behave predictably under the tested desktop
  environment.
- Record the desktop environment and display server when focus, app-switcher,
  or fullscreen behavior differs between GNOME, X11, Wayland, or other tested
  sessions.
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

# GUI Release Smoke Checklist

Use this checklist before publishing a v1.0.0 release. The packaged artifact
smoke mode validates bundled PDFium lookup and first-page rendering without
creating Slint windows. This checklist covers the interactive presenter workflow
that must be checked in a real desktop session.

Run the checklist on the final release artifacts for each supported platform:

- macOS: `QuickPresenter-<version>.dmg` or `Quick Presenter.app`
- Windows x64: the Microsoft Store-installed app (Store ID `9N913S9NJ6D1`)
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
- The macOS DMG is Developer ID signed, Apple-notarized, stapled, and accepted
  by Gatekeeper without an unidentified-developer warning.
- The existing public Microsoft Store listing is reachable and its current
  public package can be installed from the Store.
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

After the GitHub release, update the Microsoft Store package to v1.0.0, wait
for certification, and rerun this checklist against the Store-installed v1.0.0
app. Record that post-release result separately; a direct MSI or MSIX does not
replace it.

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
- Distribution channel:
- Store ID/package identity (Windows):
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

Use `shasum -a 256 <file>` on macOS or Linux to record the final DMG or Debian
package hash. For a Microsoft Store installation, set the package file and hash
fields to `N/A (Microsoft Store)` and record Store ID `9N913S9NJ6D1`, the
installed package identity, and the installed app version.

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
| At 800 × 560, open a page with a short note, then a page with a longer note. Repeat with Japanese text and explicit newlines. | Short notes use up to 24px; longer notes select the largest fitting size down to 12px without user input. Current slide, next preview, and thumbnails remain usable. |
| With `long-speaker-notes.pdf` at 800 × 560, scroll to the end and resize the presenter window larger, then smaller. | Overflow remains scrollable at 12px. Text adjusts automatically within 12–24px, and the scroll offset stays inside the new range without an empty viewport beyond the end. |
| Open a portrait PDF, switch to a no-notes page, open another PDF, and trigger a PDF reload. Repeat with the platform's menu layout. | Each note is fitted to its own viewport. The **No notes** placeholder stays at 12px. No preference or text-size control is needed. |
| After resizing or changing pages, press Space, Left, Right, and the existing presentation shortcuts. | Presentation keyboard controls still work. Automatic sizing does not change the page, timer, audience slide, or black-screen state. |
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
| In extended-desktop mode, place the presenter and slide windows on different displays, focus the presenter, and press `X`. | The windows exchange displays without changing their roles, current page, timer, or black-screen state. |
| Press `X` again from the focused slide window. | The original display assignment is restored and the next keyboard navigation command works without an extra click. |
| Press `X` while the slide window is fullscreen on another display. | The slide remains fullscreen on the destination display and the presenter window moves to the former slide display without a visible windowed transition. |
| Press `X` with one display or both windows on the same display. | Neither window moves or becomes unreachable, and the presenter shows a short explanation. |
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

## macOS

Test on a normal signed-in desktop session, not a headless CI session.

- Mount the DMG and launch `Quick Presenter.app` from the mounted volume or
  from `Applications`.
- Confirm Gatekeeper accepts the signed, notarized, and stapled release without
  an unidentified-developer warning. Treat a trust prompt as a release blocker.
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

The existing public Store version may be used to confirm channel availability
before the GitHub release. After publishing the GitHub release, rerun this
section against the Microsoft Store-installed v1.0.0 app. A direct MSI, MSIX,
or raw `quick-presenter.exe` test does not satisfy the Windows trust check.

- Install Quick Presenter from Microsoft Store ID `9N913S9NJ6D1`.
- Launch `Quick Presenter` from the Start Menu.
- Confirm the installed app reports the intended release version and launches
  without an untrusted-publisher or SmartScreen warning.
- Confirm the app uses the Quick Presenter icon in the Start Menu, taskbar, and
  Alt+Tab.
- Confirm `File > Open PDF...` opens the file picker and can load the test PDF.
- Confirm window show, hide, or bring-to-front menu actions work where present,
  and that the presenter window cannot be hidden from the app menu.
- Confirm fullscreen entry and exit work with the slide window on the intended
  display.
- Record any taskbar, Alt+Tab, or display-selection behavior that changes how
  the presenter and slide windows are recovered after focus moves away.
- Uninstall the app through Windows Settings after testing and confirm the
  normal uninstall path works.

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

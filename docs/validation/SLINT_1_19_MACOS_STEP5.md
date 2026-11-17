# Slint 1.19.0: macOS native step-5 validation

## Scope and identity

Date: 2026-11-17, Asia/Tokyo (UTC+09:00); native operations around 16:02–16:14,
then resumed and completed at 17:02–17:06 after another user unlock. This report records this session's
native desktop operations, separately from step-4 tests and automated GUI smoke.
The working tree started at `deee3b6731e631c86309b8e681a2c3fd20afdaf6` on
`codex/slint-1-19-migration`, with three untracked step-4/report assignment files.
Those files were preserved. No source, dependency, vendor or lockfile changes
were made for this validation.

- macOS 27.0.1 (26A434), arm64, Apple M6 GPU.
- Rust/Cargo 1.100.0. Native `aarch64-apple-darwin` debug binaries, default
  features, built with `cargo build --locked --bins` (exit 0).
- Slint 1.19.0, edition 1.18, unchanged local winit 0.30.13 override.
- Explicit backend `SLINT_BACKEND=winit-femtovg`, native AppKit desktop.
- One online Dell U4021QW, 5120x2160 at 60 Hz, no mirroring; saved native
  placement records report scale 1.0. No second display or mixed-DPI configuration.
- PDFium 158.0.8111.0 arm64, pinned manifest asset obtained during step 4;
  development override `/tmp/slint119-step4/pdfium/lib/libpdfium.dylib`.
- Normal native operations used a temporary flat development app at
  `/tmp/qp-slint119-flat/Quick Presenter Native119.app`, identifier
  `org.quickpresenter.slint119.flatstep5`, whose executable is a symlink to the
  unchanged debug binary. This lets CUA identify this process separately from
  the user's already-running older test application. It retains the supported
  unbundled debug PDF renderer path; it is not a release-layout bundle.
- Native keyboard and mouse input was injected through CUA, with AX state and
  screenshots observed after actions. This is actual desktop input, not physical
  keyboard evidence or direct invocation of Rust callbacks.

The first ordinary raw debug launch rendered the PDF but could not be selected
by CUA as an app. A standard `Contents/MacOS` symlink wrapper then exposed the
native menu, but correctly rejected PDF playback because it had no signing Team
ID. Neither protection nor code was changed. The subsequent flat debug wrapper
successfully rendered PDFs. No Developer ID signing, notarization, Store sandbox
or release/package approval is asserted by these results.

## Isolation and evidence

Evidence root (gitignored, local only):
`artifacts/slint-1-19-macos-step5-20261117/`.
`provenance.json` records initial HEAD/status and debug binary SHA-256 hashes.
`displays-host.json` records the display enumeration; `build.log` records the
build. `native-observations.json` records individual native observations, while
`flat.log`, subsequent normal-launch logs and CLI JSON files supply state evidence.
Native screenshots are also present in this chat's CUA tool transcript (audience
Sample 2 after reveal, Sample 3 after native keys, Japanese notes, compact Settings,
long-note beginning/end, and thumbnail-driven Japanese note selection).

`XDG_RUNTIME_DIR=/tmp/qp-slint119-macos-step5-runtime` (mode 0700) isolated the
control socket. All test audience settings used an explicit artifact-local
`--config audience.toml`; no user relay configuration was reset. Disposable PDF
copies under the evidence root supplied six recent-file identities and the long
notes fixture. Only those copies were modified for hot reload.

The normal GUI has no separate startup/recent path option on macOS. Before test
startup, `startup-state.json` and `recent-files.txt` in the usual application
support directory were copied with metadata into `original-persistence/`, their
hashes recorded, and the active test versions started empty. The user's existing
older test process was left untouched. Cleanup/restoration status is recorded
below; backup contents are private local evidence and must not be committed.

## Native desktop observations

Result terms: PASS means the stated observation was verified; PASS WITH LIMIT
means only the explicitly described single-display/input scope; BLOCKED refers
to unavailable physical display configurations. Automated callback/geometry
checks are labeled separately from native menu and injected-key operations.

| Check | Result | Evidence / boundary |
| --- | --- | --- |
| Application-menu roles | PASS | Native app menus with presenter and audience focused contained About Quick Presenter, Settings and custom Quit Quick Presenter. File/Window actions remained in their own menus; no presenter-hide item. |
| Presenter Settings and Command-comma | PASS | Native Settings item opened the panel; Close then Command-comma reopened it. Compact 800px-wide screenshot showed accessible Close and import/reset controls. |
| Audience Settings/About forwarding | PASS | Both native audience menu actions revealed the presenter and displayed Settings/About. About reported version 3.0.0 and PDFium 158.0.8111.0. |
| Native PDF file picker | PASS | File > Open PDF opened NSOpenPanel; Go to Folder selected the disposable long-notes PDF. Once Ready, Right advanced without another click. An input sent during asynchronous loading did not advance; no claim of navigation before readiness. |
| Presenter native navigation and bounds | PASS | Right moved 1->2; End+Right remained 3/3; Home+Left remained 1/3; repeated Right/Left/Home stayed synchronized. |
| Audience native navigation | PASS | Home/Right and later Right updated the real audience surface; screenshots showed Sample 2 and then Sample 3 with Japanese content. |
| Black screen while navigating | PASS | B, Right produced page 2 with blackout=true and ready rendering in `blackout-navigation.json`; B restored current slide. |
| Hide/update/show and stale frame | PASS | Native Window > Hide Audience, presenter Right, Window > Show Audience displayed Sample 2 rather than the pre-hide page. Screenshot captured the recovered audience surface. |
| Audience native close/recovery | PASS | Closing the audience's native close button retained the presenter; Window > Show Audience reopened it with current content. |
| Native window recovery after app focus change | PASS | Activated Finder, raised presenter and pressed Right; page 1->2 worked. This does not certify every Dock/Space/Cmd+Tab scenario. |
| Open Recent rebuild/selection | PASS | `recent-stress.jsonl` records 12 sequential CLI opens with ready rendering. Native menu showed the five current recents in correct order; selecting Recent 02 opened exactly Recent 02, then Right worked. This exercises normal rebuilt menus, not deliberately retained stale native item objects. |
| Long notes and scrolling | PASS | Compact 800px presenter wrapped long English notes while previews/thumbnail strip remained visible. Coordinate scroll in notes reached “End of the long speaker note.” |
| Japanese notes and thumbnails | PASS | Clicking the next thumbnail replaced overflowing notes with large readable Japanese notes at the top, without retaining an empty overscrolled region. Long Japanese multiline wrapping is covered separately by the automated fixture; no new physical readability claim. |
| Automatic PDF reload | PASS | Atomically replaced the active disposable 2-page PDF with the 3-page fixture. `reload-status.json` records document revision 17, retained page 2, pages=3 and render ready; subsequent AX showed Presenter note text. The initial status sampled notes while still loading. |
| Native fullscreen and input | PASS WITH LIMIT | Presenter F/Right/Escape ended page 3, fullscreen=false (`fullscreen-rapid-status.json`). Settled F showed Exit Fullscreen; Right/X/Escape returned page 2 windowed. Render logs changed physical width 1024->5120->1024. CUA screen capture returned transient ScreenCaptureKit -3811 errors during transitions; subsequent AX recovered. The subsequent native notification suite below passed; capture errors were automation transport failures, not application crashes. |
| Single-display swap shortcut | PASS WITH LIMIT | X was exercised windowed/fullscreen; windows remained reachable and navigation continued. No actual cross-display exchange can be certified on this setup. |
| Presenter native Quit menu | PASS | Original test PID exited, startup JSON retained active test PDF and placement (`presenter-menu-quit-startup.json`). |
| Presenter Command-Q | PASS | With a valid loaded PDF, Right then Command-Q exited the test PID and persisted the active PDF (`presenter-command-q-startup.json`). An earlier no-PDF wrapper also saved startup state. |
| Audience Command-Q | PASS | With audience focused on Sample 3, Command-Q exited the original test PID; saved test PDF and normal 1024x576 placement at scale 1 remained (`audience-command-q-startup.json`). |
| Normal restoration | PASS | Dedicated environment relaunch without `--pdf` opened the remembered PDF at page 1/3, Ready, windowed; repeated normal launches retained the same normal placement record. |
| Audience native Quit menu | PASS | After verified unlock, native Audience app-menu Quit exited PID 87213 and saved Recent 03 and placement (`audience-menu-quit-startup.json`); normal relaunch restored page 1/3. |
| Presenter native close and fullscreen quit/restore | PASS | Presenter native close exited PID 89778 and saved state. Native F, then Right/Command-Q exited PID 91092 while fullscreen; JSON retained normal 1024x576. Relaunch PID 93252 showed page 1/3, Ready and Fullscreen (windowed) button, then normal close (`presenter-close-startup.json`, `fullscreen-command-q-startup.json`). |
| Flowing/bubble comments and full native transition notification suite | PASS (AUTOMATED) | Fresh complete native GUI smoke after unlock: exit 0, 323 passed, 0 failed, 50.6 seconds. Includes AppKit baseline/will/did notifications, rapid entry/exit cancellation, both comment styles, five long bubbles, Japanese/English content, pause/resume, history, notes fitting and hidden-window rendered edges. These are separate from native menu/key observations (`final-gui-report.txt`). |

After one Audience Command-Q, an immediate CUA `getAXState` automatically
restarted a helper process without the dedicated PDFium environment. It showed
“Could not reopen the previous PDF”; the original PID had already exited and
its correct saved record was preserved. That helper process alone was terminated
without persistence, then dedicated normal relaunch restored the PDF correctly.
This is an automation-launch environment difference, not evidence of a
migration startup-restoration defect. Later quit checks avoided automatic
post-quit AX relaunch and verified exit through the process table.

## Remaining physical/distribution gates

| Check | Status | Required condition |
| --- | --- | --- |
| Actual multi-display swap including fullscreen destination | BLOCKED | Connect a second extended-desktop display and repeat X from both windows, including fullscreen. |
| Mixed DPI, display removal/rearrangement and recovery | BLOCKED | A second display with a different scale plus physical arrangement/disconnection actions. Existing single-display scale-1 evidence cannot substitute. |
| Physical keyboard comparison | NOT REQUIRED | Native injected keys are valid step-5 input evidence. Physical-key comparison is only a boundary, not an additional completion gate. |
| Signed distribution/Store/sandboxed document drops and Gatekeeper | NOT APPLICABLE | Separate step-6/distribution qualification; no signing or installation changes requested here. |
| LAN/mobile participant certification | NOT APPLICABLE | Audience release qualification; this migration run uses loopback automation separately. |

No migration bug requiring a code change was confirmed by the completed native
checks. No full Rust test rerun was needed: there were no code changes, and
step-4 evidence remains separate from this native execution.

## Final automated run

Executed on the unlocked native desktop, using the same unchanged debug binary
whose hashes are in `provenance.json`:

```sh
PDFIUM_DYNAMIC_LIB_PATH=/tmp/slint119-step4/pdfium/lib/libpdfium.dylib \
SLINT_BACKEND=winit-femtovg \
XDG_RUNTIME_DIR=/tmp/qp-slint119-macos-step5-runtime \
python3 scripts/run_bounded_command.py --timeout 300 --label step5-final-gui \
  --log-dir artifacts/slint-1-19-macos-step5-20261117/final-gui-logs -- \
  target/debug/quick-presenter --gui-smoke tests/fixtures/marp-speaker-notes.pdf \
  --gui-smoke-report artifacts/slint-1-19-macos-step5-20261117/final-gui-report.txt
```

Result: **323 passed, 0 failed; exit 0; 50.6 seconds**. `final-gui.stdout`,
`final-gui.stderr`, `final-gui-command.json` and `final-gui-exit.txt` preserve the
command/result. macOS emitted one IMKCFRunLoopWakeUpReliable mach-port diagnostic;
the suite completed successfully. No previous CI or step-4 result is counted as
this execution.

## Final run and cleanup

The desktop initially required the user to unlock it, which they did. Later it
re-locked during the Audience menu-quit check. The second unlock request was initially pending, so the first run was safely
cleaned up before resumption. No attempt was made to bypass the lock or
change auto-lock/security settings. The user subsequently confirmed another unlock, which CUA verified. The
remaining native checks and complete GUI smoke were then executed successfully.
**All step-5 checks executable on the available single-display Mac are complete.** Multiple-display work needs
the additional hardware specified above; it does not block the completed
single-display results.

Cleanup terminated only the dedicated test process group (PID 86389), then
restored both original startup/recent files and verified their recorded SHA-256
hashes. `cleanup-result.json` records restoration and final HEAD/status. The
user's pre-existing older application (PID 1878) was not quit or altered, and the
application's normal audience config was not reset. Temporary test configuration,
fixture copies and logs remain in the gitignored evidence directory for review.
After the final successful run, the separate `resume-original-persistence/`
backup was restored with SHA-256 verification; `resume-cleanup-result.json`
records the resumed cleanup, unchanged binary hashes, HEAD/status and the
untouched older application. All resumed normal test processes exited through
their native close/quit controls; the final GUI smoke exited itself.
No installed app, OS preference, global HOME value or display setting was changed.

Only this Mac-specific report was added to repository documentation for step 5;
the shared migration index and other OS reports were left untouched. No commits,
pushes, releases or signing actions were performed.

# Slint 1.19.0 Windows step-5 desktop validation

## Outcome

The available single-display development-build desktop checks are complete.
After the work-area follow-up: **30 PASS, 0 FAIL, 3 BLOCKED, 2 NOT APPLICABLE**.
The initial snapshot was 29 PASS, 1 FAIL, 4 BLOCKED, 2 NOT APPLICABLE. This is
not installed-Store or full multi-display release certification.

Native menus, injected real keyboard/mouse input, actual window visibility and
geometry, normal startup persistence, PDF recovery, thumbnails, notes scrolling,
and a real local Audience Live session were exercised. The existing 208-check
step-4 GUI results are supporting evidence, not substitutes for these actions.
The initial desktop run changed no application source. The follow-up below fixes
F01 with a separate source patch and repeats the affected desktop checks. B04 was
an additional test beyond the assignment and is removed as a gate; the required
own-window native-switcher behavior passed. Physical display limits remain.

## Tested build and environment

- Initial date/time: 2026-11-17, approximately 15:51–17:19 JST (Asia/Tokyo, UTC+09:00).
  The raw ledgers contain exact timestamps; some UI metadata uses UTC.
- HEAD: `deee3b6731e631c86309b8e681a2c3fd20afdaf6`, branch
  `codex/slint-1-19-migration`.
- Dirty state at entry: the step-4 typography fixture/diagnostics change in
  `src/gui_smoke.rs`, its regression test in `src/notes.rs`, the existing migration
  index edit, and untracked validation material including the step-4 report.
  Those changes were preserved. They are not committed or pushed.
- Initial build: native x64 MSVC, unsigned development `target/debug/quick-presenter.exe`,
  app version 3.0.0, built with the existing test profile (`debug=1`).
  GUI executable SHA-256:
  `01f32abd342145d134d9b2dbba9103a9b77a23d09172ef24cf8d22d17c476164`.
  Companion `qp.exe` SHA-256:
  `d7646674b83ab7f4dfb73f99d4b976da5388b23233672bf0f683f5e06266ea17`.
- Windows 11 Pro 25H2, NT build 26200.9974, x64; Intel i5-7300U, 2 cores / 4
  logical processors. Rust/Cargo 1.99.1; Python 3.12.14. Full preparation details
  remain in [the step-4 report](SLINT_1_19_WINDOWS_RESULTS.md).
- One physical display, 1920x1080, system/window DPI 120 (125%), session 1.
  No system DPI, display topology, security, firewall, or installed package setting
  was changed. The real minimum presenter client measured 1000x760 physical /
  **800x608 logical**; the collapsed Audience pane determines that current minimum.
- Backend explicitly selected by child environment: `SLINT_BACKEND=winit-femtovg`.
  No step-5 scale override. The step-4 process-only 100% and native 125% results
  remain separate from physical monitor/DPI-transition validation.
- PDFium: existing pinned x64 chromium/8111 test DLL, version 158.0.8111.0, using
  a development-only `PDFIUM_DYNAMIC_LIB_PATH`. This is not a packaged-library test.

Normal GUI launches used `--config <run>/settings/config.toml` and `--log-file`,
with APPDATA and LOCALAPPDATA scoped to `<run>/settings/`. PDF copies were kept in
the run directory. No existing installed app or personal app configuration was
used or replaced. The initial explicit configuration path did not exist and
correctly showed a configuration error; a valid local configuration was created
for the next normal launch. Bubble/flowing preferences changed only in that
isolated file. The source PDF temporarily moved for the missing-remembered-file
test was restored afterward.

## Methods and evidence

Evidence directory:
[`artifacts/slint-1-19-windows-step5-20261117-155102/`](../../artifacts/slint-1-19-windows-step5-20261117-155102/).
It is ignored by repository policy and retained locally; screenshots were not
uploaded. The task-switcher screenshot includes unrelated window thumbnails and
should remain local unless reviewed for sharing.

- `actions.jsonl`: CLI arguments, stdout/stderr, exit codes, launch PID and
  environment, timestamps, executable hashes, and scenario results.
- Numbered `.txt`, `-metadata.json`, and `.png` files: native UI observations and
  selected screenshots. Inputs used the Computer Use `@oai/sky` SendInput/UIA APIs.
- `ui-actions.jsonl`, `*-keys.json`: actual keyboard actions and bounded series,
  with UI observations after each input. Intermediate UIA text can lag rendering;
  stable state, final page bounds, CLI status, and screenshots were corroborated.
- `*-win32.json`: read-only public Win32 observations: visibility, foreground,
  hung-window flag, bounds, client size, DPI, and (later) GUI-thread menu flags.
  These scripts never injected input or changed window geometry.
- `app-events.log`, `normal-gui.log`: production-path logs.
- `recent-stress.*`, `reload-errors-notes.*`, `local-comments.jsonl`,
  `comments-complete.*`: native process/API test evidence and exit statuses.
- `quit-fullscreen-startup-state.json`, `quit-long-notes-startup-state.json`,
  `missing-reopen-cleared-startup-state.json`: actual normal-exit persistence.
- `before-*`, `after-*`, `source-patch-metadata.json`, and
  `slint119-windows-source.patch`: source identity, preservation and integration.

The app's actual menus and file picker were used. CLI calls only supplied
state corroboration, repeated PDF opens, and file/error scenarios; they are not
described as physical keyboard or native-menu evidence. Input APIs activate the
target window, so post-dialog/unassisted OS-focus claims are limited accordingly.

## Checklist

| ID | Status | Check and observed evidence |
| --- | --- | --- |
| P01 | PASS | Normal development launch creates Presenter and Audience; independent settings, no smoke flag. `01-normal-start`, launch ledger. |
| P02 | PASS | Native File > Open PDF loads deck A; status reports page 1/12, render/notes ready. `02-file-menu`, `03-file-picker`, `05-file-picker-accepted`. The initial slash-separated file-picker input was corrected to a Windows path. |
| P03 | PASS | Presenter Right, End, Home, Left advance/jump/clamp at 1 and 12. `06`–`10`. |
| P04 | PASS | Audience-focused Right advances the same presentation; its B/F5/Escape controls work. `23`–`27`, matching CLI state. |
| P05 | PASS | Bounded actual key series: 16 Right followed by 16 Left reaches/clamps at both ends. `64-navigation-burst-keys.json`; this is an observed paced burst, not certification of arbitrary hardware repeat rates. |
| P06 | PASS | Ctrl+E and Ctrl+A reach last/first from normal controls. `65-fullscreen-keys-and-single-display-swap-keys.json`. |
| P07 | PASS | F5 enters actual borderless fullscreen: Audience bounds [0,0,1920,1080], CLI fullscreen=true. `23-audience-F5-fullscreen`, `audience-fullscreen-win32.json`. |
| P08 | PASS | F5 repeated, F, Escape, and navigation around toggles preserve usable input and finish windowed. `65` sequence and `after-fullscreen-cycle-win32.json`. This is Windows borderless-transition evidence, not AppKit-style asynchronous transition certification. |
| P09 | PASS | B blanks the actual Audience; Right changes to page 3 while black; B restores visible page 3. `25`, `27`, CLI blackout/page state. |
| P10 | PASS | X on the single display leaves both window positions unchanged and shows "Connect a second display before switching screens." `66`, before/after bounds. |
| P11 | PASS | Native Window menu exposes Show/Hide/Bring Audience and Show/Bring Presenter, with no Presenter-hide item. `67-window-menu`. |
| P12 | PASS | Hide Audience, End to page 12 while hidden, then Show restores the actual page-12 image. `audience-hidden-win32.json`, `69`, `70`. |
| P13 | PASS | Audience titlebar Close preserves Presenter; Bring Audience Window to Front restores visibility/foreground. `71`, `bring-audience-front-win32.json`. |
| P14 | PASS | Presenter can be raised over its fullscreen Audience and remains usable; minimize/activation recovery also worked. `29`, `85`. Activation here was explicitly tool-assisted. |
| P15 | PASS | Native Application menu places Settings and Quit in their declared Windows locations. Settings opens; Space does not navigate the underlying PDF. `11`–`14`, CLI page 1/12 unchanged. |
| P16 | PASS | Help > About opens Version 3.0.0 and PDFium 158.0.8111.0; Close returns to the deck. `15`, `16`. |
| P17 | PASS | Application > Quit while fullscreen closes both windows and saves the PDF plus normal Audience placement. `30`, `quit-fullscreen-startup-state.json`, process exit observation. |
| P18 | PASS | Normal Alt+F4 Presenter close also exits cleanly and persists state. Subsequent normal-launch records and final shutdown confirm completion. |
| P19 | PASS | No-PDF-argument relaunch restores deck A at page 1, windowed, with saved Audience bounds [225,175,1523,942] and Presenter foreground. `31`, `restored-startup-win32.json`. |
| P20 | PASS | Missing remembered PDF gives a short reopen error, document=null and no automatic recent fallback. Normal quit clears last_pdf; next launch remains empty even after the fixture is restored, then valid deck B opens ready. `90`, cleared startup JSON, final CLI ledger. |
| P21 | PASS | OS Size keyboard actions reach minimum client 800x608 logical. Current minimum is 608, not the older checklist's 560, because of the collapsed Audience footer. `notes-minimum-client-win32.json`. |
| P22 | PASS | Settings at the actual minimum keeps Close visible and operable; flowing/bubble controls can be used. `56-settings-minimum`, `57`. |
| P23 | PASS | Japanese notes wrap and remain readable; short Japanese notes return to the top after switching from scrolled long notes. `06`, `77`, `82-thumbnail-short-note`. Exact 12–24px fitting measurements are supporting step-4 evidence, not inferred from screenshot pixels. |
| P24 | PASS | Long notes scroll from their beginning to "End of the long speaker note" without changing page 1/2 or hiding current/next/thumbnail content. `80`, `81`. |
| P25 | PASS | Actual page-2 thumbnail click selects 2/2 and the short Japanese note. `82`; the next preview correctly becomes empty at the end. |
| P26 | PASS | Maximize, long-note navigation, minimize/recover, and OS Restore return to the previous compact layout without losing the deck. `83`–`87`. Caption input had a tool targeting issue; OS menu Restore was the verified route. |
| P27 | PASS | Five alternating CLI opens update Recent. Native submenu shows B/A IDs 1032/1033; choosing A loads 12 pages. Rebuilt A/B IDs 1034/1035 then select B with 3 pages. No wrong-item activation was observed in this bounded sequence. `74`, `78`, ledger. Synthetic retained-item fault injection was not performed. |
| P28 | PASS | Replace the active test PDF from 3 to 12 pages: watcher reloads, revision increases, ready state returns. Failed invalid/missing opens retain the active PDF, and opening long notes afterward succeeds. `reload-errors-notes.*`; focused invalid retry also retains deck B. |
| P29 | PASS | Real local session: normal UI enables both flowing and Bubble styles; 12 WebSocket comments complete successfully and Japanese moving text plus Bubble cards are visible. `60`, `61`, `comments-complete-exit.txt`. Titlebar/taskbar icons and Ctrl+Alt+Tab names/icons are visible in `16`, `32`, `88`; ordinary Alt+Tab selects Audience and keeps the deck. `alt-tab-away-win32.json`. |
| F01 | PASS after fix | The original [100,100,1398,1132] overflow is retained as historical evidence. Normal fresh/restored 125% startup now fits [100,0,1398,1020], including native chrome and taskbar exclusion. Expanded minimum-width layout, error auto-expansion, join controls, notes, Settings, maximize/restore and fullscreen recovery passed. Process-only 100% fresh/restored checks also passed; see the follow-up. |
| B01 | BLOCKED | Physical extended-desktop swap, fullscreen destination swap, and swap-back: one monitor only. Single-monitor safe no-op passed instead. |
| B02 | BLOCKED | Real movement between monitors with different DPI and Windows scale-change transitions. No second display; no OS-wide DPI change was made. Prior process-only 100%/125% GUI tests do not certify this. |
| B03 | BLOCKED | Physical display disconnect/rearrange and placement recovery. Requires the missing second-display topology. |
| N01 | NOT APPLICABLE | Store install/uninstall, Start Menu registration, SmartScreen/trust, registered CLI alias, signing and final packaged-library certification. This session intentionally targets the current development build, not an installed release. |
| N02 | NOT APPLICABLE | External relay fixture, physical LAN/mobile audience and release load certification. The current validation uses only a host-local test session and does not claim those A6 release gates. |

## Findings and limits

### Work-area clipping

The original F01 is observable in `after-f5-win32.json` (initial error-expanded window bottom
1225) and `restored-startup-win32.json` / `alt-tab-away-win32.json` (normal window
bottom 1132), beyond physical screen height 1080. Native minimum height is
608 collapsed / 842 expanded in the existing UI. The minimum-size client was
measured, not guessed. Moving the titlebar upward, using the OS Size menu,
collapsing the footer, and maximizing restored practical access. No Slint,
placement, or UI source was changed during that initial observation. The
follow-up below resolves it. No native 1.18 comparison was performed to attribute
it to this migration; pre-migration source comparison is recorded below.

### Native menu mode and timeout

One `qp status` query returned TIMEOUT while the native menu interaction was
still being canceled after Alt+F. F5 did not enter fullscreen in that state.
Read-only Win32 inspection found responsive windows (`hung=false`); another
Escape on the Audience restored immediate status responses, and the following
F5 entered true fullscreen. The failed request is retained in `actions.jsonl`.
No uncertain mutation was retried. This is a modal/menu-mode boundary observation,
not evidence of an app crash or a confirmed new Slint defect; the exact original
menu flags were not captured before recovery. Later normal observations have
GUI flags 0. Operators should completely dismiss a native menu before using
presentation keys; prolonged native-menu/remote-control timing remains a caveat.

### Negative-open diagnostics and earlier intermittent evidence

Both invalid-PDF attempts returned OPEN_FAILED, retained the active deck and
render/notes readiness, and logged `operation=Handshake cause=Spawn`. Therefore
this run establishes failed-open recovery, not a dedicated PDFium parse-stage
diagnostic for corrupt PDFs. Missing-file errors also produced the expected
short failure and retained the deck. No helper deadline, ACL, antivirus, or
sandbox setting was weakened. The exact internal Spawn-stage attribution for
the corrupt fixture remains uninvestigated outside this task's migration scope.

The earlier step-4 renderer timeout and diagnostic comment assertions are not
silently overwritten: their reports remain linked from the Windows results.
Here, valid normal opens/reload/persistence worked and real flowing/Bubble
comments rendered. The initial auxiliary WebSocket client failed to handle a
standard ping after logging 11 accepted comments; it was corrected locally to
reply to ping, then completed all 12. This was a validation-helper error, not an
application comment failure.

### Tool and evidence limitations

The UI tool sometimes returned stale accessibility text, unavailable element
geometry, a temporarily unusable hidden-window binding, or a screenshot of the
foreground occluder. State was refreshed, returned windows reselected, and
foreground screenshots/Win32/CLI results corroborated before assigning PASS.
`28-presenter-during-fullscreen` is an occluded capture; `29` is the corrected
foreground evidence. Dragging outside the visible capture was rejected. OS
Size/Restore keyboard paths were used instead. A caption Restore click minimized
the window; it was recovered and the OS Restore menu verified. These input
limitations were not treated as application crashes. Paint was not used after
its launch approval timed out; no approval barrier was bypassed.

No full Rust, Python, or identical GUI suite was repeated during the initial
desktop-only stage. The source-changing follow-up ran the required Rust checks
and the native GUI suite, as recorded below.

## Cleanup, integration patch, and required follow-up

The test app was closed normally at the end, with no remaining target windows
or owned test GUI process. The local Audience session and helper client ended.
The missing-file fixture was restored. Test configuration and raw evidence are
retained under the isolated run directory; personal app configuration and installed
packages were not replaced. No security/display settings were changed.

The prior step-4 source changes are saved verbatim relative to HEAD in
[`slint119-windows-source.patch`](../../artifacts/slint-1-19-windows-step5-20261117-155102/slint119-windows-source.patch):

- `src/gui_smoke.rs`: native-font-measured smoke fixture and full fit diagnostics.
- `src/notes.rs`: pure padding-threshold regression test, inside `#[cfg(test)]`.

The initial desktop-only stage added no further source change. Patch hash, base HEAD, file list and
reverse-application check are in `source-patch-metadata.json`; before/after
status and source hashes protect pre-existing edits. The report is the only
new repository document for this stage. No shared index edit, commit, push,
merge, release, signature, or coverage measurement was performed during step 5.

To finish the blocked physical gates, connect a second display in extended
desktop mode, preferably with a different scale. Put Presenter/Audience on
different monitors, exercise X in both directions and while fullscreen, move
between the two DPI scales, disconnect/rearrange the audience display, and check
keyboard/focus/recovery. The additional unrelated-application switcher experiment
is not an assignment gate. F01 is resolved by the follow-up below.
Final Store/packaged acceptance remains a later, separately identified build.

## F01 work-area fix and revalidation

Follow-up: 2026-11-17, approximately 17:33–18:14 JST, on the same native Windows
host and unchanged HEAD `deee3b6731e631c86309b8e681a2c3fd20afdaf6`. Evidence is in
[`artifacts/slint119-windows-workarea-20261117-173308/`](../../artifacts/slint119-windows-workarea-20261117-173308/).
The before snapshots preserve the original step-4 source edits and this report's
initial results. No commit, push, GitHub post, release, signing or coverage run
was performed.

### Cause and implementation

Read-only `GetMonitorInfoW` observations measured monitor bounds [0,0,1920,1080]
and work area **[0,0,1920,1020]**. The normal Presenter nonclient frame, including
the native menu, is 18x72 physical pixels. At Slint scale 1.25, the fixed logical
initial position (80,80) becomes (100,100). The preferred collapsed client
1024x768 becomes 1280x960, giving outer bottom 1132. Even moving upward cannot
fit the old expanded minimum 842 logical pixels plus this frame into 1020.

`git show 23648de^:src/window_controller.rs` and `:ui/app.slint` confirm that
the fixed position, 768/1002 preferred heights and 608/842 minima existed before
the Slint migration. This establishes a pre-existing geometry assumption, not a
native 1.18-versus-1.19 rendering comparison or proof of migration causation.

The six-file incremental change is:

- `Cargo.toml`: enable the existing windows-sys dependency's GDI monitor APIs.
- `src/windows_window.rs`: query the window's nearest monitor work area, account
  for actual native frame dimensions, and clamp normal client size/outer position.
  Pure geometry tests include DPI-scaled minima, negative monitor origins,
  oversized/offscreen bounds and impossible/invalid extents.
- `src/main.rs`: await `winit_window()` after show before initial fitting; native
  creation is deferred until the event loop. Initial synchronous/zero-delay
  attempts returned before a usable window and are preserved as diagnostic evidence.
- `src/audience_panel.rs`: refresh the work-area boundary on disclosure and limit
  expansion height while retaining the device-pixel collapsed/expanded size pair.
- `ui/app.slint`: use the Rust-supplied available client height to reduce the
  scrollable history allocation and expanded minimum while reserving the main
  panes, disclosure and URL row. The collapsed minimum stays 800x608. A zero
  work-area value retains the previous layout for other platforms/software cases.
- `tests/presenter_layout.rs`: extend the existing compiled-layout test with the
  actual constrained heights at scales 1 and 1.25, landscape/portrait slides,
  repeated disclosure, notes viewport at least 180 logical pixels, visible previews,
  URL/history rows, footer bounds and restoration to the collapsed minimum.

Available logical height is rounded down before becoming a Slint constraint.
The first diagnostic revealed that a fractional minimum could otherwise round
one physical pixel past the taskbar (bottom 1021); the final build fits bottom
1020. Maximized/fullscreen geometry remains owned by Windows. Standard maximized
outer borders [-9,-9,1929,1029] and fullscreen [0,0,1920,1080] are intentional,
and are not evaluated as normal-window work-area failures.

### Final results

All coordinates below are physical. Normal nonclient dimensions remain 18x72.
The OS monitor/window DPI remains 120 throughout. The 100% rows use only
`SLINT_SCALE_FACTOR=1`; their chrome still has native 125% metrics. They do not
certify an OS-wide 100% desktop or a physical transition between DPI scales.

| Case | Observed Presenter geometry / result | Evidence |
| --- | --- | --- |
| Native 125%, fresh and no-PDF-argument restored startup | Outer [100,0,1398,1020], client 1280x948 / 1024x758.4 logical; both inside work area. Restored PDF is page 1/12, render/notes ready. | `125-fresh/initial-final-win32.json`, `reopened-win32.json`, `05`, `23`, CLI ledger |
| Native 125%, minimum collapsed | Outer [100,0,1118,832], client 1000x760 / 800x608 logical. Notes, previews, thumbnails and controls remain usable. | `125-fresh/minimum-win32.json`, `11` |
| Native 125%, minimum-width expanded | Outer [100,0,1118,1020], client 1000x948. History, actual local join URL and Copy join URL fit; a real local session starts. | `minimum-expanded-win32.json`, `12`, `13` |
| Native 125%, missing-configuration auto-expansion | Outer [100,0,1118,1020]; complete error guidance, notes and bottom status/URL row visible. Native Application > Settings opens and Close returns to playback. | `125-error/initial-error-win32.json`, `27`–`30` |
| Native 125%, maximize/restore and fullscreen return | OS maximize works; normal restoration returns client 800x608 logical inside work area. Audience F5 reaches [0,0,1920,1080]; Escape restores Audience [225,175,1523,942] and preserves Presenter. | `14`–`22`, `restored-stable-win32.json`, `fullscreen-win32.json`, `after-fullscreen-win32.json` |
| Process-only 100%, fresh/restored startup | Outer [80,80,1122,920], client 1024x768; remembered PDF page 1/12 and Audience [180,140,1222,763] restored. | `100-fresh/initial-win32.json`, `reopened-win32.json`, `31`, `40`, CLI ledger |
| Process-only 100%, expanded and minimum | Expansion fits [80,0,1122,1020], client 1024x948. OS Size reaches 800x608 collapsed, outer [80,0,898,680]. Main/next/notes/thumbnails and footer remain visible. | `expanded-win32.json`, `minimum-win32.json`, `32`–`35` |
| Process-only 100%, maximize/restore/fullscreen | Maximize and normal restoration work; Audience reaches true fullscreen and Escape returns to the visible minimum Presenter layout. | `36`–`39`, `100-fresh/maximized-win32.json`, `fullscreen-win32.json` |
| Required native switcher/focus check | OS Alt+Tab selects Audience with Win32 foreground=true; Audience Right advances to page 2/12. Alt+Tab returns Presenter with foreground=true and the Japanese notes. | `24`–`26`, `alt-tab-audience-win32.json`, `alt-tab-return-win32.json` |

The persisted placement record belongs to Audience; Presenter starts from its
normal default each launch and is fitted again. This change does not introduce
Presenter position persistence. All test apps exited normally; the local session
ended and the final window inventory contains no Quick Presenter windows.

Required checks passed on the final source, with their original bounds unchanged:

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Exit 0 |
| `cargo check --all-targets --locked` | Exit 0, no new warnings in the final run |
| `cargo test --locked --no-fail-fast` | Exit 0: **610 passed, 0 failed, 2 ignored** across all targets; no retry required |
| Native Windows GUI smoke, system 125% | Exit 0: **208 passed, 0 failed** |
| `git diff --check` | Exit 0 |

`commands.jsonl`, bounded raw stdout/stderr and `workarea-*-diagnostics.txt`
retain timings/exit codes. The earlier complete diagnostic Rust run also passed
610/0/2; its raw `workarea-test/tmpmd33wywl.stdout` is preserved alongside the
final run. Existing ignored external-relay/sustained-load cases remain ignored.
No renderer deadline or sandbox/ACL/security setting was changed.

One auxiliary `qp status` invocation from the restricted shell received OS access
denied; the host-user invocation succeeded. This is recorded in `actions.jsonl`,
not hidden or counted as an application regression. The first UI capture timed
out during concurrent validation; returned windows were reacquired and subsequent
captures/Win32 measurements succeeded. Alt+Space while the disclosure button was
focused also activated that button; this side effect is retained in `14`–`17`.
Repeating the OS maximize/restore sequence with the read-only notes field focused
kept the disclosure stable (`18`–`20`). It is not represented as a clean initial
expanded-state maximize test. No input behavior was changed as part of F01.

### Integration and remaining scope

`windows-work-area-fix.patch` contains only the six files above, based on the same
HEAD. SHA-256:
`8ad69b28ef1b5fa782529397bd29deb9613b042f7282cc92ea72c13b3c851295`.
Both the index-base application check and reverse check against the working tree
passed. `source-patch-metadata.json` verifies that all six before snapshots match
HEAD and that the step-4 `src/gui_smoke.rs` / `src/notes.rs` hashes are unchanged.
The original step-4 patch remains separate and unchanged. Cargo.lock, build.rs,
vendored winit, PDFium files and its fetch procedure are unchanged.

Final GUI executable SHA-256:
`6d8ceba63bfbf0bfb08a90d2a31c83b608f36f4a74d4a7c17bccbbff7fc7fee1`.
Companion qp SHA-256:
`6386eb6a999dbab0b90e25a560e1f0a30d282af1017429682efe130bf9b124fa`.

F01 is resolved on the available desktop. Only B01–B03 remain physical-display
gates. The earlier unrelated-application/unaided-return experiment B04 was beyond
the requested gate; the required native menu, icon and own-window switcher checks
already passed, including the repeated final-build focus observation above.
Store/packaged acceptance remains separate as N01.

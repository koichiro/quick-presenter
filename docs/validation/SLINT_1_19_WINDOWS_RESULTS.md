# Slint 1.19.0 native Windows step-4 results

## Follow-up: typography fixture repaired (2026-11-17)

The four Windows typography failures are resolved in the local working tree.
The native development GUI suite now passes **208/208 checks at native 125%**
and **208/208 checks with a process-only 100% scale override**. Production font
fitting and Slint UI code are unchanged. The repair changes only the GUI smoke
fixture/diagnostics and adds one pure Rust regression test.

Formatting and `cargo check --all-targets --locked` passed. The new full
`cargo test --locked` attempt exited 101: **602 passed, 1 failed, 2 ignored**.
The single renderer response-timeout failure passed in a focused retry under
unchanged deadlines; the two settings tests omitted after that early stop also
passed. Thus **605 distinct Rust tests have successful final observations, with
2 ignored**, but the original full-command failure remains part of the evidence.
It is not reported as a clean single-attempt Cargo result. No unrelated successful
targets were rerun to manufacture individual logs.

### Execution state and evidence

This follow-up began at 15:23:37 JST / UTC+09:00. HEAD was and remains
`deee3b6731e631c86309b8e681a2c3fd20afdaf6` on
`codex/slint-1-19-migration`. At entry, the migration index already had its
Windows report link, and the previous report and user validation files were
untracked. Their initial status and the report/index contents were preserved
before editing. New source edits are limited to `src/gui_smoke.rs` and the
`#[cfg(test)]` module in `src/notes.rs`; the report and migration index are updated.
Cargo.lock, Slint 1.19.0 pins, both compiler paths' edition 1.18, the winit
0.30.13 override, PDFium, and native licenses remain unchanged. Nothing was
committed, pushed, posted, signed, merged, or released.

Follow-up evidence, including original diagnostic failures and fixed reports:
[`artifacts/slint-1-19-windows-font-fixture-20261117-152337/`](../../artifacts/slint-1-19-windows-font-fixture-20261117-152337/).
The directory contains `commands.jsonl`, raw per-label stdout/stderr,
`diagnostic-*-report.txt`, `fixed-*-report.txt`, source diffs, per-target counts,
`before-*`, `after-*`, and exact runner scripts. The initial report and index
are copied to `report-before.md` and `index-before.md`; the earlier 14:34 run's
raw evidence remains untouched in its original directory.

### Root cause confirmed from native measurements

The diagnostic build changed logging only and retained the original
`"語\n".repeat(8)` fixture and every assertion. It reproduced all four typography
failures at both scale factors. Each report records all 13 candidate heights,
padding, viewport, actual/expected font, content height, scale factor, and both
physical and logical window sizes.

The measured heights for font sizes 12 through 24 were identical at both scales:

```text
[180, 195, 210, 225, 240, 255, 270, 285, 300, 315, 330, 345, 360]
bottom padding = 10 logical pixels; viewport width = 348 logical pixels
```

| Original failed check | Viewport height | Actual / measured maximum | Why the fixed expectation was invalid |
| --- | ---: | --- | --- |
| Intermediate size, before PDF open | 203 | 12 / 12 | 13px requires 195 + 10 = 205px |
| Intermediate size, with PDF open | 199 | 12 / 12 | 13px requires 205px |
| Grow to exactly 24px, before PDF open | 180 to 350 | 12 to 22 / 12 to 22 | 22px needs 340px; 23px needs 355px; 24px needs 370px |
| Grow to exactly 24px, with PDF open | 180 to 346 | 12 to 22 / 12 to 22 | The same 355px and 370px candidates cannot fit |

At the constrained 180px viewport, the measured 12px content plus padding was
190px: minimum-size overflow was legitimate and remained scrollable. At 203px,
199px, 350px, and 346px, content height equaled the viewport after fitting.
The values prove that all four observed selections obeyed the production
largest-fitting-size contract. The failures were in the smoke fixture's premise,
not in the observed font-fitting behavior or scale conversion.

For the initial fixture, logical 800x608 meant physical 1000x760 at scale 1.25
and 800x608 at scale 1.0. After resizing, logical 800x640 meant physical
1000x800 and 800x640 respectively. The physical size stayed fixed while the
aspect ratio released more notes space. Native 125% used no scale override;
100% used `SLINT_SCALE_FACTOR=1` only in the GUI child. System DPI settings
remained untouched. The backend's local 1.19.0 source documents and implements
this override in `winitwindowadapter.rs`.

No native Slint 1.18 comparison was needed to establish this specific contract
violation in the fixture. This evidence does not establish whether the migration
changed Windows font metrics; it establishes that the current production choices
were correct for the measurements, and that the original assertions demanded
sizes that could not fit even at 100%.

### Minimal repair and retained regression coverage

The smoke fixture now tries a bounded number of multiline repeats, using only
the measured heights to find content for which **18px fits and 24px overflows**
the initial viewport. It does not consult the selected size to choose the
fixture, set a font directly, call the production fitting callback manually,
relax the deadline, or skip an assertion. Failure to construct the fixture within
32 repeats is an explicit smoke failure.

The original contracts remain: an intermediate whole-pixel size must fit;
every larger candidate must overflow; the resize must recompute the maximum;
changing aspect ratio at fixed native geometry must strictly grow the font to
exactly 24px; long notes must remain scrollable at 12px; shrinking content must
clamp its previous scroll offset. The intermediate check now additionally rejects
fractional sizes. All padding comparisons use `NOTES_BOTTOM_PADDING`.

On both tested scales the selected fixture uses four repeats. Its full measured
height array is `[100, 109, 117, 125, 134, 142, 150, 158, 167, 175, 183, 192, 200]`.
The initial 203px viewport selects 23px (192 + 10 <= 203; 200 + 10 > 203), and
the 199px viewport selects 22px (183 + 10 <= 199; 192 + 10 > 199). A 235px/231px
resize selects 24px. The aspect-ratio check grows 20px to 24px as the viewport
increases from 180px to 350px/346px. Full arrays and both geometry forms are
included in passing reports, not only in failure diagnostics.

`windows_multiline_measurements_respect_padding_at_every_size_boundary` uses the
captured original Windows array to protect all 13 inclusive padding thresholds,
the immediately lower value at each of the 12 nonminimum thresholds, the 12px
minimum/overflow cases, and the 22px maximum at 346px/350px. The test passed in
the complete Rust run; existing software layout, Quit/close, renderer, and ACL
regression assertions were retained.

### Commands, retries, and limits of the result

| Label / command | Exit | Result |
| --- | ---: | --- |
| `diagnostic-build`: `cargo build --locked` | 1 | Stopped deliberately during compilation to use the existing debug=1 test profile; no GUI result from this build |
| `diagnostic-build-test-profile`: `cargo test --locked --no-run` | 0 | Diagnostic binaries built with the existing test profile |
| `diagnostic-125` GUI | 1 | 202 passed, 6 failed: original four typography assertions plus two comment assertions |
| `diagnostic-100` GUI | 1 | 204 passed, 4 failed: the original four typography assertions |
| `fixed-fmt`: `cargo fmt --check` | 0 | Passed |
| `fixed-check`: `cargo check --all-targets --locked` | 0 | Passed |
| `fixed-test`: `cargo test --locked` | 101 | 602 passed, 1 failed, 2 ignored |
| `fixed-renderer-retry`: `cargo test --locked --test renderer_process candidate_abort_or_eof_does_not_interrupt_active_helper -- --exact --nocapture` | 0 | 1 passed, 14 filtered; same deadlines and sandbox conditions; RUST_BACKTRACE=1 for diagnostics only |
| `fixed-remaining`: `cargo test --locked --test settings_storage_windows --example store_ipc_probe` | 0 | 2 passed; empty platform-gated example remains 0 tests |
| `fixed-doc`: `cargo test --locked --doc` | 0 | 0 doctests; not platform evidence |
| `fixed-125` GUI | 0 | 208 passed, 0 failed |
| `fixed-100` GUI | 0 | 208 passed, 0 failed |

All commands ran from the repository root using native MSVC host execution,
the existing pinned PDFium DLL, isolated GUI APPDATA/LOCALAPPDATA, and unchanged
test deadlines. Exact command arguments and local timestamps are in the command
ledger. Successful Python and no-default-features checks from the initial run
were not repeated because this repair does not affect those configurations.

The one Rust failure was
`candidate_abort_or_eof_does_not_interrupt_active_helper`, which timed out at
`tests/renderer_process.rs:558` waiting for a raw helper response under the
existing 30-second Windows deadline. All other 14 renderer tests passed,
including both Job Object termination cases and missing-PDFium diagnostics.
The failed test passed alone in 4.06 seconds without source, timeout, antivirus,
ACL, or confinement changes. The full-command failure is preserved, and its
environment/timing cause remains unproven; this repair does not claim to fix
renderer test intermittency.

The diagnostic 125% run also reported `comment=None` in two related Audience
comment assertions. Those same assertions passed in the diagnostic 100% run and
both fixed full GUI runs. No comment code or assertion was changed. That one
intermittent observation is retained and is not attributed to the typography
repair or declared independently root-caused.

Current per-target Rust command counts follow. Filtered tests are not passes;
zero-test cfg-gated targets are not native validation.

| Command | Target | Passed | Failed | Ignored | Filtered |
| --- | --- | ---: | ---: | ---: | ---: |
| fixed-test | `unittests src\lib.rs` | 23 | 0 | 0 | 0 |
| fixed-test | `unittests src\bin\qp.rs` | 0 | 0 | 0 | 0 |
| fixed-test | `unittests src\main.rs` | 546 | 0 | 2 | 0 |
| fixed-test | `tests\audience_window_layout.rs` | 1 | 0 | 0 | 0 |
| fixed-test | `tests\control_contract.rs` | 5 | 0 | 0 | 0 |
| fixed-test | `tests\control_events.rs` | 3 | 0 | 0 | 0 |
| fixed-test | `tests\control_ipc.rs` | 0 | 0 | 0 | 0 |
| fixed-test | `tests\control_ipc_windows.rs` | 5 | 0 | 0 | 0 |
| fixed-test | `tests\control_launch.rs` | 0 | 0 | 0 | 0 |
| fixed-test | `tests\control_launch_windows.rs` | 1 | 0 | 0 | 0 |
| fixed-test | `tests\control_lock.rs` | 0 | 0 | 0 | 0 |
| fixed-test | `tests\macos_fullscreen_transition.rs` | 3 | 0 | 0 | 0 |
| fixed-test | `tests\presenter_layout.rs` | 1 | 0 | 0 | 0 |
| fixed-test | `tests\renderer_process.rs` | 14 | 1 | 0 | 0 |
| fixed-renderer-retry | `tests\renderer_process.rs` | 1 | 0 | 0 | 14 |
| fixed-remaining | `tests\settings_storage_windows.rs` | 2 | 0 | 0 | 0 |
| fixed-remaining | `unittests examples\store_ipc_probe.rs` | 0 | 0 | 0 | 0 |
| fixed-doc | `quick_presenter` | 0 | 0 | 0 | 0 |

### Current CI and remaining certification

Read-only inspection of PR #548 found all current checks successful. The newer
[Build Binaries run 44020629040](https://github.com/koichiro/quick-presenter/actions/runs/44020629040)
completed successfully at HEAD `deee3b6731e631c86309b8e681a2c3fd20afdaf6`.
Its downloaded Ubuntu installed-package GUI report says **215 passed, 0 failed**
and is retained in `latest-ubuntu-gui-report/`. This supersedes the earlier
Ubuntu CI status, not the preserved raw failure evidence from run 43899056288.
This CI run predates the local fixture repair; the local uncommitted patch has
only the native Windows validation recorded here and has not been pushed.

The requested four-failure investigation and local repair are complete. The
intermittent renderer/comment observations above remain candid limitations of
the collected evidence. Step-5 manual menu placement, state persistence, focus,
taskbar/Alt+Tab, fullscreen transitions, high-DPI/multiple-display behavior,
installed packaging, and Store/signing certification remain separate pending
work. One monitor and a process-only scale comparison do not certify that matrix.
No coverage, posting, push, merge, release, signing, or OS DPI change was performed.

---

## Initial step-4 execution (historical, 14:34–15:07 JST)

The following original-run results describe the initial handoff. Their failures
and dirty state are historical; the follow-up above records the current repair
and outcomes. An unchanged copy is also saved as `report-before.md`.

## Result and scope

Native Windows Rust validation passed: **604 passed, 0 failed, 2 ignored** in
the complete default suite, and **37 passed, 0 failed, 0 ignored** in the explicit
no-default-features control subset. All Rust filtered counts were zero.
Formatting, all-target checking, and both dependency-tree commands passed.

The Python command initially reported 33 passed, 2 failed, and 5 skipped.
The two failures were resolved by selecting Git Bash ahead of the WindowsApps
`bash.exe` alias in the child PATH; only those two tests were rerun, both passed.
The resulting distinct Python test outcomes are **35 passed, 5 skipped**.
Both the original failure and the successful environment-only retry are retained.
No application, test, dependency, lockfile, compiler-edition, or vendor changes
were needed for this validation.

Native development GUI automation reported **204 passed, 4 failed**. The four
failures concern fixed typography-fixture expectations, while the initial
resize settling timeout did not recur. The complete GUI gate is therefore
**not green**. Details and the limits of migration attribution are below.
This report does not certify the manual step-5 checklist or an installed release
package.

## Execution identity and preserved work

- Local date: 2026-11-17, Tokyo Standard Time / Asia/Tokyo, UTC+09:00.
  Setup began around 14:34; the environment transcript starts at 14:36:40.
  Exact per-command start/end times are in `commands.jsonl` and `gui-command.json`.
- Initial branch: `codex/gui-recovery-windows-b595ac8`.
- Initial HEAD: `b595ac8dafe8f8c8e0de000c84b7b095faa133bd`, still using Slint 1.18.1.
  There were no tracked edits or staged changes, but existing untracked validation
  documents, images, and a historical results directory were present.
- The requested branch was absent locally. A targeted fetch and ordinary
  `git switch -c codex/slint-1-19-migration --track origin/codex/slint-1-19-migration`
  preserved those files. No reset, clean, merge, or worktree creation was used.
- Actual execution and final HEAD:
  `deee3b6731e631c86309b8e681a2c3fd20afdaf6` on
  `codex/slint-1-19-migration`.
- Requested implementation: `23648de76b2a4136a5d2f587e0f8353eb6e4e335`;
  audited baseline: `3c376b8fbe33ea506487cc21f42fdf0c72aa2394`.
  The one newer commit, `deee3b6`, changes `src/gui_smoke.rs` and the migration
  index. Its settling-clock fix is committed in this checkout, not an unverified
  local edit. The execution-time diff is saved as `baseline-diff.txt`.
- Final task edits consist of this English report and a link from the migration
  index. Raw evidence is in the ignored `artifacts/` directory. Existing untracked
  user files remain in place; no commit, push, GitHub post, merge, or release was made.
  See `before-*`, `execution-*`, and `after-*` evidence for HEAD and dirty states.

## Native environment and dependencies

| Component | Observed value |
| --- | --- |
| OS / kernel | Windows 11 Pro 25H2, Windows NT 10.0, build 26200.9974, x64 |
| CPU | Intel Core i5-7300U @ 2.60 GHz, 2 cores / 4 logical processors |
| Filesystem | C: NTFS; local user temp directory on C: |
| Rust | rustc 1.99.1 (`58cf8f9c3`, 2026-10-13), LLVM 23.1.1 |
| Cargo | 1.99.1 (`5f94df478`, 2026-08-27) |
| Host / target | Native `x86_64-pc-windows-msvc`; no WSL or cross compilation |
| Python | Bundled native Python 3.12.14, MSC v.1944 AMD64 |
| Build tools | Visual Studio Build Tools 2022 17.14.42, installed at `C:/BuildTools` |
| MSVC / linker | toolset 14.44.35207; `link.exe` file/product version 14.44.35229.0 |
| Windows SDK | 10.0.28000.0, x64 resource compiler file version 10.0.28000.2270 |
| Desktop | Windows session ID 1; one display, primary physical 1920x1080; system DPI 120 (125%) |
| Cargo features | Default empty feature set; then explicit `--no-default-features`; no optional Store feature |
| Test profile | Existing `[profile.test] debug = 1`, unchanged |

Developer PowerShell was initialized with `-arch=x64 -host_arch=x64` in the
child validation process. Full tool paths, versions, timestamps, filesystem,
session, and DLL PE machine fields are retained in `environment.txt`,
`host-details.json`, `windows-build.json`, the version logs, and
`linker-sdk-file-versions.txt`. The ordinary `python.exe` PATH entry was a
WindowsApps alias, so the bundled Python executable was used explicitly.

The existing `pdfium/bin/pdfium.dll` is x64 (PE machine `0x8664`), but its
`pdfium/VERSION` says build 8076. The pinned manifest requires chromium/8111.
The pinned fetch script therefore downloaded `pdfium-win-x64.tgz` into the fresh
run-local `pdfium-8111/` directory, without `--clean`, manifest updates, or changes
to the existing DLL or licenses. Archive SHA-256 verification passed against
`3452d11031050ec65e57f180215b57b9e6d465e3fbedafa8a77ad90885c7d23e`.

The tested library is:

```text
C:/Users/admin/projects/quick-presenter/artifacts/slint-1-19-windows-20261117-143406/pdfium-8111/bin/pdfium.dll
VERSION: MAJOR=158 MINOR=0 BUILD=8111 PATCH=0
PE machine: 0x8664 (AMD64)
DLL SHA-256: A318C39C137482DC4687C2973B7EC1C27FC1C20253CF2DC31928F922EAB89E8F
```

`PDFIUM_DYNAMIC_LIB_PATH` was scoped to the validation children. No packaged
application override or application sandbox relaxation was used.

The lockfile audit finds every Slint/internal Slint package at 1.19.0, including
the registry-sourced `i-slint-backend-winit`. `cargo tree -i winit` confirms the
local `vendor/winit` 0.30.13 override for every path. Both platform source paths
in `build.rs` use the shared `CompilerConfiguration::with_edition((1, 18))`.
The lockfile and vendor patches are unchanged; see `dependency-audit.json`.

## Commands and outcomes

Commands ran from the repository root. The wrapper records raw stdout/stderr,
exit status, elapsed time, and log paths. Its limits were 1800 seconds for Cargo,
180 seconds for fetching/retrying Python, and 600 seconds for GUI automation.
The full default suite succeeded, so no `--no-fail-fast` rerun was needed.

| Command | Exit | Outcome / log label |
| --- | ---: | --- |
| `cargo fmt --check` | 0 | `fmt` |
| `cargo check --all-targets --locked` | 0 | `check`; 492.1 seconds |
| `cargo test --locked` | 0 | `test-default`; 604 passed, 2 ignored; 764.9 seconds |
| `cargo tree --locked -i winit` | 0 | `tree-winit` |
| `cargo tree --locked --prefix none` | 0 | `tree-all` |
| `python -m unittest tests/test_fetch_pdfium.py tests/test_release_sbom.py tests/test_cli_packaging.py` | 1 | `python-tests`; 33 passed, 2 failed, 5 skipped |
| Two failed `CliPackagingTests` methods, `python -m unittest -v ...` with Git Bash first in PATH | 0 | `python-packaging-retry`; 2 passed |
| `cargo test --locked --no-default-features --lib --test control_contract --test control_events --test control_ipc --test control_ipc_windows --test control_lock --test control_launch --test control_launch_windows` | 0 | `test-no-default`; 37 passed |
| `target/debug/quick-presenter.exe --gui-smoke tests/fixtures/marp-speaker-notes.pdf --gui-smoke-report <run-dir>/windows-gui-smoke.txt` | 1 | Native development GUI; 204 passed, 4 failed |

The Python failures were
`test_macos_explicit_cli_and_bundled_documentation` and
`test_missing_sibling_cli_preserves_existing_macos_bundle`.
The initial `bash` resolved to
`C:/Users/admin/AppData/Local/Microsoft/WindowsApps/bash.exe`; the captured
subprocess reader also raised a cp932 decoding error and the assertions received
empty stderr. Prepending `C:/Program Files/Git/bin` to the child PATH made both
pass, without source or encoding changes. These are fixture-based packaging
layout tests, not native macOS validation or actual signing.

Python module outcomes, combining the original run with the two focused retries:

| Module | Passed | Failed remaining | Skipped |
| --- | ---: | ---: | ---: |
| `test_fetch_pdfium` | 11 | 0 | 2 |
| `test_release_sbom` | 20 | 0 | 1 |
| `test_cli_packaging` | 4 | 0 | 2 |

The skips correspond to unavailable symlink creation in two fetch tests, the
explicit Windows symlink guard in the SBOM test, and missing `dpkg-deb` in two
Debian tests. These were not promoted to passes or bypassed by changing privileges.

## Rust results by target

The table below comes from the full default suite and the separately requested
configuration subset. Empty cfg-gated targets are shown as zero and do not
establish native platform behavior. `macos_fullscreen_transition` runs three pure
vendored-winit tracker tests on Windows; it does not exercise native AppKit.

| Command | Target | Passed | Failed | Ignored | Filtered |
| --- | --- | ---: | ---: | ---: | ---: |
| test-default | `unittests src\lib.rs` | 23 | 0 | 0 | 0 |
| test-default | `unittests src\bin\qp.rs` | 0 | 0 | 0 | 0 |
| test-default | `unittests src\main.rs` | 545 | 0 | 2 | 0 |
| test-default | `tests\audience_window_layout.rs` | 1 | 0 | 0 | 0 |
| test-default | `tests\control_contract.rs` | 5 | 0 | 0 | 0 |
| test-default | `tests\control_events.rs` | 3 | 0 | 0 | 0 |
| test-default | `tests\control_ipc.rs` | 0 | 0 | 0 | 0 |
| test-default | `tests\control_ipc_windows.rs` | 5 | 0 | 0 | 0 |
| test-default | `tests\control_launch.rs` | 0 | 0 | 0 | 0 |
| test-default | `tests\control_launch_windows.rs` | 1 | 0 | 0 | 0 |
| test-default | `tests\control_lock.rs` | 0 | 0 | 0 | 0 |
| test-default | `tests\macos_fullscreen_transition.rs` | 3 | 0 | 0 | 0 |
| test-default | `tests\presenter_layout.rs` | 1 | 0 | 0 | 0 |
| test-default | `tests\renderer_process.rs` | 15 | 0 | 0 | 0 |
| test-default | `tests\settings_storage_windows.rs` | 2 | 0 | 0 | 0 |
| test-default | `unittests examples\store_ipc_probe.rs` | 0 | 0 | 0 | 0 |
| test-default | `quick_presenter` | 0 | 0 | 0 | 0 |
| test-no-default | `unittests src\lib.rs` | 23 | 0 | 0 | 0 |
| test-no-default | `tests\control_contract.rs` | 5 | 0 | 0 | 0 |
| test-no-default | `tests\control_events.rs` | 3 | 0 | 0 | 0 |
| test-no-default | `tests\control_ipc.rs` | 0 | 0 | 0 | 0 |
| test-no-default | `tests\control_ipc_windows.rs` | 5 | 0 | 0 | 0 |
| test-no-default | `tests\control_launch.rs` | 0 | 0 | 0 | 0 |
| test-no-default | `tests\control_launch_windows.rs` | 1 | 0 | 0 | 0 |
| test-no-default | `tests\control_lock.rs` | 0 | 0 | 0 | 0 |

The required exact tests passed within the default suite, without redundant
individual reruns:

- `presenter_panes_resize_without_losing_notes_or_overlapping_the_footer`
  (software Slint layout, 18.30 seconds).
- `audience_window_does_not_cap_fullscreen_size_to_its_windowed_size`
  (compiled-source constraint).
- `quit_callback_and_presenter_close_share_recoverable_event_loop_exit`
  (software presenter/audience Quit, presenter close, exit error recovery, and
  audience Settings/About forwarding).
- Both `settings_storage_windows` tests:
  `new_settings_do_not_inherit_public_access_from_the_parent` and
  `replacing_existing_settings_removes_inherited_public_access`.
- Native `control_ipc_windows`: 5 tests; native `control_launch_windows`: 1 test.
  Both also ran nonzero tests in the no-default-features subset.
- Native `renderer_process`: 15 tests, 163.77 seconds, including active and
  suspended helper Job Object termination and PDFium-resolution diagnostics
  without path leakage. The harness printed over-60-second notices while these
  serialized broker tests ran, then all passed within their existing deadlines.
  No antivirus change, timeout extension, or confinement bypass was used.

The two intentionally ignored Audience tests remain ignored:

- `audience::tests::sustained_local_load_remains_bounded_and_stops_cleanly`:
  `explicit local sustained-load validation; takes about 30 seconds`.
- `audience_relay::fixture_tests::external_relay_room_creation_round_trip_and_termination`:
  `requires a running OSS relay runtime and QUICK_PRESENTER_TEST_CONFIG`.

## Native GUI automation and known Linux failure

The native winit development build ran on the Windows desktop at the observed
125% system scale from 15:02:07 to 15:02:54 (+09:00), using isolated APPDATA and
LOCALAPPDATA directories and the pinned PDFium DLL. No software-backend override
was used; `SLINT_RENDERER` was unset. Full scope was selected (no comments-only
scope). Result: **204 passed, 4 failed**, exit 1, elapsed 46.7 seconds.

The initial no-PDF 800x608 resize completed without the known settling timeout.
Both explicit resized-viewport maximum-fit checks passed: 15px in a 235px
viewport before PDF opening and 14px in a 231px viewport with a PDF open.
PDF rendering, page navigation, Audience Live, blackout, fullscreen state, and
hidden-window restoration checks also progressed through the complete suite.
These are automated app-observable checks, not manual desktop certification.

All four failed assertions are retained in `windows-gui-smoke.txt`:

| Assertion | Before opening PDF | With PDF open |
| --- | --- | --- |
| Multi-line fixture must use an intermediate size greater than 12px | actual 12px, viewport 203px | actual 12px, viewport 199px |
| Releasing notes space must reach exactly 24px | 12 to 22px, viewport 180 to 350px | 12 to 22px, viewport 180 to 346px |

Inspection identifies a fixture assumption rather than evidence of incorrect
font fitting. The fixed `"語\n".repeat(8)` fixture measures
`[180, 195, 210, 225, 240, 255, 270, 285, 300, 315, 330, 345, 360]` pixels for
12 through 24px on this host. With the existing 10px bottom padding, 13px needs
205px and cannot fit either 203px or 199px, so the selected 12px is correct.
At the larger reported viewports, that same unwrapped fixture needs 370px for
24px; its 22px measurement plus padding is 340px. The observed 22px is consistent
with the measured maximum, but the assertion hard-codes 24px. The controlled
native window remained 1000x800 physical pixels during both aspect-ratio changes.

These fixed-fixture assertions and the notes UI/font-fitting function predate
the migration; `gui-smoke-migration-diff.txt` shows only the settling-clock
change in this harness. This session did not run a native pre-migration GUI
comparison, so it does not establish whether the backend upgrade affected the
metrics. The defensible classification is **GUI fixture portability failure
under the observed Windows typography**, with migration causation unproven.
No production typography change, assertion relaxation, fixture rewrite, or
repeat of successful checks was made. A separately scoped portable-fixture
follow-up remains necessary before calling the full automated GUI suite green.

The execution-time diff confirms that `wait_for_presenter_notes_resize` now
settles and samples the initial input/render pass before starting its five-second
stability clock. Snapshot equality, the quiet period, and geometry checks remain.
This is a GUI harness change; production PDF rendering/layout code was not edited.

The original Ubuntu CI evidence was retrieved read-only from
[run 43899056288, job 133412757409](https://github.com/koichiro/quick-presenter/actions/runs/43899056288/job/133412757409).
Its original report is preserved under `ubuntu-original-gui-report/`: **3 passed,
1 failed**, before PDF opening, notes resize to logical 800x608. The final snapshot
has physical 800x608, no notes, font 12, viewport 348x203, zero measurement
heights, slide 368x207, inline-menu height 0, and main y=54. This is the reported
layout synchronization timeout, not a compiler or missing-library failure.

The migration index describes the independent throttled Xvfb/software-GL
reproduction: its first sample took 12.6 seconds, and the fixed run passed the
first 77 checks before an unrelated emulated renderer handshake failure. Those
are previous Linux evidence, not results from this Windows host. This session
did not execute native Ubuntu or establish a green Ubuntu CI rerun, isolate a
Slint production defect, or reproduce the old version under a Windows CPU limit.
The earlier PR #538 result used different dependencies/environment and is not a
controlled Slint comparison. The smoke path's omission of production
`configure_presenter_menu_bar` predates this migration and was not changed.

`docs/validation/SLINT_1_19_MACOS_STEP4.md` is referenced by the assignment and
index but is absent from the fetched execution HEAD. Its supplied historical
323-check GUI result could not be independently reread here and is not counted
as Windows evidence.

## Boundaries, restrictions, and pending step 5

The outer sandbox initially denied `.git/FETCH_HEAD` writes. That failure is
retained in `fetch.txt`; the authorized host fetch succeeded in `fetch-host.txt`.
Builds and native IPC/process/ACL tests then ran through authorized host execution.
No application sandbox was weakened. The Python shell-alias issue was an
environmental failure resolved by child PATH selection, not an application fix.

No unresolved blocker remains for the mandatory native Rust tests. The four GUI
typography assertions remain failed and need a separate portable-fixture
follow-up; the automated GUI gate and manual step 5 are not certified. Skipped
or out-of-scope work remains separate:

- No staged/installed Windows packaging, full release sandbox-denial configuration,
  MSIX installation, Store certification, or signing was attempted. The debug
  process tests above are not substitutes for packaged checks.
- Follow [the GUI checklist](../GUI_SMOKE_CHECKLIST.md) later in a normal
  interactive Windows desktop: actual Settings/About/Quit menu location/actions,
  state persistence on quit, keyboard navigation, window recreation, fullscreen
  transitions, multi-monitor/high-DPI scales, icon/taskbar/Alt+Tab/focus, and
  recovery from render/load failures. Automation does not certify these manual
  observations. This host has only one display and one observed scale.
- The two Audience ignored cases and five Python skips remain unexecuted.
- No coverage measurement, GitHub posting, merge, publish, or release was performed.

## Evidence directory

All raw logs and runner scripts are local repository artifacts at
[`artifacts/slint-1-19-windows-20261117-143406/`](../../artifacts/slint-1-19-windows-20261117-143406/).
That directory is ignored by the existing repository policy and is retained on
disk; this report does not imply it was committed or uploaded.

- `commands.jsonl`: actual commands, exit codes, and local timestamps.
- `<label>/*.stdout`, `<label>/*.stderr`: unmodified subprocess output bytes.
- `<label>-diagnostics.txt`: wrapper exits, elapsed time, and raw log filenames.
- `run-validation.ps1`, `retry-python-packaging.ps1`, `run-gui-smoke.py`: exact
  environment setup and executions.
- `test-counts.json`, `test-counts.md`: per-target counts from Cargo output.
- `baseline-diff.txt`, `dependency-audit.json`, `before-*`, `execution-*`, `after-*`:
  source/dependency/state evidence.
- `windows-gui-smoke.txt`, `gui-command.json`, `gui-smoke/`: native development
  GUI results, separate from `ubuntu-original-gui-report/` and CI logs.

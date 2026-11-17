# Slint 1.19.0: Mac step-4 execution

## Identity and environment

This is a fresh local execution on 2026-11-17 (JST, UTC+09:00), starting at
approximately 13:47. Earlier migration and CI results are not counted as local
passes here. Branch: `codex/slint-1-19-migration`; PR:
https://github.com/koichiro/quick-presenter/pull/548.
HEAD at start: `3c376b8fbe33ea506487cc21f42fdf0c72aa2394`, clean working tree.
Implementation: `23648de76b2a4136a5d2f587e0f8353eb6e4e335`.

- macOS 27.0.1, build 26A434; arm64 / `aarch64-apple-darwin`.
- Homebrew Rust 1.100.0 (`0ef1bc386`, 2026-11-09), LLVM 23.1.5;
  Cargo 1.100.0 (`3acc79c0c`, 2026-11-05); Python 3.15.2.
- Xcode 27.2, build 27B52; selected developer directory
  `/Applications/Xcode.app/Contents/Developer`; macOS SDK 27.2.
  Unqualified `xcrun --show-sdk-version` initially tried the absent
  `/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk` and failed.
  `xcrun --sdk macosx --show-sdk-version` succeeded; Cargo builds needed no SDK
  override. No Xcode configuration was changed.
- Existing `pdfium/lib/libpdfium.dylib` is arm64, version 156.0.8076.0.
  It was preserved. The current manifest pins chromium/8111, so
  `python3 scripts/fetch_pdfium.py --out /tmp/slint119-step4/pdfium` fetched and
  hash-verified the pinned arm64 asset into a separate directory. The complete
  default rerun and GUI smoke used
  `PDFIUM_DYNAMIC_LIB_PATH=/tmp/slint119-step4/pdfium/lib/libpdfium.dylib`.
  Initial tests and the Store-feature suite used the pre-existing 8076 library.

During execution, an independent change added diagnostic logging in
`src/gui_smoke.rs::wait_for_presenter_notes_resize`. Later test/GUI builds can
include it. After the GUI binary had been built, that independent edit was
replaced by a patch extracting `sample_presenter_notes_layout` and starting the
settling clock after the first sample. Neither edit was authored or removed by
this task. The completed checks do **not** validate that subsequent timing patch;
the initial fmt/check ran on clean HEAD. Keep this concurrent code change
separate from the documentation and execution evidence produced here. HEAD
remained unchanged; final dirty state includes that source change plus this
report, the two OS assignments and migration-index links.

## Commands and results

Commands ran from the repository root, on the native Mac target. Long commands
used `scripts/run_bounded_command.py` with 1200-second Rust, 600-second Python/
build, or 300-second GUI limits and raw output files. Log inventory and outputs
are retained locally under `artifacts/slint-1-19-macos-2026-11-17/`; that directory
is gitignored. The temporary original outputs are `/tmp/slint119-step4/`.

| Command | Result | Log |
| --- | --- | --- |
| `cargo fmt --check` | exit 0 on initial clean tree | `fmt.stdout`, `fmt.stderr` |
| `cargo check --all-targets --locked` | exit 0, default features | `check.stdout`, `check.stderr` |
| `cargo test --locked` inside Codex sandbox | exit 101; main target 524 passed, 33 failed, 2 ignored; local socket operations denied | `test-default.log` |
| `cargo test --locked` on host | exit 101; 607 passed, 2 failed, 2 ignored before early stop | `test-default-host.log` |
| `cargo test --locked --test control_launch -- --test-threads=1` on host | exit 0; 11 passed | `control-launch-recheck.log` |
| `cargo test --locked --no-fail-fast` on host, PDFium 8111 override | exit 0; 635 passed, 0 failed, 2 ignored | `test-default-complete.log` |
| `cargo check --locked --features macos-store-control --lib --bins` | exit 0; exact CI optional configuration | `check-store.log` |
| `cargo test --locked --features macos-store-control` on host | exit 0; 611 passed, 0 failed, 2 ignored | `test-store.log` |
| `cargo tree --locked -i winit` | exit 0; local `vendor/winit` 0.30.13 on all paths | `tree-winit.log` |
| `cargo tree --locked --prefix none` | exit 0; runtime/compiler/internal Slint crates 1.19.0, registry backend | `tree.log` |
| `python3 -m unittest tests/test_release_sbom.py tests/test_cli_packaging.py tests/test_fetch_pdfium.py tests/test_macos_documents.py` on host | exit 0; 41 passed | `python-policy-host.log` |
| `cargo build --locked` | exit 0; debug GUI smoke binary | `build-gui.log` |

The unused `futures` manifest warning remains. No warnings-as-errors flag was
added. The Python batch initially failed only in its native document bridge
inside the outer sandbox (SIGABRT); the same batch passed on the host. The first
host Rust run failed `cold_open_waits_for_startup_restoration_before_sending_once`
and `failed_startup_is_reported_without_relaunching_or_opening`: the former
reported `IPC_FAILURE: Permission denied (os error 13)`, and the latter returned
7 instead of 3. Both passed in the focused sequential rerun and the complete
parallel rerun. They do not use Slint/PDFium; their source was unchanged by the
migration. This is an intermittent local IPC result, not a confirmed migration
defect or a silently discarded failure. Its precise permission-race cause was
not established. The full rerun also covers targets skipped after the first
failure and validates the current pinned PDFium.

The two ignored tests are
`audience::tests::sustained_local_load_remains_bounded_and_stops_cleanly`
(explicit sustained-load exercise) and
`audience_relay::fixture_tests::external_relay_room_creation_round_trip_and_termination`
(requires a running relay and `QUICK_PRESENTER_TEST_CONFIG`). Neither was run.

### Complete default suite by target

| Target | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| library | 24 | 0 | 0 |
| `qp` | 0 | 0 | 0 |
| main application | 557 | 0 | 2 |
| `audience_window_layout` | 1 | 0 | 0 |
| `control_contract` | 5 | 0 | 0 |
| `control_events` | 3 | 0 | 0 |
| `control_ipc` | 8 | 0 | 0 |
| `control_ipc_windows` | 0 | 0 | 0 |
| `control_launch` | 11 | 0 | 0 |
| `control_launch_windows` | 0 | 0 | 0 |
| `control_lock` | 2 | 0 | 0 |
| `macos_fullscreen_transition` | 3 | 0 | 0 |
| `presenter_layout` | 1 | 0 | 0 |
| `renderer_process` | 13 | 0 | 0 |
| `settings_storage_windows` | 0 | 0 | 0 |
| `store_ipc_probe` example | 7 | 0 | 0 |
| doctests | 0 | 0 | 0 |

All filtered counts in this full run are zero. Windows-only empty targets are
not native Windows test evidence. With the Store feature, the library runs 25,
main 557 (+2 ignored), audience layout 1, control contract 5, fullscreen tracker
3, presenter layout 1, renderer process 13, and example 6 tests. Other targets
run zero; in particular the Unix default-control tests are cfg-disabled on macOS
with this feature. This is not an installed Store/XPC end-to-end certification.

The main test
`tests::quit_callback_and_presenter_close_share_recoverable_event_loop_exit`
uses a software platform for both windows' Quit callbacks, presenter close,
exit failure/retry and audience Settings/About forwarding. The software layout
integration test and source constraint test run without AppKit. The three
vendored fullscreen tracker tests exercise intent/reversal and destruction
without native window creation. These establish automated behavior only.

## Native GUI smoke

Executed the existing `.github/workflows/macos-slide-regression.yml` procedure
against the local debug build, with pinned PDFium 8111, in the host desktop:

```sh
PDFIUM_DYNAMIC_LIB_PATH=/tmp/slint119-step4/pdfium/lib/libpdfium.dylib \
  python3 scripts/run_bounded_command.py --timeout 300 --label macos-gui-smoke \
  --log-dir /tmp/slint119-step4 -- target/debug/quick-presenter \
  --gui-smoke tests/fixtures/marp-speaker-notes.pdf \
  --gui-smoke-report /tmp/slint119-step4/macos-audience-regression.txt
```

**Passed: exit 0, 323 checks passed, 0 failed, elapsed 50.4 seconds.**
The full report is `macos-audience-regression.txt` and process output is
`gui.log` in the local artifact directory. It includes native fullscreen reversal,
window geometry/rendered edges, software layout fixtures and hidden-window
reveal checks. This is a real local automated AppKit run, not a replay of CI.
The smoke binary included the independent diagnostic logging noted above.

## Ubuntu CI investigation

Read-only GitHub inspection retrieved run 43899056288, job 133412757409 and its
`quick-presenter-ubuntu-x64-gui-smoke` artifact. The failed step is
`Verify Ubuntu Debian package`, specifically the installed release application's
Xvfb GUI smoke. Compilation and earlier renderer/control tests succeeded; this
is not a missing build library or Rust compiler failure.

The saved `ubuntu-deb-installed.txt` reports **3 passed, 1 failed**. The initial
window creation/handles passed, then `check_notes_font_size` timed out before
opening a PDF: presenter notes did not settle after a requested logical 800x608
resize within five seconds, with zero geometry retries. The final snapshot was
physical 800x608, `has_notes=false`, font 12, viewport 348x203, content height
203, thirteen zero measurement heights, slide 368x207, notes outer width 368,
menu height 0, main y=54. The failure is a notes-resize settling deadline; the
final snapshot does not identify which intermediate condition kept the wait
active (including scale conversion and quiet-period tracking).

For comparison, PR #538's Ubuntu job in run 41077615687 passed its 215-check
installed GUI smoke on 2026-10-30. That older result does not isolate a Slint
regression from environment/toolchain differences. `src/gui_smoke.rs`,
`src/notes.rs`, and `tests/control_launch.rs` were unchanged in the migration;
the smoke path already omitted production `configure_presenter_menu_bar`, which
explains why the initial Linux snapshot uses a native menu rather than the
production inline menu. That discrepancy is an investigation lead, not a proven
cause of the timeout.

**This session's classification: GUI test synchronization failure, not a build
failure; migration causality was not isolated here.** A concurrent update to the
migration index now reports an Xvfb/software-GL reproduction under a 0.25-CPU
limit: the first layout sample arrived after 12.6 seconds, already beyond the
five-second clock. That independently recorded evidence explains the otherwise
correct final snapshot. Its working-tree fix starts the settling clock after
the first sample, preserving the 200ms quiet period and fitting checks.
This session did not execute that Linux reproduction or validate the subsequent
patch. No green Ubuntu CI result is claimed here. Native Linux and
installed-release confirmation are explicitly assigned in the Linux handoff. Later packaging/SBOM checks skipped after the CI failure must not
be inferred to pass. Local SBOM policy tests above are a different check.

Evidence links:
[failed Ubuntu job](https://github.com/koichiro/quick-presenter/actions/runs/43899056288/job/133412757409),
[failed GUI artifact](https://github.com/koichiro/quick-presenter/actions/runs/43899056288/artifacts/14241686806),
[earlier Ubuntu job](https://github.com/koichiro/quick-presenter/actions/runs/41077615687/job/124139629733).
Downloaded reports/logs are retained with this session's local artifacts.

## Handoffs and remaining work

- [Native Windows Codex assignment](SLINT_1_19_WINDOWS_CODEX.md).
- [Native Linux Codex assignment](SLINT_1_19_LINUX_CODEX.md).

No Windows/Linux native validation, new OS chat creation, remote execution,
coverage, signing, publishing, merging, or release approval was performed here.
No Clippy rerun, full SBOM generation or signed packaging was needed for these
validation-document changes. Slint edition 1.18 and the winit override remain.
Manual native menus/keyboard accelerators, Quit state persistence and cleanup,
Open Recent stress, DPI/multi-monitor behavior, real desktop activation, sandboxed
signed document drops and Store/private-API distribution gates remain step-5/6
work; the automated GUI smoke is not blanket manual or distribution certification.

A concurrent working-tree patch now addresses first-sample initialization time,
as described above and in the migration index's Installed Linux GUI smoke
follow-up. Its independently reported reproduction is separate evidence; this
session's Mac GUI success predates that patch. The
OS handoffs require checking the actual execution-time HEAD and dirty diff and
revalidating any such change before claiming the Ubuntu failure resolved.

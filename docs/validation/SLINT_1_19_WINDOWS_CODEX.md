# Codex assignment: native Windows Slint 1.19.0 validation

## Assignment and boundaries

Execute migration step 4 on this native OS, then leave an English report and raw
logs in the repository. Read `AGENTS.md` first; discuss results with the user in
Japanese. This is an execution request, not a request for a plan. Do not create
other chats, post to GitHub, merge, release, sign, or measure coverage.

Repository: https://github.com/koichiro/quick-presenter
PR: https://github.com/koichiro/quick-presenter/pull/548 (base `v3.0.0`).
Branch: `codex/slint-1-19-migration`.
Implementation commit: `23648de76b2a4136a5d2f587e0f8353eb6e4e335`.
Audited baseline: `3c376b8fbe33ea506487cc21f42fdf0c72aa2394`.
Record the actual HEAD and dirty state before and after execution. If newer
changes exist, inspect and identify them rather than silently substituting the
baseline. Preserve all user changes; never reset, clean, overwrite, or merge them.
If the repository/branch is missing, inspect the existing checkout and safely
fetch/checkout only when that will not displace local work; otherwise report the
specific blocker. No new worktree is required.

Read `docs/validation/SLINT_1_19_MIGRATION.md`, `vendor/README.md`, `build.rs`,
`Cargo.toml`, `.github/workflows/ci.yml`, and the relevant platform portions of
`.github/workflows/build-binaries.yml`. Keep both Slint compiler paths on edition
1.18 and keep the `vendor/winit` 0.30.13 override. Do not adopt edition 1.19 or
remove vendor patches as part of validation. All Slint internal crates must
resolve to 1.19.0; the backend must come from the registry.

## Known evidence to recheck

The Mac step-4 report is `docs/validation/SLINT_1_19_MACOS_STEP4.md`; its results
are historical evidence for this assignment, not native results on your OS.
The Ubuntu job in run 43899056288 failed at the installed Debian package GUI
smoke, after successful compilation and earlier process/control tests:
https://github.com/koichiro/quick-presenter/actions/runs/43899056288/job/133412757409
The downloaded report says 3 passed / 1 failed: before opening a PDF, presenter
notes did not settle after resizing to logical 800x608 within five seconds.
The last snapshot has physical 800x608, no notes, font 12, viewport 348x203,
zero measurement heights, slide 368x207, inline menu height 0, and main y=54.
This is a resize/layout synchronization failure, not a compiler or missing-library
failure. The migration index now records an independent Xvfb/software-GL
reproduction under a 0.25-CPU limit: cold initialization delayed the first sample
to 12.6 seconds, beyond the five-second settling clock. The concurrent fix starts
the clock after that sample. The Mac report's 323-check GUI success predates
this fix; verify the execution-time diff and the fix on your host. Do not infer a
confirmed Slint production defect or a green Ubuntu CI rerun from this evidence.
PR #538's earlier Ubuntu run 41077615687 passed its 215-check GUI smoke, but used
an older dependency/environment snapshot; that comparison does not isolate Slint.
The smoke path also omits production `configure_presenter_menu_bar`, so its
initial Linux menu mode differs from normal startup; this predates migration.
Reproduce and diagnose before changing it. Record whether this known failure
still occurs and preserve the failing report. No verified fix or green Ubuntu rerun was claimed
by the Mac session for this failure. A concurrent working-tree candidate starts
the settling clock after the first layout sample; inspect its status and rerun
relevant checks. Do not treat that unverified edit as a resolved CI result.

## Required regression coverage

The full default suite includes these exact existing tests/targets:

- `cargo test --locked --test presenter_layout`: software Slint layout;
  `presenter_panes_resize_without_losing_notes_or_overlapping_the_footer`.
- `cargo test --locked --test audience_window_layout`: compiled source constraint;
  `audience_window_does_not_cap_fullscreen_size_to_its_windowed_size`.
- `cargo test --locked --bin quick-presenter quit_callback_and_presenter_close_share_recoverable_event_loop_exit`:
  software callback probe for presenter/audience Quit, presenter close,
  recoverable exit errors, and audience Settings/About forwarding.
- `cargo test --locked --test macos_fullscreen_transition`: three pure tracker
  tests included from vendored winit; these run without creating AppKit windows,
  even on other operating systems. They do not certify native fullscreen there.
- `cargo test --locked --test renderer_process` and the platform control targets
  listed below: real subprocess/IPC boundaries, not merely cross compilation.

Use the complete suite first. Do not rerun successful checks merely to produce
individual logs; identify test names and results in that suite. Run focused
commands only for failures or uncovered configurations. Record both ignored
Audience tests (sustained local load and external relay fixture) and their stated
reasons; do not turn them into passes or run the external fixture without setup.

## Reporting and repair policy

Save raw command output and exit statuses under a unique directory such as
`artifacts/slint-1-19-<os>-<date>/` (do not overwrite prior runs), and write
`docs/validation/SLINT_1_19_<OS>_RESULTS.md`. Link it from the migration index.
Include local date/time/timezone, OS/build/kernel, CPU/architecture, Rust/Cargo/
Python/linker/SDK, display session and scale when applicable, HEAD/dirty state,
PDFium version/path/architecture, commands, features/target, and per-target
passed/failed/ignored/filtered counts. An empty cfg-gated target is not evidence
of platform behavior. Separate native execution, software layout, packaged
checks, previous CI evidence, manual checks, and skipped/blocked work.

Prepare required dependencies and execute the standard tests. If Codex's outer
sandbox blocks loopback sockets, IPC, process creation, or desktop access, retain
the failure log and rerun with the appropriate authorized host execution rather
than weakening the application's sandbox. Distinguish host restrictions from
application failures. Diagnose migration-related failures and make only minimal
necessary fixes with relevant checks rerun. Report unrelated failures without
expanding scope. Never report an unexecuted check as passed. After success, rerun
only for subsequent edits or new evidence. Do not introduce `-D warnings`.

Finish in Japanese with the result, blockers and classification, counts, exact
report/log locations, and pending step-5 GUI checks. Coverage, Store certification,
signing, publishing, merging, and releases remain outside this task.

## Native Windows preparation (PowerShell)

Use a normal Windows x64 machine and a native MSVC Rust host (not WSL). Record
Windows version/build and CPU, `rustc -Vv`, `cargo -V`, `python --version`, and
`git status --short` / `git rev-parse HEAD`. Use Rust >=1.95 (the Slint MSRV),
rustfmt, Python 3, Git, and Visual Studio Build Tools with Desktop development
with C++, the MSVC x64 toolchain and Windows SDK/resource compiler. Enter the
appropriate Developer PowerShell if tools are absent from PATH. Record actual
SDK/linker versions. Preserve the lockfile; do not use `cargo update`.

No optional Cargo feature is needed: default features are empty;
`macos-store-control` is macOS-specific and is not a Windows validation matrix.
WiX 5.0.2, .NET, and `makeappx.exe` are required only for the later packaging
workflow, not these Rust tests. Do not install/sign Store packages for this task.

Inspect existing `pdfium/VERSION` and DLL architecture first. If unavailable or
not matching `scripts/pdfium_manifest.json`, use the pinned fetch script. Avoid
`--clean` on an existing directory; use a fresh output directory instead:

```powershell
python scripts/fetch_pdfium.py --asset pdfium-win-x64.tgz --out "$env:TEMP/qp-slint119-pdfium"
$env:PDFIUM_DYNAMIC_LIB_PATH = Join-Path $env:TEMP 'qp-slint119-pdfium/bin/pdfium.dll'
Test-Path $env:PDFIUM_DYNAMIC_LIB_PATH
```

Choose a unique unused output path if that directory already exists. Do not
refresh the manifest or rewrite native licenses. For a valid existing layout,
use the actual full DLL path (`pdfium/bin/pdfium.dll`). Debug tests discover the
repository layout from the repository root; explicit override is for development.
Packaged builds require `QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE=1` to honor it;
packaged checks must instead use their bundled library and clear overrides only
in a scoped child environment. Do not confuse a successful Mac Windows cross
check with native test execution.

## Execution

Run each command from the repository root, capture its output and `$LASTEXITCODE`
immediately, and continue independent checks after a failure. PowerShell native
commands do not reliably throw from `$ErrorActionPreference='Stop'`; explicitly
check their exit codes. `scripts/run_bounded_command.py` can bound subprocesses
and save raw output without native stderr pipeline/descendant handle hangs:

```powershell
python scripts/run_bounded_command.py --timeout 1800 --label windows-tests --log-dir artifacts/slint119-windows-logs -- cargo test --locked --no-fail-fast
```

Use a unique log directory. Required commands:

```powershell
cargo fmt --check
cargo check --all-targets --locked
cargo test --locked
cargo tree --locked -i winit
cargo tree --locked --prefix none
python -m unittest tests/test_fetch_pdfium.py tests/test_release_sbom.py tests/test_cli_packaging.py
```

If the default test command stops at an early target, use `--no-fail-fast` to
collect the remaining results. The following CI subset adds the explicit
no-default-features configuration; run it once:

```powershell
cargo test --locked --no-default-features --lib --test control_contract --test control_events --test control_ipc --test control_ipc_windows --test control_lock --test control_launch --test control_launch_windows
```

Confirm `settings_storage_windows` runs both
`new_settings_do_not_inherit_public_access_from_the_parent` and
`replacing_existing_settings_removes_inherited_public_access`. Confirm
`control_ipc_windows`, `control_launch_windows`, and `renderer_process` actually
execute nonzero native tests. Keep the existing test profile (`debug=1`), which
limits copied AppContainer helper size. The renderer tests create isolated
AppContainers, copy helper binaries, exercise Job Object cleanup (including a
suspended helper), and verify missing-PDFium diagnostics without path leakage.
The machine must permit AppContainer/Job Object APIs and writable local temp
storage with NTFS ACL support. Antivirus scanning can consume helper deadlines;
record this as evidence, do not disable antivirus or extend timeouts blindly.
Settings tests deliberately check private ACLs under publicly inheritable parents;
network shares and non-NTFS storage are not equivalent environments. Report
policy/identity/ACL restrictions as blockers, never bypass confinement.

The full release sandbox-denial check is a separate packaging configuration:
`scripts/check_renderer_sandbox.py` expects a suitably built/staged executable
and fixture. See Build Binaries before attempting it. Unit/process tests are
mandatory; an installed MSIX sandbox probe is not required by this step.

## Step 5 follow-up (do not certify from automation)

Use `docs/GUI_SMOKE_CHECKLIST.md` later in a normal interactive Windows desktop:
native Settings/About/Quit menu location/actions, state saving on quit, keyboard
navigation, window recreation, fullscreen transitions, multi-monitor/high-DPI
scales, icon/taskbar/Alt+Tab/focus, and recovery from render/load failures.
Hosted Windows CI intentionally skips packaged GUI certification. These checks
require explicit observations on Windows; software tests and cross compilation
do not substitute for them. Store installation and signing are later work.

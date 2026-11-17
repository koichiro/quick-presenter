# Codex assignment: native Linux Slint 1.19.0 validation

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

## Native Linux preparation (Bash)

Use native Linux, preferably Ubuntu x86_64 matching CI. Record `/etc/os-release`,
`uname -a`, `rustc -Vv`, `cargo -V`, `python3 --version`, `cc --version`,
`git status --short`, `git rev-parse HEAD`, and session type/display availability
(`XDG_SESSION_TYPE`, `DISPLAY`, `WAYLAND_DISPLAY`; do not publish launcher-token
values). Rust must be >=1.95 with rustfmt. Install Python 3, Git and a C/C++
build toolchain as needed. Do not run the application or Rust tests with sudo.

The native build libraries below are taken from `.github/workflows/ci.yml`:

```bash
sudo apt-get update
sudo apt-get install -y libegl1-mesa-dev libfontconfig1-dev libgl1-mesa-dev   libudev-dev libwayland-dev libx11-dev libxcb1-dev libxkbcommon-dev pkg-config
```

For the CI GUI/package reproduction, Build Binaries additionally installs:

```bash
sudo apt-get install -y desktop-file-utils dpkg-dev libgtk-3-bin   libxkbcommon-x11-0 xauth xvfb
```

On another distribution, map packages and record differences. Preserve the
lockfile and do not use `cargo update`. Default Cargo features are empty;
`macos-store-control` is not a Linux configuration and need not be tested here.

Inspect `pdfium/VERSION`, library architecture (`file pdfium/lib/libpdfium.so`),
and the pinned manifest. If missing or mismatched, fetch to a fresh directory:

```bash
python3 scripts/fetch_pdfium.py --out /tmp/qp-slint119-pdfium
export PDFIUM_DYNAMIC_LIB_PATH=/tmp/qp-slint119-pdfium/lib/libpdfium.so
```

Choose a unique path if occupied. The script detects x64 versus arm64; use
`--asset pdfium-linux-x64.tgz` only for a native x64 host. Do not use `--clean`
on user artifacts or update the manifest/licenses. Debug tests discover the
repository `pdfium/lib/libpdfium.so` from the repository root. Overrides must
point to the full library file. Packaged checks should use bundled PDFium with
both override variables removed in the child process.

## Required automatic execution

Run from the repository root and save output/exit codes (use `set -o pipefail`
when piping to tee). These are native tests, not a Mac Docker cross check:

```bash
cargo fmt --check
cargo check --all-targets --locked
cargo test --locked
cargo tree --locked -i winit
cargo tree --locked --prefix none
python3 -m unittest tests/test_fetch_pdfium.py tests/test_release_sbom.py tests/test_cli_packaging.py
cargo test --locked --no-default-features --lib --test control_contract --test control_events --test control_ipc --test control_ipc_windows --test control_lock --test control_launch --test control_launch_windows
```

Do not stop independent checks after a failure. If the full test command stops
before later targets, use `cargo test --locked --no-fail-fast` to capture them.
`scripts/run_bounded_command.py --timeout 1800 --label linux-tests --log-dir <unique-directory> -- cargo test --locked --no-fail-fast`
provides bounded subprocess execution and raw logs. Retain existing deadlines
and application sandbox enforcement.

Confirm the full suite covers
`renderer_helper::tests::linux_environment_keeps_only_reviewed_keys_and_debug_hooks`
environment allowlisting (including `XDG_ACTIVATION_TOKEN` and `DESKTOP_STARTUP_ID`) and these exact
`renderer_process` tests:
`helper_does_not_inherit_broker_secrets_and_renders_with_preserved_locale`,
`broker_survives_helper_fcntl_signal_attack_and_renders_pdf`, and
`unavailable_landlock_and_flatpak_modes_fail_closed`.
The helper requires supported Landlock/seccomp confinement; restricted containers,
older kernels, or a Flatpak environment can prevent normal rendering. Capture
kernel/policy diagnostics and distinguish the deliberately injected fail-closed
cases from an unusable host. Do not remove sandbox restrictions to make tests
pass. Local HTTP/WebSocket and Unix-domain IPC tests also need host permission
for local sockets. Windows-only targets may report zero tests on Linux.

## Reproduce the Ubuntu GUI failure

First build the debug GUI and execute the existing complete smoke under Xvfb:

```bash
cargo build --locked
xvfb-run -a -s '-screen 0 1920x1080x24'   env LIBGL_ALWAYS_SOFTWARE=1 target/debug/quick-presenter   --gui-smoke tests/fixtures/marp-speaker-notes.pdf   --gui-smoke-report /tmp/slint119-linux-xvfb.txt
```

Use unique report paths and a bounded wrapper (e.g. 300 seconds). This creates
windows in Xvfb and uses Mesa software GL; it differs from the pure Slint
software-platform tests and from an installed desktop. Preserve ambient backend
selection for the first run. Record backend/renderer and actual scale factor if
investigating the resize deadline. If adding instrumentation, log the requested
and logical/physical sizes, scale factor, snapshot changes and fitting predicate;
remove only your instrumentation afterward. Do not mask the failure by weakening
assertions or increasing waits without evidence.

The original failure used the *installed release Debian package*. A passing debug
smoke alone does not close it. On a disposable Linux VM, reproduce the exact
`Verify Ubuntu Debian package` step in Build Binaries: build release binaries,
stage bundled PDFium and desktop assets, use `scripts/build_linux_deb.sh`, then
run the extracted and installed smoke/sandbox/CLI checks as documented there.
Read the script's arguments and the workflow first. Do not replace an existing
user installation just to reproduce CI; report installed-package reproduction
as blocked if the machine is not disposable. Keep unsigned package diagnostics
separate from release/distribution approval. Capture the installed GUI report
and exit status. Do not claim that SBOM/package verification steps skipped after
this failing CI step have passed.

Xvfb covers an X11 virtual display, without a normal compositor/desktop launcher.
For additional interactive runs use an available real X11 or Wayland session,
recording which was used; XWayland is not native Xorg. Missing Wayland is a skip,
not a pass. Never fabricate desktop-launch tokens. Slint 1.19 consumes real tokens
only at the first native window; renderer children must never inherit them.

## Step 5 follow-up (separate native observations)

Use `docs/GUI_SMOKE_CHECKLIST.md` and
`docs/validation/ISSUE304_LINUX_DESKTOP.md` later for real desktop-entry launch,
first-window activation/token consumption, repeated launches, X11 icon/WM_CLASS,
Wayland app ID/window recreation, focus, inline menus, notes spacing, fullscreen,
multiple monitors and mixed DPI. Preserve the inline Linux menu. Record native
Wayland and X11/XWayland separately. Xvfb cannot certify launcher focus, icon
latency or real compositor fullscreen. Signed distribution/release and other
operating systems are outside this assignment.

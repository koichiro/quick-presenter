# Slint 1.19.0 migration

## Scope

The runtime and build compiler are pinned to 1.19.0. `build.rs` explicitly selects
Slint edition 1.18 for both `ui/app.slint` and the generated Linux source.
Edition 1.19 adoption is outside this change. Slint 1.19.0 requires Rust 1.95;
the Linux cross-build container currently uses Rust 1.96.

The published backend replaces the complete former 1.18.1 backend override:

- Slint #14331 rejects stale native menu events across rebuilds and adapters.
- Slint #14357 resets hidden macOS windows before their next frame is revealed.
- Slint #14359 assigns the native icon before window creation.
- Slint #14360 consumes the Linux activation token once at native window creation.

Settings and About retain their callbacks and use `preferences` and `about`
roles. Quit uses the `quit` role and a Rust callback shared with presenter close.
The audience window also declares these roles on macOS, preventing a focus
change from installing Slint's predefined Quit item (which terminates directly).
Its Settings/About actions reveal the presenter and open the existing panels.
On macOS, Slint moves these roles into the application menu and assigns
Command-comma and Command-Q. On Windows, role items retain their declared menu
locations. Linux retains its separate inline menu and notes typography.
The empty native About-menu mutation helper is removed.

The winit 0.30.13 override remains unchanged. Its private-API removal, fullscreen
input/intent changes, and sandboxed URL-drop hooks are still required.
License and SBOM collection discover Cargo dependencies dynamically; the backend
is now a registry dependency and winit remains a modified local dependency.
Earlier validation documents describe their historical vendored builds.
A file-by-file comparison against the published 1.18.1 crate found local changes
only in the two Cargo manifests, `event_loop.rs`, `lib.rs`, `muda.rs`,
`startup_notify.rs`, and `winitwindowadapter.rs`; all belong to the behaviors
listed above. No additional backend patches were discarded.

## Linux activation token timing

The old backend cleared `XDG_ACTIVATION_TOKEN` and `DESKTOP_STARTUP_ID` during
backend initialization. Upstream 1.19.0 reads and clears them in `ensure_window`,
just before the first native window is created. Subsequent windows receive no
launcher token. The GUI can request a startup PDF before its deferred Linux show,
so renderer startup must not inherit these tokens during that interval.

`configure_linux_renderer_environment` already clears the child environment and
allows only reviewed keys; neither launcher token is allowed, including in debug
probes. Its unit test explicitly covers both keys. Startup logging, watching,
control installation, and audience installation start threads rather than child
processes. User-triggered dialogs and browser launches occur after initial show.
No additional global environment mutation is needed. The old integration test
that included the deleted private backend module is removed; it cannot validate
the registry backend. Native desktop-launch checks remain necessary.

## Validation boundary

Required local checks: `cargo fmt --check`, `cargo check`, and `cargo test`.
A software-platform exit probe tests both windows' Quit callbacks, presenter
close, exit failures/retry, and audience Settings/About forwarding.
The existing software-rendered layout tests exercise the compiled UI without
AppKit window creation. The winit fullscreen transition tests remain intact.
Coverage, release, merge, and signing are outside this task.

Native checks are still pending for the upgraded dependency set:

- macOS Settings/About placement and Command-comma; Quit/Command-Q startup state
  persistence and cleanup; Open Recent stress; hidden-window frames.
- Linux desktop-entry activation, first-window token use, X11 icon timing,
  repeated launches, and Wayland window recreation.
- Windows menu placement/actions and native window behavior.
- Signed distribution, sandboxed document drops, and Store private-API scanning.

Historical procedures remain in `ISSUE304_LINUX_DESKTOP.md`,
`ISSUE400_HIDDEN_SLIDE.md`, and `PR397_MACOS_STARTUP_RESTORATION.md`.

## Local results (2026-11-17)

The migration is based on the updated `v3.0.0` branch, retaining its dependency
security updates and Windows settings ACL hardening.

- `cargo fmt --check`: passed.
- `cargo check --locked`: passed on macOS arm64 with Rust 1.100.0.
- `cargo test --locked`: passed (including the new exit/role forwarding probe).
- `scripts/check_linux_cross_docker.sh`: passed with Rust 1.96 for
  `x86_64-unknown-linux-gnu`, including generated `app-linux.slint`.
- `python3 -m unittest tests/test_release_sbom.py`: 21 passed.
- `cargo tree --locked -i winit`: all paths resolve to the unchanged local
  `vendor/winit` 0.30.13 override; Slint's backend resolves to registry 1.19.0.

The existing manifest warning for unused `futures` remains outside this migration.

## Step 4 execution and OS handoffs (2026-11-17)

The earlier local results above are historical. Fresh Mac execution, retries,
PDFium versions, concurrent working-tree changes, native GUI evidence and the
Ubuntu CI resize timeout investigation are recorded separately:

- [Mac step-4 execution report](SLINT_1_19_MACOS_STEP4.md).
- [Self-contained native Windows Codex assignment](SLINT_1_19_WINDOWS_CODEX.md).
- [Self-contained native Linux Codex assignment](SLINT_1_19_LINUX_CODEX.md).

The OS assignments request native execution and saved evidence; they do not
assert that Windows/Linux have already been validated or authorize release.

## Installed Linux GUI smoke follow-up

The first PR build passed compilation, unit tests, packaging, and headless PDF
opening, then failed the installed Debian GUI smoke's first notes resize.
Its final geometry and 12px placeholder font were correct. The five-second
settling deadline had already expired before the first layout observation.

The failure was reproduced under Xvfb with software OpenGL and a 0.25-CPU limit:
the first observation arrived at 12.6 seconds and immediately failed with the
same 800x608 geometry. The harness now samples the initial input/render pass
before starting its settling clock. This keeps cold renderer/font setup out of
the observation budget. Exact snapshot equality, the 200ms quiet period, the
five-second deadline, geometry retry behavior, and font-fitting checks remain
unchanged. Production layout and PDF playback code are unaffected.

With the fix, the same throttled startup passed the notes resize and the first
77 GUI checks. The CPU limit was removed after the first 25 seconds for the rest
of the run. This emulated x86_64 Docker environment then failed the renderer
helper handshake, so it does not establish full PDF GUI success; native Ubuntu
CI must complete the installed-package smoke. `cargo fmt --check`,
`cargo check --locked`, and `cargo test --locked` (635 passed, 2 ignored) passed
locally after the harness change.

## Native Linux step-4 execution (2026-11-17)

See [the native Linux results](SLINT_1_19_LINUX_RESULTS.md) for the exact HEAD,
host, commands, per-target counts, raw evidence and remaining package/desktop
boundaries. Rust tests passed (629 passed, 2 ignored); X11/Xvfb GUI smoke passed
215 checks. Native Wayland comment-width assertions failed in both full and
focused smoke. These observations do not claim a green installed-package Ubuntu
CI rerun or completion of the manual step-5 desktop checks.

## Native Linux step-5 follow-up (2026-11-17)

The [Linux step-5 report](SLINT_1_19_LINUX_STEP5.md) records the native Wayland
comment-width regression, its persistent-presenter measurement workaround,
630 passing Rust tests (2 ignored), and 215 passing GUI checks on each tested
Wayland/XWayland configuration. The separate [About readability follow-up](SLINT_1_19_LINUX_ABOUT.md)
records the layout repair and completed normal/minimum-size desktop checks.
These changes are transferred on `codex/slint-1-19-linux-validation`, based on
`deee3b6`, for integration and review. Raw evidence remains local and uncommitted.
Physical multiple-monitor/mixed-DPI acceptance and the pre-existing Linux
Bring-to-Front focus limitation remain explicitly outside the passing results.

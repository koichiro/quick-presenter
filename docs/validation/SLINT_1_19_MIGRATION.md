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

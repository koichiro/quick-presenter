# Temporary Slint backend patches

`i-slint-backend-winit` is vendored from the published Slint 1.18.1 crate. Its
upstream licenses remain in `i-slint-backend-winit/LICENSES`.

Quick Presenter carries this patch for native menu identity and orderly macOS
shutdown:

- use muda 0.21.0, which keeps a macOS native menu item's Rust owner alive for
  as long as AppKit retains the item;
- include a process-wide, monotonically increasing menu generation in every
  muda item ID;
- ignore activation events from an older generation after Slint rebuilds the
  native menu;
- adapt Slint 1.18.1's shortcut conversion to muda 0.21.0's keyboard API; and
- route the macOS Quit menu and Command-Q through event-loop exit so startup
  settings can be saved after the loop returns, instead of terminating directly
  through AppKit's predefined Quit item.

The generation check prevents a retained native item from activating whichever
item later occupies the same flattened menu index. A stale activation is
dropped instead of opening the wrong PDF.

Track these upstream changes before removing the patch:

- <https://github.com/slint-ui/slint/pull/13119>
- <https://github.com/tauri-apps/muda/pull/361>

Remove the override in the repository `Cargo.toml` after a Slint release uses a
fixed muda version and preserves item identity, or rejects stale activation,
across native menu updates. Preserve orderly Quit handling as well. Re-run the
macOS Open Recent stress checks and startup restoration checks before removing it.

## Linux startup metadata (#304)

The backend also carries Linux-only changes for early shell integration:

- Copy the Slint window icon to winit's creation attributes before mapping an
  X11 window.
- Read and clear the launcher activation environment during backend startup,
  then pass the token for the connected backend to the first native window only.

Preserve these behaviors when upgrading the vendored crate until upstream Slint
provides them. The application's integration harness includes the exact backend
module so these tests also run with the normal `cargo test` command:

```sh
cargo test --test linux_startup_notify
```

The reporter confirmed improved taskbar icon timing and correct Alt+Tab icon
grouping on GNOME. Repeated desktop-entry launches and Wayland window recreation
checks are documented in
[`docs/validation/ISSUE304_LINUX_DESKTOP.md`](../docs/validation/ISSUE304_LINUX_DESKTOP.md).
Retain the app identity, early X11 icon, and one-time launch token behavior when
upgrading the backend.

## macOS hidden-window frames

After hiding a macOS window, reset its first-frame state so the next Slint
show renders updated content before revealing the window. If Metal cannot
render while unmapped, the existing `RevealOnFirstFrame` guard keeps the
previous backing image hidden until a frame is submitted. This complements
the application's use of Slint show/hide before native focus operations;
see [the #400 validation](../docs/validation/ISSUE400_HIDDEN_SLIDE.md).

Retain this behavior, or an equivalent upstream fix, when updating Slint.
Re-run the signed hidden-window GUI checks before removing it.

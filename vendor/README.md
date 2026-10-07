# Temporary Slint native menu patch

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

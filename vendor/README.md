# Vendored dependencies

## winit 0.30.13

`winit` remains a local crates.io override for all Slint and application users.
See [QUICK_PRESENTER_PATCH.md](winit/QUICK_PRESENTER_PATCH.md) for the private
CoreGraphics API removal, fullscreen input and reversal handling, and sandboxed
URL drop hooks. Its upstream license remains in `winit/LICENSE`.

## Slint backend migration

Quick Presenter uses the published `i-slint-backend-winit` 1.19.0 crate.
The former Slint 1.18.1 backend override has been removed. Slint 1.19.0 includes
stale native menu event rejection, macOS hidden-window frame reset, early native
window icon assignment, and one-time Linux activation token consumption.
Settings, About, and Quit use Slint menu roles; Quit and Command-Q invoke the
same Rust exit path as closing the presenter, preserving post-loop cleanup and
startup settings persistence.

See [the migration validation notes](../docs/validation/SLINT_1_19_MIGRATION.md)
for the activation-token timing difference and remaining native checks.

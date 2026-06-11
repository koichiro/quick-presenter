# Packaging Notes

Quick Presenter currently builds raw development binaries in CI. Full installer
or app bundle packaging is intentionally separate from the binary build workflow.

## Application Icons

The source icon assets are documented in `docs/ICONS.md`.

### macOS

Development and raw binary runs embed `assets/icons/macos/QuickPresenter.icns`
into the executable and set it through AppKit at runtime. This makes the app icon
available to macOS app switching surfaces such as Cmd+Tab without depending on a
filesystem path next to the executable.

Future `.app` packaging should also wire the bundle metadata:

- Copy `assets/icons/macos/QuickPresenter.icns` to `Contents/Resources/`.
- Set `CFBundleIconFile` to `QuickPresenter.icns`.
- Keep `CFBundleName` and `CFBundleDisplayName` as `Quick Presenter`.
- Keep the executable name as `qp` unless a packaging decision explicitly changes
  the bundle layout.

Manual verification:

- Run `cargo run --bin qp -- --pdf tests/fixtures/marp-speaker-notes.pdf`.
- Press Cmd+Tab and confirm the Quick Presenter icon is shown.

### Windows

Raw Windows `qp.exe` builds embed `assets/icons/windows/quick-presenter.ico`
as an executable resource from `build.rs`. This keeps the development binary
name as `qp.exe` while allowing Windows shell surfaces to discover the Quick
Presenter icon.

The `build-binaries.yml` workflow runs on `windows-latest`, builds `qp.exe`, and
extracts the associated executable icon into `qp-associated-icon.png` inside the
uploaded Windows artifact. That preview is a CI smoke test that the executable
has an associated icon resource.

CI cannot fully verify final Windows shell behavior because Explorer, Alt+Tab,
and taskbar rendering depend on an interactive Windows session and icon cache
state. Use a real Windows machine for final acceptance.

Manual verification:

- Download the `quick-presenter-windows-x64` artifact or build locally with
  `cargo build --release --bin qp`.
- Inspect `qp.exe` in Explorer.
- Run the app and confirm the icon appears in Alt+Tab and the taskbar.
- If Explorer shows a stale generic icon, copy the artifact to a fresh path and
  retry before treating it as a failure.

Future installer packaging is tracked separately from raw executable icon
embedding.

### Linux

Linux binary artifacts stage desktop metadata with
`scripts/stage_linux_desktop_assets.sh`. The staged layout includes a desktop
entry and PNG icons from `assets/icons/png/` installed under the hicolor icon
theme with the icon name `quick-presenter`.

The desktop entry is:

- `share/applications/quick-presenter.desktop`

It uses:

```ini
Name=Quick Presenter
Exec=qp %f
Icon=quick-presenter
```

The staged hicolor icon paths are:

- `share/icons/hicolor/16x16/apps/quick-presenter.png`
- `share/icons/hicolor/24x24/apps/quick-presenter.png`
- `share/icons/hicolor/32x32/apps/quick-presenter.png`
- `share/icons/hicolor/48x48/apps/quick-presenter.png`
- `share/icons/hicolor/64x64/apps/quick-presenter.png`
- `share/icons/hicolor/128x128/apps/quick-presenter.png`
- `share/icons/hicolor/256x256/apps/quick-presenter.png`
- `share/icons/hicolor/512x512/apps/quick-presenter.png`
- `share/icons/hicolor/1024x1024/apps/quick-presenter.png`

To stage the Linux desktop metadata locally:

```sh
scripts/stage_linux_desktop_assets.sh /tmp/quick-presenter-linux-stage
```

The `build-binaries.yml` workflow runs the staging script for the Linux artifact
and validates:

- the desktop entry exists and contains the expected `Name`, `Exec`, and `Icon`,
- the desktop entry passes `desktop-file-validate`,
- each expected hicolor icon file exists and is non-empty,
- `gtk-update-icon-cache` can process the staged hicolor tree.

CI cannot fully verify launcher, app switcher, dock, or taskbar rendering because
those require an interactive Linux desktop session. Use a real desktop
environment for final acceptance.

Manual verification:

- Download the `quick-presenter-ubuntu-x64` artifact or build and stage locally.
- Copy the staged metadata into a test prefix:

```sh
cp -r share/applications ~/.local/share/
cp -r share/icons ~/.local/share/
update-desktop-database ~/.local/share/applications || true
gtk-update-icon-cache ~/.local/share/icons/hicolor || true
```

- Confirm `Quick Presenter` appears in the launcher.
- Launch the app through the desktop entry.
- Confirm the icon appears in launcher search, app switcher, and the desktop
  environment's dock, taskbar, or panel.

Full `.deb`, `.rpm`, AppImage, Flatpak, or distro package creation is tracked
separately from desktop metadata staging.

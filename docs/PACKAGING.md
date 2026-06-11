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

Windows packaging should use `assets/icons/windows/quick-presenter.ico`.

For a raw `qp.exe`, the icon must be embedded as a Windows executable resource
before Explorer, Alt+Tab, and the taskbar can reliably show it. Add that as a
Windows-only build or packaging step when Windows packaging is introduced.

Manual verification:

- Inspect `qp.exe` in Explorer.
- Run the app and confirm the icon appears in Alt+Tab and the taskbar.

### Linux

Linux desktop packaging should install PNG icons from `assets/icons/png/` into
the hicolor icon theme and reference the icon from a desktop entry.

Expected install layout examples:

- `share/icons/hicolor/16x16/apps/quick-presenter.png`
- `share/icons/hicolor/32x32/apps/quick-presenter.png`
- `share/icons/hicolor/48x48/apps/quick-presenter.png`
- `share/icons/hicolor/64x64/apps/quick-presenter.png`
- `share/icons/hicolor/128x128/apps/quick-presenter.png`
- `share/icons/hicolor/256x256/apps/quick-presenter.png`
- `share/icons/hicolor/512x512/apps/quick-presenter.png`

The desktop entry should use:

```ini
Icon=quick-presenter
```

Manual verification depends on the desktop environment, but should include the
launcher, app switcher, and taskbar or dock equivalent.

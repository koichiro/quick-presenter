# Packaging Notes

Quick Presenter currently builds raw development binaries in CI. Full installer
or app bundle packaging is intentionally separate from the binary build workflow.

## Bundled PDFium

Release packages and binary artifacts must include PDFium. Quick Presenter does
not download PDFium at first launch for MVP releases because presentation startup
must work without network access.

Runtime PDFium lookup order:

1. `PDFIUM_DYNAMIC_LIB_PATH`
2. `pdfium/` next to the running executable
3. `pdfium/` one directory above the running executable, for layouts such as
   `bin/qp` plus a sibling `pdfium/`
4. macOS app bundle locations:
   - `Contents/Resources/pdfium/`
   - `Contents/Frameworks/pdfium/`
   - `Contents/MacOS/pdfium/`
5. repository-local `pdfium/` under the current working directory for
   development runs
6. system PDFium as a final fallback

For each `pdfium/` directory, the app checks `lib/`, `bin/`, then the directory
itself for the platform PDFium library name.

Package builders must keep the bundled `pdfium/` directory and its license files
with the installed application. This applies to the macOS app bundle, Windows
installer, Linux package artifacts, and packaged artifact smoke tests tracked by
#87, #94, #96, and #88.

## Packaged Artifact Smoke Tests

Quick Presenter provides a non-interactive smoke mode for packaged artifact
validation:

```sh
qp --smoke-open-pdf tests/fixtures/marp-speaker-notes.pdf
```

This mode does not create Slint windows. It opens the PDF through the same
PDFium lookup path as normal startup, renders the first page at a small size, and
exits with status `0` on success. It is intended for CI and package validation,
not for end-user presentation playback.

The `build-binaries.yml` workflow runs this smoke mode from outside the
repository working directory against the staged artifacts with
`PDFIUM_DYNAMIC_LIB_PATH` unset. This catches missing executables, missing
bundled PDFium files, and broken relative PDFium lookup.

## Application Icons

The source icon assets are documented in `docs/ICONS.md`.

### macOS

Development and raw binary runs embed `assets/icons/macos/QuickPresenter.icns`
into the executable and set it through AppKit at runtime. This makes the app icon
available to macOS app switching surfaces such as Cmd+Tab without depending on a
filesystem path next to the executable.

macOS app bundle artifacts are staged with `scripts/stage_macos_app_bundle.sh`.
The staged layout is:

```text
Quick Presenter.app/
  Contents/
    Info.plist
    MacOS/
      qp
    Resources/
      QuickPresenter.icns
      pdfium/
      licenses/
        QuickPresenter-LICENSE.txt
        PDFium-LICENSE.txt
```

The bundle metadata uses:

- `CFBundleName`: `Quick Presenter`
- `CFBundleDisplayName`: `Quick Presenter`
- `CFBundleExecutable`: `qp`
- `CFBundleIconFile`: `QuickPresenter`
- `NSPrincipalClass`: `NSApplication`

The bundled PDFium directory is copied to `Contents/Resources/pdfium/`, which is
covered by the runtime lookup order documented above.

To stage the app bundle locally:

```sh
python3 scripts/fetch_pdfium.py
cargo build --release --bin qp
scripts/stage_macos_app_bundle.sh /tmp/quick-presenter-macos
```

The `build-binaries.yml` workflow stages the `.app` inside the macOS artifact
and validates:

- `Contents/Info.plist` is valid,
- bundle display name, executable, icon file, package type, and principal class
  are set,
- `Contents/MacOS/qp` is executable,
- `QuickPresenter.icns` exists in `Contents/Resources/`,
- bundled PDFium and license files are present.
- the bundled app executable can run `--smoke-open-pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`.

Manual verification:

- Stage the app bundle locally.
- Run `open "/tmp/quick-presenter-macos/Quick Presenter.app"`.
- Open `tests/fixtures/marp-speaker-notes.pdf` without `PDFIUM_DYNAMIC_LIB_PATH`.
- Press Cmd+Tab and confirm the Quick Presenter icon is shown.

Developer ID signing, notarization, `.dmg` creation, and universal binary
packaging are tracked separately from the first `.app` bundle staging flow.

### Windows

Raw Windows `qp.exe` builds embed `assets/icons/windows/quick-presenter.ico`
as an executable resource from `build.rs`. This keeps the development binary
name as `qp.exe` while allowing Windows shell surfaces to discover the Quick
Presenter icon.

The `build-binaries.yml` workflow runs on `windows-latest`, builds `qp.exe`, and
extracts the associated executable icon into `qp-associated-icon.png` inside the
uploaded Windows artifact. That preview is a CI smoke test that the executable
has an associated icon resource.

The workflow also runs `qp.exe --smoke-open-pdf` from the staged artifact with
`PDFIUM_DYNAMIC_LIB_PATH` unset.

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
- the staged `qp` binary can run `--smoke-open-pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`.

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

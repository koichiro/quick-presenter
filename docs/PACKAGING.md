# Packaging Notes

Quick Presenter builds release-oriented package artifacts in the `Build
Binaries` workflow. The workflow stages bundled PDFium, platform metadata,
licenses, and installer or app bundle layouts before uploading artifacts.

The GUI application executable is `quick-presenter` on Unix-like platforms and
`quick-presenter.exe` on Windows. The short `qp` command name is reserved for a
future automation-oriented CLI entrypoint and is intentionally not used by the
GUI binary.

## Bundled PDFium

Release packages and binary artifacts must include PDFium. Quick Presenter does
not download PDFium at first launch for MVP releases because presentation startup
must work without network access.

Runtime PDFium lookup order:

1. macOS app bundle locations:
   - `Contents/Resources/pdfium/`
   - `Contents/Frameworks/pdfium/`
2. `pdfium/` next to the running executable
3. `pdfium/` one directory above the running executable, for layouts such as
   `bin/quick-presenter` plus a sibling `pdfium/`
4. `Contents/MacOS/pdfium/` for raw macOS bundle development layouts

For each `pdfium/` directory, the app checks `lib/`, `bin/`, then the directory
itself for the platform PDFium library name.

Debug builds also allow the repository-local `pdfium/` directory under the
current working directory and system PDFium as development conveniences.
Packaged builds do not use current-directory lookup or system PDFium fallback.

`PDFIUM_DYNAMIC_LIB_PATH` is always available in debug builds. In packaged
non-debug builds, it is ignored unless `QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE=1`
is also set. This keeps release startup deterministic by default while
preserving an explicit troubleshooting escape hatch.

Package builders must keep the bundled `pdfium/` directory and its license files
with the installed application. This applies to the macOS app bundle, Windows
installer, Linux package artifacts, and packaged artifact smoke tests tracked by
#87, #94, #96, and #88.

## PDFium Version Updates

PDFium downloads are pinned in
[`scripts/pdfium_manifest.json`](../scripts/pdfium_manifest.json). The manifest
stores the bblanchon/pdfium-binaries release tag and SHA256 for each supported
asset.

`scripts/fetch_pdfium.py` verifies the pinned archive checksum and rejects unsafe
archive member paths before extraction. The extraction target must still be a
clean or trusted directory, because pre-existing files or symlinks can affect the
resulting filesystem state. CI and release packaging should fetch PDFium with
`--clean`, which removes and recreates only the configured output directory
before extraction.

To update PDFium to the latest upstream release:

```sh
python3 scripts/fetch_pdfium.py --update-manifest
python3 -m unittest tests/test_fetch_pdfium.py
python3 scripts/fetch_pdfium.py --clean
```

To pin a specific upstream release tag:

```sh
python3 scripts/fetch_pdfium.py --update-manifest --version chromium/7891
python3 -m unittest tests/test_fetch_pdfium.py
python3 scripts/fetch_pdfium.py --clean
```

Review the manifest diff, confirm the fetched `pdfium/VERSION`, and run the
normal Rust verification before opening the release-engineering pull request.

## Source Code for Binary Releases

Quick Presenter is licensed under `GPL-3.0-or-later`. Paid distribution is
allowed, including paid store distribution, but every binary distribution must
preserve the recipient's GPL freedoms and provide the corresponding source code
for that exact build.

Official release binaries must be built from a tagged Git revision. The release
page must include or link to the source archive for the same tag as the binary
artifact. Store listings should include the project/source URL, and store
submission notes should identify how recipients can obtain the corresponding
source for the submitted binary.

The source archive must include `Cargo.lock` so recipients can identify the
exact Rust dependency sources used for the build. If a release ever vendors,
patches, or otherwise modifies dependency source, those modified sources must be
included or linked as part of the corresponding source for that binary.

Packaged artifacts should include `packaging/SOURCE-OFFER.txt` in their
installed `licenses/` directory as `QuickPresenter-SOURCE-OFFER.txt`. This
notice is included by the current macOS app bundle and Windows MSI staging
flows. Future Linux package formats tracked by #96 must include the same notice
next to the Quick Presenter and PDFium license files.

Before submitting App Store or Microsoft Store builds, re-check the current
store terms against GPLv3 requirements, including source availability and any
installation-information obligations for the target package type. This document
is release engineering guidance, not legal advice.

## Packaged Artifact Smoke Tests

Quick Presenter provides a non-interactive smoke mode for packaged artifact
validation:

```sh
quick-presenter --smoke-open-pdf tests/fixtures/marp-speaker-notes.pdf
```

This mode does not create Slint windows. It opens the PDF through the same
PDFium lookup path as normal startup, renders the first page at a small size, and
exits with status `0` on success. It is intended for CI and package validation,
not for end-user presentation playback.

On Windows release builds, `quick-presenter.exe` uses the Windows GUI subsystem so packaged
MSI/MSIX launches do not create an extra console window. Smoke validation on
Windows should rely on the process exit status; debug builds keep the console
subsystem for local CLI diagnostics.

The `build-binaries.yml` workflow runs this smoke mode from outside the
repository working directory against the staged artifacts with
`PDFIUM_DYNAMIC_LIB_PATH` and `QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE` unset.
This catches missing executables, missing bundled PDFium files, and broken
relative PDFium lookup.

This smoke mode is not a GUI validation path. Before publishing v1.0.0 or later
release artifacts, run the interactive
[GUI release smoke checklist](GUI_SMOKE_CHECKLIST.md) on macOS, Windows, and
Ubuntu Linux to validate presenter and slide windows, fullscreen transitions,
menu actions, keyboard focus, black screen mode, and on-screen readability.

Packaged builds also provide a semi-automated GUI smoke mode:

```sh
quick-presenter --gui-smoke tests/fixtures/marp-speaker-notes.pdf \
  --gui-smoke-report /tmp/quick-presenter-gui-smoke.txt
```

This mode creates the Slint presenter and slide windows, opens and renders the
PDF, drives shared Rust presentation commands, writes a text report, and exits
non-zero on failed app-observable checks. It still does not replace the manual
desktop-session checks for OS-owned shell, focus, and visual readability
behavior.

`build-binaries.yml` runs this GUI smoke mode from the installed Ubuntu package
under Xvfb and uploads the report as `quick-presenter-ubuntu-x64-gui-smoke`.
The workflow uploads explicit skip-reason reports for macOS and Windows because
hosted CI does not provide the normal desktop sessions needed to make platform
GUI behavior a reliable release gate.

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
      quick-presenter
    Resources/
      QuickPresenter.icns
      pdfium/
      licenses/
        QuickPresenter-LICENSE.txt
        QuickPresenter-SOURCE-OFFER.txt
        PDFium-LICENSE.txt
```

The bundle metadata uses:

- `CFBundleName`: `Quick Presenter`
- `CFBundleDisplayName`: `Quick Presenter`
- `CFBundleExecutable`: `quick-presenter`
- `CFBundleIconFile`: `QuickPresenter`
- `NSPrincipalClass`: `NSApplication`

The bundled PDFium directory is copied to `Contents/Resources/pdfium/`, which is
covered by the runtime lookup order documented above.

To stage the app bundle locally:

```sh
python3 scripts/fetch_pdfium.py --clean
cargo build --release --bin quick-presenter
scripts/stage_macos_app_bundle.sh /tmp/quick-presenter-macos
```

To create an unsigned disk image from the staged app bundle:

```sh
scripts/create_macos_dmg.sh \
  "/tmp/quick-presenter-macos/Quick Presenter.app" \
  "/tmp/quick-presenter-macos/QuickPresenter-<version>.dmg"
```

To sign the staged app bundle before creating a distribution disk image:

```sh
export MACOS_SIGNING_IDENTITY="Developer ID Application: Example Name (TEAMID)"

scripts/sign_macos_app.sh \
  "/tmp/quick-presenter-macos/Quick Presenter.app"

scripts/create_macos_dmg.sh \
  "/tmp/quick-presenter-macos/Quick Presenter.app" \
  "/tmp/quick-presenter-macos/QuickPresenter-<version>.dmg"
```

`scripts/sign_macos_app.sh` does not contain certificate names, passwords, or
notarization credentials. It signs the Mach-O files inside the app bundle first,
including the bundled PDFium dynamic library, then signs `Quick Presenter.app`
with hardened runtime and a timestamp. Signature verification must run in a
normal macOS user session that can access the relevant Keychain and trust
settings; restricted sandboxes can report false negatives even when Gatekeeper
accepts the notarized app.

The signing identity can also be passed as the second argument:

```sh
scripts/sign_macos_app.sh \
  "/tmp/quick-presenter-macos/Quick Presenter.app" \
  "Developer ID Application: Example Name (TEAMID)"
```

To create a signed, notarized, and stapled distribution disk image, first store
notarytool credentials in the local Keychain. The profile name is local machine
state and should not be committed to the repository:

```sh
xcrun notarytool store-credentials quick-presenter-notary \
  --key /path/to/AuthKey_XXXXXXXXXX.p8 \
  --key-id YOUR_KEY_ID \
  --issuer YOUR_ISSUER_ID
```

Then create, sign, notarize, staple, and validate the DMG:

```sh
export MACOS_SIGNING_IDENTITY="Developer ID Application: Example Name (TEAMID)"
export NOTARYTOOL_KEYCHAIN_PROFILE="quick-presenter-notary"
export QUICK_PRESENTER_DMG_SMOKE_PDF="$PWD/tests/fixtures/marp-speaker-notes.pdf"

scripts/notarize_macos_dmg.sh \
  "/tmp/quick-presenter-macos/Quick Presenter.app" \
  "/tmp/quick-presenter-macos/QuickPresenter-<version>.dmg"
```

`scripts/notarize_macos_dmg.sh` expects the app bundle to already be signed by
`scripts/sign_macos_app.sh`. It creates the DMG, signs the DMG, submits it with
`xcrun notarytool submit --wait`, staples the notarization ticket, validates the
ticket, and runs a Gatekeeper assessment on the DMG.

After stapling, the script also mounts the final DMG and validates the packaged
payload. It verifies `Quick Presenter.app`, `Contents/MacOS/quick-presenter`, and the bundled
PDFium dylib with `codesign --verify`, runs a Gatekeeper execution assessment on
the mounted app, and runs a smoke-open check when `QUICK_PRESENTER_DMG_SMOKE_PDF`
points to a local PDF. This catches cases where the DMG itself is notarized but
the app payload was damaged during staging.

The DMG staging path uses `ditto` rather than `cp -R` so signed app bundle
metadata, extended attributes, and resource forks are preserved while the
distribution image is assembled.

The `build-binaries.yml` workflow stages the `.app` inside the macOS artifact
and validates:

- `Contents/Info.plist` is valid,
- bundle display name, executable, icon file, package type, and principal class
  are set,
- `Contents/MacOS/quick-presenter` is executable,
- `QuickPresenter.icns` exists in `Contents/Resources/`,
- bundled PDFium, license files, and source offer are present.
- the bundled app executable can run `--smoke-open-pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`.

The same workflow also creates an unsigned `QuickPresenter-<version>.dmg`, mounts it,
and validates:

- the disk image contains `Quick Presenter.app`,
- the disk image contains an `Applications` symlink,
- the app bundle inside the mounted disk image still contains the icon, PDFium,
  license files, and source offer,
- the mounted app executable can run `--smoke-open-pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`.

Manual verification:

- Stage the app bundle locally.
- Create the unsigned disk image locally.
- Mount `/tmp/quick-presenter-macos/QuickPresenter-<version>.dmg`.
- Confirm the mounted volume contains `Quick Presenter.app` and an
  `Applications` symlink.
- Run `open "/tmp/quick-presenter-macos/Quick Presenter.app"`.
- Open `tests/fixtures/marp-speaker-notes.pdf` without `PDFIUM_DYNAMIC_LIB_PATH`.
- Press Cmd+Tab and confirm the Quick Presenter icon is shown.

The current CI disk image is unsigned unless release signing credentials are
configured. Developer ID notarization and stapling are tracked as release-only
steps that come after app bundle signing. Universal binary packaging is also
tracked separately from the first disk image packaging flow.

### Windows

Raw Windows `quick-presenter.exe` builds embed `assets/icons/windows/quick-presenter.ico`
as an executable resource from `build.rs`. This keeps the development binary
name as `quick-presenter.exe` while allowing Windows shell surfaces to discover the Quick
Presenter icon.

Release builds of `quick-presenter.exe` use the Windows GUI subsystem to avoid showing an
extra console window when launched from the MSI or MSIX. Debug builds keep the
console subsystem, so local development still shows command-line output for
`--help`, `--smoke-open-pdf`, and startup errors.

The `build-binaries.yml` workflow runs on `windows-latest`, builds `quick-presenter.exe`, and
extracts the associated executable icon into `quick-presenter-associated-icon.png` inside the
uploaded Windows artifact. That preview is a CI smoke test that the executable
has an associated icon resource.

The workflow also runs `quick-presenter.exe --smoke-open-pdf` from the staged artifact with
`PDFIUM_DYNAMIC_LIB_PATH` unset.

Windows MSI packages are built with WiX Toolset from the staged release binary
and bundled PDFium files. The installer keeps the executable name as `quick-presenter.exe`
while presenting the product name as `Quick Presenter` in installer metadata
and the Start Menu shortcut.

The installed layout is:

```text
Quick Presenter/
  quick-presenter.exe
  pdfium/
  licenses/
    QuickPresenter-LICENSE.txt
    QuickPresenter-SOURCE-OFFER.txt
    PDFium-LICENSE.txt
```

The bundled PDFium directory is installed next to `quick-presenter.exe`, which is covered by
the runtime lookup order documented above.

To build the MSI locally on Windows:

```powershell
python scripts/fetch_pdfium.py --clean
cargo build --release --bin quick-presenter
dotnet tool install --global wix
scripts/build_windows_msi.ps1
```

The MSI is written to:

```text
artifacts/quick-presenter-windows-x64/QuickPresenter-<version>.msi
```

The `build-binaries.yml` workflow builds this MSI on `windows-latest` and
validates:

- the MSI artifact exists,
- administrative extraction with `msiexec /a` succeeds,
- the extracted layout contains `quick-presenter.exe`, bundled PDFium, license files, and the
  source offer,
- the extracted `quick-presenter.exe` can run `--smoke-open-pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`.

CI cannot fully verify final Windows shell behavior because Explorer, Alt+Tab,
and taskbar rendering depend on an interactive Windows session and icon cache
state. Use a real Windows machine for final acceptance.

Manual verification:

- Download the `quick-presenter-windows-x64` artifact or build locally with
  `cargo build --release --bin quick-presenter`.
- Inspect `quick-presenter.exe` in Explorer.
- Run `scripts/build_windows_msi.ps1` on Windows.
- Install `artifacts/quick-presenter-windows-x64/QuickPresenter-<version>.msi`.
- Confirm the Start Menu contains `Quick Presenter`.
- Launch Quick Presenter from the Start Menu.
- Open `tests/fixtures/marp-speaker-notes.pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`.
- Confirm Windows Apps/Installed apps can uninstall Quick Presenter cleanly.
- Run the app and confirm the icon appears in Alt+Tab and the taskbar.
- If Explorer shows a stale generic icon, copy the artifact to a fresh path and
  retry before treating it as a failure.

The current pull request MSI is unsigned. Trusted non-PR builds can
optionally sign Windows artifacts when signing credentials are configured, as
described below. Microsoft Store distribution, PDF file associations, and
auto-update infrastructure are tracked separately from the first MSI packaging
flow.

#### Optional Authenticode signing

Windows Authenticode signing is optional in CI. Pull request and contributor
builds remain unsigned so they can run without signing credentials. On trusted
repository events, such as `workflow_dispatch` or pushes to `main`, the
`build-binaries.yml` workflow signs Windows artifacts when these secrets are
available:

- `WINDOWS_SIGNING_CERT_PFX_BASE64`: base64-encoded `.pfx` code signing
  certificate.
- `WINDOWS_SIGNING_CERT_PASSWORD`: password for the `.pfx`.

These repository variables are optional:

- `WINDOWS_SIGNING_TIMESTAMP_URL`: RFC 3161 timestamp server URL. If omitted,
  the workflow uses `http://timestamp.digicert.com`.
- `WINDOWS_SIGNING_PUBLISHER`: expected certificate subject and MSIX manifest
  publisher. If omitted, the workflow uses `CN=Quick Presenter`.

When signing is enabled, CI signs `target/release/quick-presenter.exe` before staging and
packaging so both MSI and MSIX payloads contain the signed executable. After the
MSI and MSIX are built and layout-validated, CI signs the
`QuickPresenter-<version>.msi` and `QuickPresenter-<version>.msix`
containers and verifies signatures for:

- `quick-presenter.exe`
- `QuickPresenter-<version>.msi`
- `QuickPresenter-<version>.msix`

The decoded PFX is written only to the Windows runner's temporary directory and
removed before artifact upload. For self-signed test certificates, the workflow
allows `signtool verify /pa` to fail with an untrusted chain, then validates
that Authenticode metadata exists and that the signer thumbprint matches the
configured PFX. This fallback is only for self-signed test certificates; normal
CA-issued certificates must pass `signtool verify /pa`. The workflow skips
signing, rather than failing, when the signing secrets are absent.

To sign locally on Windows after building all Windows artifacts:

```powershell
scripts/sign_windows_artifacts.ps1 `
  -ArtifactDir "artifacts/quick-presenter-windows-x64" `
  -PfxPath "C:\path\to\certificate.pfx" `
  -PfxPassword "<pfx-password>" `
  -TimestampUrl "http://timestamp.digicert.com" `
  -ExpectedPublisher "CN=Quick Presenter"
```

To verify signatures locally:

```powershell
signtool.exe verify /pa /v "artifacts/quick-presenter-windows-x64/quick-presenter.exe"
signtool.exe verify /pa /v "artifacts/quick-presenter-windows-x64/QuickPresenter-<version>.msi"
signtool.exe verify /pa /v "artifacts/quick-presenter-windows-x64/QuickPresenter-<version>.msix"
```

MSIX signing requires the manifest `Identity Publisher` to match the signing
certificate subject. Pass the same publisher value to
`scripts/build_windows_msix.ps1 -Publisher` when creating a signed MSIX locally.
SmartScreen reputation is not a CI gate; a technically valid signature may still
show warnings until the publisher or app has sufficient reputation.

#### Unsigned MSIX validation

Quick Presenter also has an unsigned MSIX packaging path for CI layout
validation. This package is not a user-installable distribution artifact.
Windows requires installable MSIX packages to be signed by a trusted
certificate, and production signing is tracked separately in #109.

The MSIX payload intentionally mirrors the MSI payload where practical:

```text
QuickPresenter-<version>.msix
  AppxManifest.xml
  quick-presenter.exe
  pdfium/
  licenses/
    QuickPresenter-LICENSE.txt
    QuickPresenter-SOURCE-OFFER.txt
    PDFium-LICENSE.txt
  Assets/
    Square44x44Logo.png
    Square150x150Logo.png
```

To build the unsigned MSIX locally on Windows:

```powershell
python scripts/fetch_pdfium.py --clean
cargo build --release --bin quick-presenter
scripts/build_windows_msix.ps1
```

The MSIX is written to:

```text
artifacts/quick-presenter-windows-x64/QuickPresenter-<version>.msix
```

The `build-binaries.yml` workflow builds this unsigned MSIX on
`windows-latest` and validates:

- the MSIX artifact exists,
- `MakeAppx.exe unpack` succeeds,
- the unpacked layout contains `AppxManifest.xml`, `quick-presenter.exe`, bundled PDFium,
  icon assets, license files, and the source offer,
- the manifest contains the expected desktop identity, `quick-presenter.exe` application
  entry, and `runFullTrust` capability,
- the unpacked `quick-presenter.exe` can run `--smoke-open-pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`.

The CI workflow intentionally does not run `Add-AppxPackage` for the unsigned
MSIX. Signed, installable MSIX packages and signature verification belong to the
Windows signing work in #109.

### Linux

Linux packaging starts with an Ubuntu-oriented Debian package (`.deb`). Ubuntu
is the first Linux package target because the existing Linux CI job runs on
`ubuntu-latest`, and Debian package artifacts can be built, inspected,
extracted, installed, and removed with standard `dpkg` and `apt` tooling in CI.
RPM, AppImage, and Flatpak remain future package format candidates after the
Debian package layout and bundled PDFium behavior are stable.

The raw Linux binary artifact remains available. The same artifact directory
also includes the Debian package.

The installed Debian package layout is:

```text
/usr/bin/quick-presenter -> ../lib/quick-presenter/quick-presenter
/usr/lib/quick-presenter/
  quick-presenter
  pdfium/
  licenses/
    QuickPresenter-LICENSE.txt
    QuickPresenter-SOURCE-OFFER.txt
    PDFium-LICENSE.txt
/usr/share/applications/quick-presenter.desktop
/usr/share/icons/hicolor/.../apps/quick-presenter.png
/usr/share/doc/quick-presenter/
  QuickPresenter-LICENSE.txt
  QuickPresenter-SOURCE-OFFER.txt
  PDFium-LICENSE.txt
```

The bundled PDFium directory is installed next to the real application
executable at `/usr/lib/quick-presenter/quick-presenter`, which is covered by the runtime
lookup order documented above. The `/usr/bin/quick-presenter` entry is a symlink
so shell launches and the desktop entry use the same installed command path.

Linux binary artifacts also stage desktop metadata with
`scripts/stage_linux_desktop_assets.sh`. The staged layout includes a desktop
entry and PNG icons from `assets/icons/png/` installed under the hicolor icon
theme with the icon name `quick-presenter`.

The desktop entry is:

- `share/applications/quick-presenter.desktop`

It uses:

```ini
Name=Quick Presenter
Exec=/usr/bin/quick-presenter %f
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

To build the Debian package locally on Ubuntu:

```sh
python3 scripts/fetch_pdfium.py --clean
cargo build --release --bin quick-presenter
scripts/build_linux_deb.sh
```

The Debian package is written to:

```text
artifacts/quick-presenter-ubuntu-x64/quick-presenter_<version>_amd64.deb
```

The `build-binaries.yml` workflow runs the staging script for the Linux artifact
and builds the Debian package. It validates:

- the desktop entry exists and contains the expected `Name`, `Exec`, and `Icon`,
- the desktop entry passes `desktop-file-validate`,
- each expected hicolor icon file exists and is non-empty,
- `gtk-update-icon-cache` can process the staged hicolor tree,
- the Debian package exists and exposes expected package metadata,
- the extracted Debian package contains `quick-presenter`, the bundled `pdfium/` directory,
  license files, the source offer, the desktop entry, and hicolor icons,
- the extracted `quick-presenter` binary can run `--smoke-open-pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`,
- the extracted `/usr/bin/quick-presenter` symlink can run `--smoke-open-pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`,
- the Debian package can be installed with `apt`, smoke-tested, and removed.
- the staged `quick-presenter` binary can run `--smoke-open-pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`.

CI cannot fully verify launcher, app switcher, dock, or taskbar rendering because
those require an interactive Linux desktop session. Use a real desktop
environment for final acceptance.

Manual verification:

- Download the `quick-presenter-ubuntu-x64` artifact or build locally.
- Install the Debian package on an Ubuntu desktop:

```sh
sudo apt-get install ./quick-presenter_<version>_amd64.deb
```

- Confirm the installed command can open the smoke-test PDF without a manual
  PDFium path:

```sh
env -u PDFIUM_DYNAMIC_LIB_PATH quick-presenter --smoke-open-pdf tests/fixtures/marp-speaker-notes.pdf
```

- Confirm the package can be removed cleanly:

```sh
sudo apt-get remove quick-presenter
```

- Install the package again for desktop-session checks.
- Confirm `Quick Presenter` appears in the launcher.
- Launch the app through the desktop entry.
- Open `tests/fixtures/marp-speaker-notes.pdf` without
  `PDFIUM_DYNAMIC_LIB_PATH`.
- Confirm the icon appears in launcher search, app switcher, and the desktop
  environment's dock, taskbar, or panel.

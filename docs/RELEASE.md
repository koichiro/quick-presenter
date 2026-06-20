# Release Usage

This page explains how to use Quick Presenter release artifacts downloaded from
GitHub Actions or a GitHub release. For build, signing, and package validation
details, see [PACKAGING.md](PACKAGING.md).

Quick Presenter only opens PDF slide decks. It is not a slide editor and does
not import Keynote, PowerPoint, Google Slides, Marp, or Beamer source files
directly. Export the final deck to PDF before presenting.

## Supported Release Artifacts

Use the artifact that matches your operating system:

| Platform | Artifact | What to use |
| --- | --- | --- |
| macOS | `quick-presenter-macos` | `Quick Presenter.dmg` or `Quick Presenter.app` |
| Windows x64 | `quick-presenter-windows-x64` | `Quick Presenter.msi` |
| Ubuntu x64 | `quick-presenter-ubuntu-x64` | `quick-presenter_<version>_amd64.deb` |

The uploaded artifact directories may also include raw `qp` or `qp.exe`
development binaries. Prefer the packaged artifact for normal use because
it keeps the executable, bundled PDFium files, desktop metadata, and license
files in the expected layout.

The Windows artifact may include `Quick Presenter.msix` for layout validation.
Unsigned MSIX packages are not a user-installable distribution artifact.
Use the MSI unless a release explicitly identifies a signed MSIX as supported.

## macOS

Download `quick-presenter-macos` and open `Quick Presenter.dmg`.

1. Drag `Quick Presenter.app` to `Applications`, or run the app from the mounted
   disk image for a quick check.
2. Launch `Quick Presenter`.
3. Choose `Open` or `File > Open PDF...`.
4. Select a PDF slide deck.
5. Use the presenter window for controls and the slide window for audience
   output.

Current macOS disk images may be unsigned unless release signing and
notarization were configured for that build. macOS Gatekeeper can warn about
unsigned or unnotarized builds.

## Windows

Download `quick-presenter-windows-x64` and run `Quick Presenter.msi`.

1. Install the MSI.
2. Launch `Quick Presenter` from the Start Menu.
3. Choose `Open` or `File > Open PDF...`.
4. Select a PDF slide deck.
5. Use the presenter window for controls and the slide window for audience
   output.

Current MSI packages may be unsigned unless release signing was configured
for that build. Windows SmartScreen can warn about unsigned or low-reputation
builds. PDF file associations and auto-update are not part of the current
installer flow.

## Ubuntu Linux

Download `quick-presenter-ubuntu-x64` and install the Debian package:

```sh
sudo apt-get install ./quick-presenter_<version>_amd64.deb
```

Then launch Quick Presenter from the desktop launcher, or run:

```sh
qp
```

To open a PDF directly from a shell:

```sh
qp --pdf path/to/slides.pdf
```

The current Linux package targets Ubuntu x64. Other Debian-based distributions
may work, but they are not the first supported release target. RPM, AppImage,
and Flatpak packages are not part of the current artifact set.

To remove the package:

```sh
sudo apt-get remove quick-presenter
```

## Opening PDFs

Quick Presenter can open a PDF from the app UI or at startup:

```sh
qp --pdf path/to/slides.pdf
```

Packaged builds include a presenter window and a separate audience-facing slide
window. Keyboard controls are documented in [KEYBOARD.md](KEYBOARD.md).

If opening a PDF fails, try a known-good PDF first. Quick Presenter reports
short presenter-facing errors and does not attempt to repair invalid PDFs.

## Bundled PDFium

Packaged releases are expected to bundle PDFium so the app can open PDFs
without a first-launch download and without network access. Packaged layouts keep a
`pdfium/` directory next to the application executable or inside the macOS app
bundle resources.

`PDFIUM_DYNAMIC_LIB_PATH` is still available as a troubleshooting and
development override. You should not need it for correctly packaged artifacts.
Set it only when running a raw binary without the bundled `pdfium/`
directory, or when diagnosing a broken package layout.

## Known Limitations

- Speaker notes are extracted from supported PDF text annotations. The first
  supported workflow is Marp PDF speaker notes; arbitrary PDF comments or
  editor-specific note formats may not appear. See [NOTES.md](NOTES.md).
- Quick Presenter opens prepared PDF decks only. Editing slides, creating
  slides, and managing document libraries are outside the product scope.
- Linux desktop integration can vary by desktop environment. Package validation
  checks the installed layout, desktop metadata, and smoke startup, but cannot
  cover every launcher, dock, and app switcher behavior.

## Licenses

Quick Presenter is licensed under `GPL-3.0-or-later`. See the repository
[LICENSE](../LICENSE).

Packaged builds that bundle PDFium must include PDFium and third-party
component license files. In installed packages, look for the `licenses/`
directory next to the application files. The repository copy is in
[`pdfium/LICENSE`](../pdfium/LICENSE), after running `scripts/fetch_pdfium.py`.

Binary distributions must also provide the corresponding Quick Presenter source
code for the exact build. Packaged artifacts include
`QuickPresenter-SOURCE-OFFER.txt`, sourced from
[`packaging/SOURCE-OFFER.txt`](../packaging/SOURCE-OFFER.txt).

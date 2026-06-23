# Release Usage

This page explains how to use Quick Presenter release artifacts downloaded from
GitHub Actions or a GitHub release. For build, signing, and package validation
details, see [PACKAGING.md](PACKAGING.md).

Quick Presenter only opens PDF slide decks. It is not a slide editor and does
not import Keynote, PowerPoint, Google Slides, Marp, or Beamer source files
directly. Export the final deck to PDF before presenting.

## Release Validation

Routine pull requests are validated by the lightweight `CI` workflow. The full
`Build Binaries` workflow is release/package validation and does not run for
every source-only pull request.

Before publishing a release:

1. Confirm `CI` passes on the release branch.
2. Confirm the scheduled `Dependency Audit` workflow has passed recently, then
   run `scripts/audit_deps.sh` locally and resolve dependency advisories, or
   document any explicitly accepted advisory in `.cargo/audit.toml`.
3. Run `Build Binaries` manually with `workflow_dispatch`, or push a release
   tag matching `v*`.
4. Record the successful `Build Binaries` workflow run URL in the release
   checklist.
5. Confirm the Linux, macOS, and Windows artifacts are uploaded and their package
   smoke tests pass.
6. Review the uploaded `quick-presenter-ubuntu-x64-gui-smoke` report and the
   macOS/Windows GUI smoke skip-reason reports from `Build Binaries`.
7. Run the [GUI release smoke checklist](GUI_SMOKE_CHECKLIST.md) on the final
   macOS, Windows, and Ubuntu Linux artifacts before publishing.
8. Attach the completed manual GUI smoke reports to the GitHub release, or
   document any platform-specific waiver in the release notes before publishing.

`Build Binaries` still runs automatically for pull requests that change
packaging-sensitive files, such as packaging scripts, installer metadata,
icons, or Cargo dependency metadata.

The dependency advisory audit uses `cargo audit`. Install it with
`cargo install cargo-audit --locked` before running `scripts/audit_deps.sh`
locally. The `Dependency Audit` workflow runs the same script on a weekly
schedule and can also be started manually. `Build Binaries` keeps its own
dependency advisory audit so release package validation still fails on
unaccepted vulnerability advisories even if the scheduled workflow was skipped
or stale.

Warning-only advisories, such as unmaintained or yanked transitive crates, must
be reviewed before release. Any explicitly accepted advisory must be listed in
`.cargo/audit.toml` with a reason, impact summary, tracking issue, and review
date so release maintainers can tell whether the risk is still accepted.
Dependabot checks Cargo and GitHub Actions dependencies weekly.

GitHub Actions should use predictable references. First-party actions may use a
major version tag when Dependabot covers update review. Third-party actions
should be added only when the release workflow needs them and should use at
least a major or minor version tag when the action publishes versioned tags.
Toolchain selector actions may use explicit toolchain channel references such
as `stable`. Any third-party action that affects release signing, notarization,
package upload, or artifact trust should be considered for commit SHA pinning
before adoption. Shell-installed tools should use an explicit version or a
locked installation mode when the tool supports it.

SBOM generation is not part of the v1.0.0 release baseline. Release artifacts
bundle Rust dependencies, platform packages, and PDFium native binaries, so the
project will choose an artifact-level SBOM policy separately before adopting a
tool or output format.

The package smoke tests are intentionally non-interactive and do not create
Slint windows. `Build Binaries` also runs
`quick-presenter --gui-smoke <PDF> --gui-smoke-report <PATH>` from the installed
Ubuntu package under Xvfb and uploads the report. macOS and Windows runners
upload explicit GUI smoke skip-reason reports because hosted CI does not provide
the normal desktop sessions needed for reliable platform GUI release gates. The
GUI checklist remains the release gate for OS-owned title-bar, menu,
keyboard-focus, shell integration, and readability behavior.

Manual GUI smoke reports are release artifacts. For v1.0.0 and later releases,
attach one report per supported platform to the GitHub release using the
filenames and required metadata in [GUI_SMOKE_CHECKLIST.md](GUI_SMOKE_CHECKLIST.md).
Each report must identify the exact tested package file and package SHA-256 so
the result can be matched to the released DMG, MSI, or Debian package.

## Supported Release Artifacts

Use the artifact that matches your operating system:

| Platform | Artifact | What to use |
| --- | --- | --- |
| macOS | `quick-presenter-macos` | `QuickPresenter-<version>.dmg` or `Quick Presenter.app` |
| Windows x64 | `quick-presenter-windows-x64` | `QuickPresenter-<version>.msi` |
| Ubuntu x64 | `quick-presenter-ubuntu-x64` | `quick-presenter_<version>_amd64.deb` |

The uploaded artifact directories may also include raw `quick-presenter` or `quick-presenter.exe`
development binaries. Prefer the packaged artifact for normal use because
it keeps the executable, bundled PDFium files, desktop metadata, and license
files in the expected layout.

The Windows artifact may include `QuickPresenter-<version>.msix` for layout validation.
Unsigned MSIX packages are not a user-installable distribution artifact.
Use the MSI unless a release explicitly identifies a signed MSIX as supported.

## macOS

Download `quick-presenter-macos` and open `QuickPresenter-<version>.dmg`.

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

Download `quick-presenter-windows-x64` and run `QuickPresenter-<version>.msi`.

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
quick-presenter
```

To open a PDF directly from a shell:

```sh
quick-presenter --pdf path/to/slides.pdf
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
quick-presenter --pdf path/to/slides.pdf
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
Packaged builds ignore it unless `QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE=1` is
also set. Use that combination only when diagnosing a broken package layout or
running a raw binary without the bundled `pdfium/` directory.

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

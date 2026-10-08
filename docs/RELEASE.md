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

1. Set the release version in `Cargo.toml`, refresh `Cargo.lock`, and confirm
   both files contain the intended version. The About dialog and package
   builders read this Cargo package version.
2. Confirm `CI` passes on the release branch.
3. Confirm the scheduled `Dependency Audit` workflow has passed recently, then
   run `scripts/audit_deps.sh` locally and resolve dependency advisories, or
   document any explicitly accepted advisory in `.cargo/audit.toml`.
4. Confirm the monthly and pre-release PDFium review is recorded, the pinned
   build satisfies the adoption or documented-deferral policy, and no relevant
   native security update is awaiting expedited validation. See
   [PDF Rendering Security and Isolation Policy](PDF_RENDERING_SECURITY.md).
5. Run `Build Binaries` manually with `workflow_dispatch`, or push a release
   tag matching `v*`.
6. Record the successful `Build Binaries` workflow run URL in the release
   checklist.
7. Confirm the Linux and macOS packages use the intended release version and
   their package smoke tests pass. For Linux, verify the Debian file name and
   the `Version` reported by `dpkg-deb --field <package> Version`. Confirm the
   existing Microsoft Store listing remains publicly reachable; the v1.0.0
   Store update is verified after the GitHub release as described below.
8. Review the uploaded `quick-presenter-ubuntu-x64-gui-smoke` report and the
   macOS/Windows GUI smoke skip-reason reports from `Build Binaries`.
9. Run the [GUI release smoke checklist](GUI_SMOKE_CHECKLIST.md) on the final
   macOS and Ubuntu Linux artifacts and the existing public Microsoft Store
   build before publishing. Rerun the Windows checks on v1.0.0 after its Store
   update is certified.
10. Complete the platform trust checks in the v1.0.0 policy below.
11. Verify SBOM sidecars against the final DMG and Debian package, review their
    source/target/PDFium metadata, and attach the sidecars to the GitHub release.
    Record package hashes and follow the [SBOM release gate](SBOM.md#release-gate).
    Generate and publish the Windows Store submission SBOM with its post-release
    Store update; record submission and certified package identities separately.
12. Attach the completed manual GUI smoke reports to the GitHub release, or
    document any platform-specific waiver in the release notes before publishing.

### Mac App Store submissions

For each Mac App Store submission, also complete the separate
[Mac App Store release checklist](MAC_APP_STORE.md#release-checklist). It covers
the exact source revision, Store build number, signing profile, sandboxed PDF
reopening, upload processing, review submission, and Store-installed validation.
The Store package does not replace the Developer ID DMG or its notarization gate.

Record upload, review submission, approval, and release as separate events. A
Transporter delivery receipt alone does not mean the app has entered review or
is available to customers. Preserve the corresponding source and SBOM required
by [PACKAGING.md](PACKAGING.md#source-code-for-binary-releases) and
[SBOM.md](SBOM.md#release-gate), including any Store-specific source changes.

### v1.0.0 distribution and trust policy

The supported v1.0.0 distribution channels are intentionally narrow:

| Platform/channel | User package | Trust and identity | v1.0.0 status | Review lead time and fallback |
| --- | --- | --- | --- | --- |
| macOS direct | Developer ID DMG | Developer ID signed, Apple-notarized, and stapled | Required release gate | Notarization must finish before publishing; there is no unsigned fallback. |
| Mac App Store | App Store package | App Sandbox, App Store entitlements, and App Store review | Best effort; not a blocker | Review timing is external; use the signed/notarized direct DMG if the Store version is not ready. |
| Windows | Microsoft Store MSIX | Partner Center identity, certification, and Store-managed signing | Store availability is required; the v1.0.0 package update is post-release | Verify the existing public listing before release, then submit v1.0.0 and verify the updated Store installation. Never use a direct installer as a fallback. |
| Ubuntu Linux direct | Debian package | Package version, SHA-256, bundled licenses, and real-machine smoke test | Required release gate | There is no alternate v1.0.0 package channel. |

Validate the final macOS DMG with `codesign --verify --deep --strict`,
`xcrun stapler validate`, and `spctl --assess`. An unsigned or unnotarized CI
DMG is a validation artifact and must not be published as the release.

For Windows, the already-public Store version establishes that the distribution
channel is available before the GitHub v1.0.0 release. After that release,
submit the v1.0.0 Store package, wait for certification, and confirm that the
Store-installed app reports v1.0.0 and launches without an untrusted-publisher
or SmartScreen warning. This post-release verification does not block the
GitHub release. Direct MSI, MSIX, and raw executable artifacts are CI
validation artifacts and must not be published as supported downloads. A
self-signed certificate only exercises the signing pipeline and is never a
production trust credential.

For Ubuntu Linux, record the Debian package SHA-256, confirm the filename and
package metadata version, and install and smoke-test that exact package on a
supported real machine before publishing.

Homebrew Cask, WinGet, Flatpak/Flathub, and other package-manager channels are
outside the v1.0.0 scope.

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

Release packages require CycloneDX 1.5 JSON SBOM sidecars under the
[Release SBOM Policy](SBOM.md). `Build Binaries` generates and verifies sidecars
for its package artifacts. Regenerate and verify them after production signing,
notarization, stapling, or repackaging, then attach them to the GitHub release.
The policy documents native inventory limitations and Store submission identity.

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
Each report must identify the exact tested distribution. Record the final file
and SHA-256 for the DMG and Debian package. For Windows, record the Microsoft
Store ID, Store package identity, and installed version instead of a local
package hash.

## Supported Release Artifacts

Use the artifact that matches your operating system:

| Platform | Artifact | What to use |
| --- | --- | --- |
| macOS | `quick-presenter-macos` | `QuickPresenter-<version>.dmg` or `Quick Presenter.app` |
| Windows x64 | Microsoft Store ID `9N913S9NJ6D1` | Install from Microsoft Store |
| Ubuntu x64 | `quick-presenter-ubuntu-x64` | `quick-presenter_<version>_amd64.deb` |

The uploaded artifact directories may also include raw `quick-presenter` or `quick-presenter.exe`
development binaries. Prefer the packaged artifact for normal use because
it keeps the executable, bundled PDFium files, desktop metadata, and license
files in the expected layout.

The Windows CI artifact may include MSI, MSIX, and raw executable files for
layout, packaging, and signing validation. They are not supported v1.0.0
downloads. The Store-identity MSIX is submitted to Partner Center and becomes
the supported package only after Microsoft Store certification and signing.

## macOS

Download `quick-presenter-macos` and open `QuickPresenter-<version>.dmg`.

1. Drag `Quick Presenter.app` to `Applications`, or run the app from the mounted
   disk image for a quick check.
2. Launch `Quick Presenter`.
3. Choose `Open` or `File > Open PDF...`.
4. Select a PDF slide deck.
5. Use the presenter window for controls and the slide window for audience
   output.

The v1.0.0 release disk image must be Developer ID signed, Apple-notarized, and
stapled. Gatekeeper must accept the final DMG without an unidentified-developer
warning. Unsigned CI disk images are packaging validation artifacts only.

## Windows

Install Quick Presenter from Microsoft Store ID `9N913S9NJ6D1`.

1. Complete the Microsoft Store installation.
2. Launch `Quick Presenter` from the Start Menu.
3. Choose `Open` or `File > Open PDF...`.
4. Select a PDF slide deck.
5. Use the presenter window for controls and the slide window for audience
   output.

The Microsoft Store is the only supported Windows distribution channel for
v1.0.0. Do not publish the workflow-generated MSI, direct MSIX, or raw
executable as a release download. Microsoft Store certification and signing
provide the production trust boundary; a self-signed CI artifact does not.

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

The Linux desktop entry advertises Quick Presenter as an available handler for
PDF files, so supported desktop environments can show it in "Open With" flows.
Installing the package does not make Quick Presenter the default PDF viewer.

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
Password-protected and encrypted PDFs are not supported in v1.0.0. Export an
unprotected PDF before opening it in Quick Presenter; the app does not prompt
for or store PDF passwords.

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
  checks the installed layout, desktop metadata, PDF handler registration, and
  smoke startup, but cannot cover every launcher, dock, app switcher, or
  "Open With" behavior.

## Licenses

Quick Presenter is licensed under `GPL-3.0-only`. See the repository
[LICENSE](../LICENSE).

Packaged builds that bundle PDFium must include PDFium and third-party
component license files. In installed packages, look for the `licenses/`
directory next to the application files. The repository copy is in
[`pdfium/LICENSE`](../pdfium/LICENSE), after running `scripts/fetch_pdfium.py`.

Binary distributions must also provide the corresponding Quick Presenter source
code for the exact build. Packaged artifacts include
`QuickPresenter-SOURCE-OFFER.txt`, sourced from
[`packaging/SOURCE-OFFER.txt`](../packaging/SOURCE-OFFER.txt).

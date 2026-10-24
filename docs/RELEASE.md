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

The v2.0.0 CLI and AI-assisted presentation release uses Cargo package version
`2.0.0` and Control Protocol v1. GitHub publication and Microsoft Store
submission, certification, and installed-version qualification are separate
events. Record the exact source and package identity for each channel.

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
   existing Microsoft Store listing remains publicly reachable; the v2.0.0
   Store update is verified after the GitHub release as described below.
8. Review the uploaded `quick-presenter-ubuntu-x64-gui-smoke` report and the
   macOS/Windows GUI smoke skip-reason reports from `Build Binaries`.
9. Run the [GUI release smoke checklist](GUI_SMOKE_CHECKLIST.md) on the final
   macOS and Ubuntu Linux artifacts and the existing public Microsoft Store
   build before publishing. Rerun the Windows checks on v2.0.0 after its Store
   update is certified.
10. Complete the platform trust checks in the v2.0.0 policy below.
11. Verify SBOM sidecars against the final DMG and Debian package, review their
    source/target/PDFium metadata, and attach the sidecars to the GitHub release.
    Record package hashes and follow the [SBOM release gate](SBOM.md#release-gate).
    Generate and publish the Windows Store submission SBOM with its post-release
    Store update; record submission and certified package identities separately.
12. Attach the completed manual GUI smoke reports to the GitHub release, or
    document any platform-specific waiver in the release notes before publishing.

### v2.5.0 validation evidence

Before publishing v2.5.0, review the [security and macOS artifact assessment](validation/ISSUE508_MACOS_RELEASE_REVIEW_20261022.md)
and [internal TestFlight 25009 qualification](validation/ISSUE433_STORE_CLI_TESTFLIGHT_25009.md).
They identify the tested source and artifacts, CVE evidence, retained maintenance
warnings and remaining coverage. Complete the outstanding gates and record final
channel artifacts/dispositions in [#508](https://github.com/koichiro/quick-presenter/issues/508).
For the remaining macOS coverage, [#493](https://github.com/koichiro/quick-presenter/issues/493)
owns fresh-account, upgrade/uninstall/reinstall and stale/unregistered-agent
qualification. [#508](https://github.com/koichiro/quick-presenter/issues/508)
retains supported-macOS/Intel runtime, foreign-team/cross-user rejection, genuine
bookmark revocation and customer Store-installed qualification or the applicable
explicit maintainer release dispositions.
These reports do not approve publication or qualify a changed final package.

The [October 23 security review](validation/SECURITY_REVIEW_20261023.md) supersedes
the earlier current-security assessment. It records the CISA PoC-status change,
additional public CVE mappings and four enabled-path PDFium fixes after the
selected pin. Resolve their update/backport or evidence-based release disposition
in #508 before publication; latest-prebuilt status alone is not approval.

The [October 23 final source/artifact inventory](validation/V250_FINAL_ARTIFACT_INVENTORY_20261023.md)
fixes the selected source, retained PDFium 8111 disposition, notarized direct DMG,
Store submission candidates, Ubuntu DEB, hashes and verified SBOMs. It completes
the inventory work in #508; the remaining channel, environment/authority and
traceability gates still apply before publication. The subsequent macOS
qualification and DMG limitation disposition are linked below.

The [October 23 final macOS runtime report](validation/V250_FINAL_MACOS_RUNTIME_20261023.md) records the installed final direct app and internal TestFlight **25010**, including exact identity, renderer isolation, GUI/CLI regression, installed direct drop acceptance and same-user lifecycle checks. The [final document-delivery follow-up](validation/V250_MACOS_DOCUMENT_DELIVERY_20261023.md) completes Store Finder/drop checks and records the maintainer-approved read-only-DMG icon-drop limitation. Customer Store coverage and the other open #508 gates remain separate.

The [October 24 final Windows runtime report](validation/V250_FINAL_WINDOWS_RUNTIME_20261024.md) records checks of the exact selected final-source MSVC Store submission payload, fresh SBOM/PDFium validation, rendering/CLI/lifecycle checks, and maintainer-confirmed initial display, physical keyboard and Narrator routes. Final customer Store installation/update/alias and other release conditions remain separate in #508; the unsigned submission input is not a Store-distributed installation.

The [October 24 final Ubuntu package report](validation/V250_FINAL_LINUX_RUNTIME_20261024.md) completes current-machine qualification of the selected final DEB: installed payload/SBOM and renderer checks, default Wayland/XWayland GUI regression, CLI/cold-start/lifecycle, physical keyboard and launcher/Open With checks. It records unperformed dual-display coverage, the maintainer's explicit Orca gate exclusion, supplemental forced-FemtoVG smoke failures and a new Windows-only yanked dependency warning for release-wide review. These bounded results do not close the other #508 conditions or approve publication.

The [October 24 post-merge integration record](validation/V250_RELEASE_INTEGRATION_20261024.md)
rechecks preserved macOS/Linux artifacts against their actual build source and
merged-main normal dependency graphs, tracks the post-merge Windows replacement,
and lists unresolved release gates. It does not relabel old artifacts or approve
unperformed checks, customer Store submission or publication.

### v2.5.0 customer Store qualification timing

On **2026-10-24 JST**, the maintainer explicitly moved customer Mac App Store
and Microsoft Store 2.5.0 installation qualification to the post-review /
post-certification follow-up [#520](https://github.com/koichiro/quick-presenter/issues/520).
These customer checks remain **UNPERFORMED** until the actual distributed
products are tested. The timing condition no longer independently blocks
[#508](https://github.com/koichiro/quick-presenter/issues/508); all other source,
artifact, runtime, environment/authority, traceability and pre-publication
security conditions remain there. Do not automatically close #520 when #508 or
a documentation PR closes.

After certification, bind the actual customer builds to their submitted
source/package/SBOM identities and verify installation/update, distribution
trust, installed workflows and the registered Windows `qp.exe` alias as listed
in #520. Internal TestFlight and unsigned/CI-signed or extracted Windows
submission payloads retain their supporting scope. Upload, review submission,
approval and release require separate authorization and event records; this
timing decision authorizes none of those actions. The Microsoft Store-only
Windows distribution policy and final pre-publication package/security checks
remain in force.

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

### v2.0.0 distribution and trust policy

The supported v2.0.0 distribution channels are intentionally narrow:

| Platform/channel | User package | Trust and identity | v2.0.0 status | Review lead time and fallback |
| --- | --- | --- | --- | --- |
| macOS direct | Developer ID DMG | Developer ID signed, Apple-notarized, and stapled | Required release gate | Notarization must finish before publishing; there is no unsigned fallback. |
| Mac App Store | App Store package | App Sandbox, App Store entitlements, and App Store review | Best effort; not a blocker | Review timing is external; use the signed/notarized direct DMG if the Store version is not ready. |
| Windows | Microsoft Store MSIX | Partner Center identity, certification, and Store-managed signing | Store availability is required; the v2.0.0 package update is post-release | Verify the existing public listing before release, then submit v2.0.0 and verify the updated Store installation. Never use a direct installer as a fallback. |
| Ubuntu Linux direct | Debian package | Package version, SHA-256, bundled licenses, and real-machine smoke test | Required release gate | There is no alternate v2.0.0 package channel. |

Validate the final macOS DMG with `codesign --verify --deep --strict`,
`xcrun stapler validate`, and `spctl --assess`. An unsigned or unnotarized CI
DMG is a validation artifact and must not be published as the release.

For Windows, the already-public Store version establishes that the distribution
channel is available before the GitHub v2.0.0 release. After that release,
submit the v2.0.0 Store package, wait for certification, and confirm that the
Store-installed app reports v2.0.0 and launches without an untrusted-publisher
or SmartScreen warning. This post-release verification does not block the
GitHub release. Direct MSI, MSIX, and raw executable artifacts are CI
validation artifacts and must not be published as supported downloads. A
self-signed certificate only exercises the signing pipeline and is never a
production trust credential.

For v2.0.0, record the submission MSIX filename and SHA-256, source revision,
`2.0.0.0` manifest version, x64 architecture, package-bound SBOM, and
`qp.exe` console execution alias. The expected identity is
`KoichiroOhba.QuickPresenter`, publisher
`CN=A3F64AFD-6298-42F8-BAC9-DB0AB1831F51`, and package family
`KoichiroOhba.QuickPresenter_pma9xfvs5bmmy`. An extracted submission candidate
can validate its payload and behavior, but does not establish Store signing,
installed package identity, alias registration, or upgrade behavior.

After certification, verify the Store-installed GUI and `qp --version --json`
report application 2.0.0 / Control Protocol v1. Repeat cold `qp open` checks
with a different remembered deck, no remembered deck, and a missing remembered
file; check the final `status.document` as well as the Open result. On a
TIMEOUT, query Status without replaying an Open whose outcome is uncertain.
Run installed-package renderer isolation and the manual GUI checklist without
development PDFium overrides. Run `scripts/check_windows_msix_sandbox.ps1`
only on its designated disposable CI runner; its certificate/install flow must
not be used on a maintainer machine.

For Ubuntu Linux, record the Debian package SHA-256, confirm the filename and
package metadata version, and install and smoke-test that exact package on a
supported real machine before publishing.

Homebrew Cask, WinGet, Flatpak/Flathub, and other package-manager channels are
outside the v2.0.0 scope.

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

The [October 24 clipboard-win review](validation/V250_CLIPBOARD_WIN_REVIEW_20261024.md)
updates the yanked Windows transitive dependency from 5.4.1 to 5.4.3, containing
the upstream HTML-fragment bounds fix. The application clipboard adapter uses
Unicode text rather than that HTML getter. Final Windows packages/SBOMs must
be regenerated for the changed dependency; previous ce42e1d artifact hashes
remain historical and must not be reused as the updated candidate identity.

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

Manual GUI smoke reports are release artifacts. For each release,
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
layout, packaging, and signing validation. They are not supported v2.0.0
downloads. The Store-identity MSIX is submitted to Partner Center and becomes
the supported package only after Microsoft Store certification and signing.

## macOS

Download `quick-presenter-macos` and open `QuickPresenter-<version>.dmg`.

1. Drag `Quick Presenter.app` to `Applications` (or a writable user Applications
   folder) before using PDF drag and drop. For a quick check from the mounted
   disk image, use `Open` or `File > Open PDF...`.
2. Launch `Quick Presenter`.
3. Choose `Open` or `File > Open PDF...`.
4. Select a PDF slide deck.
5. Use the presenter window for controls and the slide window for audience
   output.

For v2.5.0, dropping a PDF onto the application-bundle icon on the mounted
read-only DMG is a known unsupported route: Finder can display a prohibited
cursor and refuse the drop. Use the installed app icon, its running Dock icon,
or the presenter/slide windows. These installed-app routes passed final direct
and internal TestFlight 25010 qualification on the test Mac; the DMG refusal
is a documented release limitation, not a passed test or an established OS cause.

The v2.0.0 release disk image must be Developer ID signed, Apple-notarized, and
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
v2.0.0. Do not publish the workflow-generated MSI, direct MSIX, or raw
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
Password-protected and encrypted PDFs are not supported in v2.0.0. Export an
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

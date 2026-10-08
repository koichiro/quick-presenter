# Mac App Store distribution

The Mac App Store package is a separate channel from the Developer ID DMG.
Store signing does not replace direct-download signing or notarization.

Use this checklist alongside [Release Validation](RELEASE.md#release-validation),
the [Store Submission Checklist](PACKAGING.md#store-submission-checklist),
and the [SBOM release gate](SBOM.md#release-gate). Signing/upload preparation is
implemented here; Apple processing, review, and Store-installed runtime support
still require independent evidence.

## Release checklist

1. Select a clean checkout of the intended release source, including these Store
   changes. Record its commit and tag, Cargo version, target triple, Xcode/SDK,
   and pinned PDFium version. Do not build a historical tag without the required
   Store patches, or reuse a binary from another checkout. Publish corresponding
   source for the exact build, including `Cargo.lock` and local patches, before
   distributing it as an official release.
2. Complete the common release checks: CI, dependency/PDFium review, Cargo
   checks, bundled licenses, and GUI smoke reports. Store validation does not
   waive those checks.
3. Check the Apple Distribution and Mac Installer Distribution certificates and
   their private keys in the release Mac's Keychain. Obtain a current Mac App
   Store distribution profile for the app's explicit Bundle ID and signing
   certificate. Keep credentials and profiles outside Git.
4. Choose a new `MACOS_STORE_BUILD_NUMBER`. Confirm the marketing version and
   build number match the App Store Connect version being prepared. Generate
   the package using the commands below and retain its SHA-256 separately from
   Apple's eventual Store package identity.
5. Verify the app's nested signatures, installer signature, embedded profile,
   and minimal entitlements. Test PowerBox selection, recent-file reopening
   after quit/relaunch, history clearing, notes, page navigation, presentation
   windows, fullscreen/blackout, and renderer denial behavior. Follow
   [GUI_SMOKE_CHECKLIST.md](GUI_SMOKE_CHECKLIST.md) and identify the exact signed
   payload tested. If production signing cannot run locally, record that limit
   and use a separate Developer ID-signed copy with the same sandbox permissions
   for development checks. That copy is not the upload package or proof of
   Store-installed behavior.
6. Generate and verify the Store payload inventory described in
   [SBOM.md](SBOM.md#artifact-coverage-and-publication). Preserve the source
   revision, PDFium version, target, inventory limitations, and submission-input
   hashes with the release record.
7. Check App Store Connect metadata: name/version, description/keywords,
   current Mac screenshots, category/age rating/content rights, privacy/support/
   source URLs, data disclosures, review contact and instructions, price,
   availability, and release mode. Confirm DSA and paid agreement/tax/banking
   requirements in the account. Account holders enter identity and financial
   information themselves; never copy those documents into release records.
8. Deliver `Quick-Presenter-Mac-App-Store.pkg` with Transporter and retain its
   delivery result. Wait for App Store Connect processing; investigate any
   processing error before uploading another build. Select the processed build
   and complete the applicable export-compliance questions for that exact
   version. Future features may change these answers or require different
   entitlements; reassess the declaration for each release. PDFium includes
   cryptographic implementations: lack of networking or a password-entry UI
   alone does not establish that a build contains no encryption. Consult
   [Apple's export compliance overview](https://developer.apple.com/help/app-store-connect/manage-app-information/overview-of-export-compliance/)
   and the exact bundled PDFium source when assessing the build.
9. Add the version to review, submit it, and record the resulting review status
   and timestamp. Keep release mode consistent with the intended release plan;
   approval and customer availability are separate from submission.
10. Validate the processed TestFlight/Store-installed app, including external
    PDF selection/reopening and the presentation/renderer gates. Record the
    installed marketing version, build, and Store identity, then record approval
    and release separately. Only claim Store runtime support after this check.

Each submission record should contain source tag/commit, version/build, target,
Xcode/SDK/PDFium versions, package hash, SBOM verification, sandbox/GUI results
and limitations, upload/processing/review/release states, and installed-build
validation. Store raw logs, screenshots, signing files, and private account
details outside public Git history; publish only the necessary redacted summary.

## Package

Run from the selected release checkout on macOS with Xcode command-line tools.
Fetch the pinned PDFium build and build the exact source:

```sh
python3 scripts/fetch_pdfium.py
cargo fmt --check
cargo check
cargo test
cargo build --release --locked
```

When sharing a Cargo target directory between checkouts, pass the resulting
binary's actual path to the package script and confirm it came from this build.

Create an Apple Distribution certificate, a Mac Installer Distribution
certificate, and a Mac App Store provisioning profile for the explicit Bundle
ID `app.quickpresenter.QuickPresenter`. Keep private keys in Keychain, outside
the repository. Generate the upload package with:

```sh
MACOS_STORE_BUILD_NUMBER=10001 scripts/package_macos_app_store.sh \
  /tmp/quick-presenter-store target/release/quick-presenter \
  /path/to/Quick_Presenter_Mac_App_Store.provisionprofile \
  'Apple Distribution: Account Name (TEAMID)' \
  '3rd Party Mac Developer Installer: Account Name (TEAMID)'
```

The script checks the profile's Bundle ID and distribution type, embeds the
profile, removes download quarantine attributes from the generated app only
before signing (Store error `ITMS-91109`), generates the app/team entitlements,
signs nested code, and produces a
signed `productbuild` package. Marketing version comes from Cargo; Store build
numbers must increase independently. Upload the package using Apple's
Transporter. Successful local signing alone does not establish Store support.

The staging step replaces `Quick Presenter.app` inside the destination. Use a
dedicated output directory outside the repository and preserve any prior
submission artifact first. Never modify the app payload after final signing.

```sh
codesign --verify --deep --strict --verbose=4 "/tmp/quick-presenter-store/Quick Presenter.app"
codesign -d --entitlements :- "/tmp/quick-presenter-store/Quick Presenter.app"
pkgutil --check-signature /tmp/quick-presenter-store/Quick-Presenter-Mac-App-Store.pkg
shasum -a 256 /tmp/quick-presenter-store/Quick-Presenter-Mac-App-Store.pkg
```

## Sandbox ownership

The UI has App Sandbox, user-selected read-only file access, and app-scoped
bookmarks. Version 1.4.0 has no network server feature and requires neither
network client nor network server entitlements.

The UI creates read-only security-scoped bookmarks for selected PDFs and stores
them in its private application-support directory. On reopening, it resolves
the bookmark and holds a balanced scope while that PDF remains active. A moved
or stale bookmark is refreshed. Replacing the active PDF releases older scopes;
clearing recent history clears stored bookmarks while retaining active access
until replacement or exit. Unavailable access falls back to the normal short
open/reopen error and lets the presenter select a PDF again.

Bookmarks and URLs remain in the UI process. The inheriting proxy has no new
independent file authority. The non-inheriting XPC renderer receives only the
read-only file descriptor and retains its separate sandbox; PDFium receives no
bookmark authority.

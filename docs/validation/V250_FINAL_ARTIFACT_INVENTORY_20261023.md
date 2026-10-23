# v2.5.0 final source and artifact inventory — 2026-10-23

Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508).

Follow-up: the [final macOS runtime report](V250_FINAL_MACOS_RUNTIME_20261023.md) records subsequent authorized build-25010 upload, internal TestFlight installation and final installed-app checks. Statements below describe the inventory checkpoint before that upload.

## Fixed inputs and scope

- Build source: `ce42e1de7754a02530fa85a4187b04222ec2585d` (main after #511), including #512.
- Application: **2.5.0**, Control Protocol **1**. macOS Store candidate build: **25010**, superseding the older 25009 for future qualification; not uploaded.
- PDFium: **chromium/8111 / 158.0.8111.0**, upstream source `a50f1a5953264de698f398fe3a2ef6153677fafa`; V8/XFA disabled.
- `Cargo.lock` SHA-256: `b81622fb863fd45e01b465a8c22354229c8883d808f7ac873305e236ff6173a5`.
- PDFium manifest SHA-256: `c9dceacc564ff9a1c22f03de657eaaa3769766d5279ca2c70a369d1266ecc201`.
- The maintainer [explicitly accepted retaining 8111 for this release](https://github.com/koichiro/quick-presenter/issues/508#issuecomment-6358413430). The four identified post-pin changes remain absent; this inventory does not claim their inclusion or comprehensive CVE affectedness.
- Source changes from the previously tested #492 merge `aab4b346eec926e04aa38b6083d7d2851f134856` are the two #511 documentation files only. The artifacts here are bound to the exact source above; merging this report does not change them.

## Artifact/channel selection

| Channel | Target / features | Selected object |
| --- | --- | --- |
| macOS direct | `aarch64-apple-darwin`, default features | Locally Developer ID signed, notarized and stapled DMG below. |
| Mac App Store submission candidate | `aarch64-apple-darwin`, `macos-store-control` | Locally Apple Distribution signed app and installer PKG, build 25010. Payload DMG is only the exact app inventory subject. |
| Ubuntu direct | `x86_64-unknown-linux-gnu`, default features | Exact final-source CI DEB and verified sidecar; details below. |
| Microsoft Store submission input | `x86_64-pc-windows-msvc`, default features | Exact final-source CI Store-identity MSIX and verified sidecar; certification/signing/customer installation remain separate. |

Raw executables, MSI, direct Windows MSIX and ad-hoc macOS CI DMG are validation artifacts, not selected supported release downloads. The six pinned PDFium archives are verified inputs, not a claim of six shipped application targets or Intel/other-architecture runtime coverage. macOS direct minimum is 13.0; the Store candidate's minimum is 14.4. No channel was published or submitted to Store review.

## macOS final package identity

Local builds used rustc/Cargo 1.99.1, LLVM 23.1.3, macOS 27.0.1 arm64 and the locked dependencies. Default and Store-feature release builds completed. Source/lock/manifest inputs were unchanged.

The direct app and DMG use Developer ID Team `NP5Q6U6NT5`. Deep/strict signatures passed. Apple notarization submission `a262aa7c-8234-4d65-8723-193f24af445e` was **Accepted** after explicit maintainer authorization; stapling and validation passed. Gatekeeper accepted both the final DMG and its mounted app as Notarized Developer ID. The mounted app's 112 regular-file hashes exactly match the signed source app.

The Store candidate uses bundle ID `app.quickpresenter.QuickPresenter`, Apple Distribution Team `NP5Q6U6NT5`, and an Apple-issued 3rd Party Mac Developer Installer certificate. Its embedded distribution profile matches the explicit app/team, has no development device/get-task-allow grant and expires October 8, 2027 UTC. Deep/strict app and installer signature checks passed; all 119 regular files expanded from the PKG exactly match the staged signed app. The seven-Mach-O private-CGS scan passed. No TestFlight upload, customer installation, review submission or Store signing is claimed.

Role entitlements were inspected: direct GUI/proxy have no App Sandbox entitlement while the XPC renderer has its independent App Sandbox; Store GUI has sandbox/read-only-selection/bookmark/app-group/production identity entitlements, proxy inherits sandbox, CLI/broker have sandbox plus the control group, and XPC renderer has its own sandbox. This records signed configuration, not a new runtime denial test.

| Artifact | SHA-256 |
| --- | --- |
| `QuickPresenter-2.5.0.dmg` | `5f1ae4ee19299ad3aa136a9a73d814ec352624cb124ca13b7632aae0cc6898b0` |
| `Quick-Presenter-Mac-App-Store.pkg` | `64d8a3697f87afe5a3fa34f928c022370b34078c9de31bcd7d19d7bd66a171b1` |
| `QuickPresenter-2.5.0-25010-store-payload.dmg` | `a0b144ae6574abb2cf9c21d303327f39ca932768c92383820bc2a0604fe3d3b6` |
| `QuickPresenter-2.5.0.dmg.cdx.json` | `43f1dcafe9f0080dd712a3ab7c58a19ca78d224c22fb8c0f7ebc4bc0674a97c1` |
| `QuickPresenter-2.5.0-25010-store-payload.dmg.cdx.json` | `a2973abb254bc9ecf6940a56470e2152131d71c1275c9cc533d9972e3faea8bb` |

The direct sidecar was regenerated **after stapling** and then verified. The two sidecars passed the pinned CycloneDX 1.5 schema/reference checks and full package/source reinventory: direct **489 components**, Store payload **496 components**. The Store sidecar is bound to the signed app payload DMG, not Apple's delivery container or the installer PKG; the PKG hash and exact payload equivalence are recorded separately.

## PDFium inputs and packaged licenses

Fresh downloads of all six selected archives passed the pinned SHA-256, VERSION **158.0.8111.0**, root LICENSE and complete **18-file** component-license inventory checks.

| Archive | SHA-256 |
| --- | --- |
| `pdfium-linux-arm64.tgz` | `042de247138f96341534cccf24ed21c2550a7245b936bb4a60d2340120000f05` |
| `pdfium-linux-x64.tgz` | `bdc60126399f39fe768566a18d4be055a7b3681bc9e248e8a6656b4f6cae0aa2` |
| `pdfium-mac-arm64.tgz` | `46c674298c60123f1195c2a03c65ee08c4783c84874ce4f5b3a054abb3ecaf30` |
| `pdfium-mac-x64.tgz` | `e7fece4693f65a7e47f84fa38099068d573d4dcbd46b1a3c8d2c26581c308466` |
| `pdfium-win-x64.tgz` | `3452d11031050ec65e57f180215b57b9e6d465e3fbedafa8a77ad90885c7d23e` |
| `pdfium-win-x86.tgz` | `ef298314d6e71b08e38dc90724111e37e1d29d668f739ad656e2329d491e533a` |

Both PDFium copies in each signed macOS app match the official arm64 archive in every inspected file-backed Mach-O section. Signature/load-command/linkedit differences from signing are not called whole-file equality. Both complete 18-file inventories, root licenses and VERSION files match exact archive bytes; application GPL license and source-offer notices match the selected source. Whole-file role hashes are retained in the local inventory, separately from the section comparison.

SBOM tooling: cargo-cyclonedx **0.5.9**, jsonschema **4.25.1**, repository schema pins. Direct Cargo graph used default features; Store used `macos-store-control`; both target arm64 and exclude build-only dependencies. Lockfile unchanged. Existing native-inventory limitations in [SBOM policy](../SBOM.md) remain: license evidence is not a complete native component version inventory.

## Final-source CI packages

[Build Binaries 39908348895](https://github.com/koichiro/quick-presenter/actions/runs/39908348895) ran through `workflow_dispatch` on the exact source above. Dependency audit and all three platform jobs completed successfully, including native package construction, installed-package checks where present, and SBOM generation plus complete `--verify` on the corresponding runners. Windows execution occurred on the Windows runner. This is CI package evidence, not customer Store installation or manual GUI acceptance.

Selected downloaded inputs:

| Actions artifact | ID | Downloaded ZIP SHA-256 |
| --- | --- | --- |
| Ubuntu x64 | `12481852922` | `021db4cf071383f0ad36899f75a1de7ffaeab3708c07bde520355616a3e459d3` |
| Windows Store x64 | `12482651689` | `cba59393ac3dba9a2d9cf0b47b36d0c7635fd976005fc9f00f96ab378eff6f36` |

Each ZIP matched GitHub's recorded artifact digest. This is archive integrity evidence, not an independently verified artifact attestation.

| Selected package | Package SHA-256 | Sidecar SHA-256 | Components / payload files |
| --- | --- | --- | --- |
| `quick-presenter_2.5.0_amd64.deb` | `5ed07c1ef3b2a902d3f0717659c5b61f9de75c56d5055950be043fcb6cfb3230` | `298072c2f4762a434205fdee1cc9c7396b11dc802623f176b2ff227ea84ac1de` | 532 / 70 |
| `QuickPresenter-2.5.0.msix` | `4074a3f0e5a8ea074f5c21f7209752918fbba83e9e1ece6bc8307933c365b9c5` | `53feff8cf1c01e3ae4b39c960a3c80104ad42569a667906c69a9217af4343876` | 415 / 62 |

Independent local inspection passed pinned CycloneDX schema/reference checks and source/target/version/package-hash/all-payload-file-hash comparisons for both packages. ELF/PE headers are x64. Bundled PDFium libraries, VERSION, root/component licenses and the distribution URL/archive hash match the corresponding freshly verified official inputs. All 18 license evidence records match their upstream bytes. Application license/source-offer notices match their source checkout bytes.

Windows checkout uses CRLF: its recorded `Cargo.lock` SHA-256 is `accb0a008af597086af3586b50bd689285eb9cfe0dcdbdb3ad0666b76e0f57eb`, independently reproduced by converting the exact committed LF source to CRLF. Application license/source-offer text likewise matches exact CRLF checkout bytes. PDFium archive files are compared without such conversion. This is a checkout byte difference, not a dependency change; do not equate the two lockfile whole-file hashes.

DEB identity is `quick-presenter` / **2.5.0** / **amd64**. The selected MSIX identity is `KoichiroOhba.QuickPresenter`, publisher `CN=A3F64AFD-6298-42F8-BAC9-DB0AB1831F51`, version **2.5.0.0**, **x64**. It has no `AppxSignature.p7x`: this is the unsigned Partner Center submission input. CI installed-package tests use their separately test-signed copy; neither CI signing nor this input constitutes Microsoft Store certification or final customer trust.

The 21 existing release-SBOM contract tests passed locally. No Rust/Slint/build source was changed by the results report; Cargo checks/tests are not repeated for this documentation-only change.


## Remaining qualification and preservation

This completes source/package identity, provenance and SBOM inventory for the selected objects, not all of #508. Final signed/channel runtime, customer Store identity/install/update, DMG icon-drop disposition, environment/authority/bookmark and initial-lifecycle traceability gates remain tracked there. Repackaging, re-signing, stapling or changing code/dependencies/features invalidates affected hashes/sidecars and requires corresponding verification. Preserve the exact selected packages for those next tests.

Local selected files and machine-readable result inventories are preserved under `artifacts/v250-final-20261023/` in the maintainer's primary workspace, outside Git. No raw smoke logs, credentials, account details or package binaries are committed. Actions storage is expiring; required release sidecars/source/provenance must be preserved and attached to the eventual release through the normal publication workflow. No release/tag/customer distribution was created by this work.

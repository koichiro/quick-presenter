# v2.5.0 release integration after #517 / #518 — 2026-10-24

Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508).
This is results-only evidence and a remaining-gate register, not publication,
customer Store submission approval, or acceptance of unperformed tests.

## Source and artifact boundaries

Merged baseline: **b4910ea458d00c40563a3f500b11b4837cd6ff2e** (#518).
#517 is merged as `d15ded272413cdf716dbf2606a0e71162df07e0f`.
Compared with the earlier package source `ce42e1d`, the only non-documentation
change is Cargo.lock's **clipboard-win 5.4.1 → 5.4.3** entry/checksum. Application,
vendor, UI, packaging, build scripts, manifests and native PDFium inputs are
unchanged. The crate is absent from macOS and Linux normal dependency graphs.
Compared with the previously tested updated Windows source `cf12d7f`, merged
main changes documentation only.

The existing macOS and Linux artifacts retain their **actual ce42e1d build
source**, original Cargo.lock hashes and sidecars. They are not relabeled as
b4910ea builds. Reinventory used the archived original source for package/source
comparison, then independently compared each package's normal Rust graph with
fresh, locked, target-filtered metadata from merged main.

| Preserved artifact | Actual build source | SHA-256 | October 24 recheck |
| --- | --- | --- | --- |
| Developer ID arm64 DMG | ce42e1d | `5f1ae4ee19299ad3aa136a9a73d814ec352624cb124ca13b7632aae0cc6898b0` | Exact SBOM reinventory, schema and deep/strict app signature passed: 489 components / 112 regular files. Merged-main normal graph matched: 358 libraries / 1001 edges. |
| Mac Store build 25010 submission PKG | ce42e1d | `64d8a3697f87afe5a3fa34f928c022370b34078c9de31bcd7d19d7bd66a171b1` | Supporting exact payload DMG below rechecked; earlier PKG/submission/TestFlight evidence remains scoped to this package. |
| Store 25010 exact submission-payload DMG | ce42e1d | `a0b144ae6574abb2cf9c21d303327f39ca932768c92383820bc2a0604fe3d3b6` | Exact SBOM reinventory, schema and deep/strict app signature passed: 496 components / 119 regular files. Merged-main normal graph matched: 358 libraries / 1001 edges. This is not Apple's distribution container or a direct-release notarized DMG. |
| Ubuntu x64 DEB | ce42e1d | `5ed07c1ef3b2a902d3f0717659c5b61f9de75c56d5055950be043fcb6cfb3230` | Source/lock/package/target/schema/references/all 70 payload files/links/CPU/official PDFium and 18 licenses matched: 532 components. Merged-main normal graph matched: 443 libraries / 1261 edges. |
| Updated pre-merge Windows Store MSIX | cf12d7f | `a72f08e047ed44466574c43b8b3c19d242a7eeeef42b32e6bd79b36bf0fbbdd6` | #518 records exact package/SBOM validation and Windows runtime. It is separate from the post-merge build below. |

Original sidecar digests, signatures/notarization and executed runtime scope
remain in the [original inventory](V250_FINAL_ARTIFACT_INVENTORY_20261023.md),
[Mac runtime](V250_FINAL_MACOS_RUNTIME_20261023.md),
[document delivery](V250_MACOS_DOCUMENT_DELIVERY_20261023.md),
[Linux runtime](V250_FINAL_LINUX_RUNTIME_20261024.md), and
[Windows clipboard review](V250_CLIPBOARD_WIN_REVIEW_20261024.md).
No package or sidecar was modified by these rechecks. Normal graph equality
and source review do not claim a reproducible binary build.

[Build Binaries run 40059375645](https://github.com/koichiro/quick-presenter/actions/runs/40059375645)
completed successfully on exact merged main **b4910ea**: audit and all three
platform/package/SBOM jobs passed. The post-merge Windows Store input is:

- Artifact **12547490525**; ZIP SHA-256
  `1219c4b29f77869b45a1b1d5c333c51d77d1037a57506942f0aa8dfd8aac503f`,
  independently matched to GitHub's artifact digest and source revision.
- MSIX SHA-256: `9e092f8a701398602bd96f1dd29a67677cf8c1a029d65e7a5e2db41d3361e183`.
- Sidecar SHA-256: `a6d0ae281f0cd67841b0f45352994dafe4cc8e4bad95d4c17b56ceb6868438c1`.
- Independent root verification matched source/CRLF lock/target/Store identity,
  package and all **62** payload files, schema/references, **415** components,
  **334** Rust libraries/**861** normal edges, clipboard-win **5.4.3** and its
  official checksum, official PDFium 8111 and all **18** component licenses.
- The Windows session executed this exact extracted MSVC payload: **10 CLI
  checks**, actual rendering/confinement denial checks and **126 GUI checks**
  (63 short-note + 63 long-note) passed. A local verification-harness assumption
  about license placement was corrected to the actual `licenses/` layout;
  it was not an application/package change. Validation processes were stopped;
  the existing customer Store 2.0 installation, settings and clipboard were retained.
- Actual physical-keyboard/Narrator rechecks on this exact new payload were not
  performed. Earlier maintainer confirmations retain their exact earlier-payload
  scope. This MSIX remains **unsigned Store submission input**, separate from
  actual Microsoft Store signing/install/update/alias qualification in #520.

Exact package/sidecar/ZIP metadata and results are preserved outside Git in
`artifacts/v250-release-integration-20261024/`; the Windows results-only report
is retained in its session's `artifacts/windows-main-b4910ea-20261024/`.
The old ce42e1d Windows package (clipboard-win 5.4.1) remains historical.
A subsequent changed candidate, including #521, requires its own affected
source/package/SBOM qualification; these results are not silently relabeled.

## Additional Apple-distributed authentication probe

The installed Apple-distributed TestFlight **25010** app was launched normally.
Its packaged legitimate CLI returned status successfully immediately before
and after a separate lookup probe. No PDF was changed by the probe.

The probe ran inside the existing Xcode Python interpreter. Runtime Security
framework checks returned a valid signature with team **59GAB85EFG** and signed
identifier **com.apple.python3**, different from product team **NP5Q6U6NT5** and
expected CLI identifier **app.quickpresenter.qp**. A temporary test library sent
only a lookup dictionary to the existing broker service, without performing
the product CLI's local signing checks or imposing its own peer requirement.
The response was **Connection interrupted**, with no accepted dictionary or
endpoint. The surrounding legitimate CLI calls both succeeded. No Apple
binary, entitlement, signature, system trust or access policy was modified.
The app was then quit, restoring the original not-running state.

This establishes that this actual different-team/different-identifier process
was not accepted in the bounded attempt. The generic interruption does not
identify the rejection reason or isolate the team condition from the identifier
condition. It does **not** close the foreign-team/cross-user authority gate.
An initial invocation inside the command sandbox could not run the signed CLI;
the valid comparisons above used the normal host execution context. That
harness restriction is excluded from application-failure evidence.

## Remaining release gates

These remain open unless executed or explicitly dispositioned by the maintainer.
Closing the earlier implementation/lifecycle issues did not waive them.

| Condition | Current evidence and next action |
| --- | --- |
| Post-merge Windows replacement — completed for b4910ea | Build, independent package/SBOM inventory and exact-payload Windows runtime passed as recorded above. Preserve its exact source and manual-test limitations; later candidate changes require affected requalification. |
| Customer Store installations — transferred | On October 24 JST the maintainer explicitly moved actual customer Mac/Windows Store 2.5.0 qualification to [#520](https://github.com/koichiro/quick-presenter/issues/520), after review/certification, with all customer checks **unperformed**. This timing condition no longer independently blocks #508; #520 stays open for actual install/update/signature/identity, registered Windows alias and installed workflows. TestFlight/CI/extracted-payload checks remain separate. This decision authorizes neither customer review submission nor publication and leaves every other #508 gate in force. |
| Unavailable Mac environments | No additional Intel/other-supported-macOS machine is available. Testing or an explicit bounded release disposition is needed. |
| Actual product authority | Cross-user rejection and sufficiently attributable foreign-team rejection on the Apple-distributed product remain incomplete. The bounded probe above is supporting evidence only. Genuine OS revocation of persisted security-scoped bookmark authority remains untested; moved files, ACL denial and stopAccessing are not substitutes. |
| First-attempt trace | #493 has verified continuation/reinstall and manual initial picker/TIMEOUT observations, but the original automated preflight/TIMEOUT trace was lost. A pristine-account rerun or explicit acceptance of this evidence limitation is needed. |
| Supplemental Linux renderer failure | Forced winit-femtovg under actual XWayland failed the with-PDF notes-resize assertion on both fixtures (69/1). Default native Wayland and actual XWayland later passed 70/0 each. Investigation captured a stale 16px value naturally reaching 24px about 985ms after the failed check: insufficient smoke synchronization. [#521](https://github.com/koichiro/quick-presenter/pull/521) adds bounded convergence checks and retains strict assertions; final validation passed 72/0 in all eight backend/renderer/fixture combinations, 497 Rust tests and a fault-injection negative control. CI/review passed. Merge and affected final-package qualification remain; the corrected validation bundle is not the original DEB. |
| Security/disclosure checkpoint | Preserve the accepted PDFium 8111 four-change residual-risk disposition and five maintenance-warning deferrals. Repeat the review on October 26 or immediately before publication, whichever is earlier; the October 28 candidate checkpoint is unchanged. A fresh review is not promised as background monitoring. |

## Customer Store follow-up disposition

The maintainer chose on **2026-10-24 JST** to perform customer Store acceptance
after review/certification and track it separately in [#520](https://github.com/koichiro/quick-presenter/issues/520).
Customer Mac/Windows checks remain **UNPERFORMED**, not passed. This is an
explicit v2.5.0 timing decision; it does not silently import the v2.0.0 policy.
The maintainer subsequently approved the staged execution workflow: Mac App
Review with manual release, approved-version promo-code installation before
general release; Windows Package-flight certification and limited-group Store
installation; then general release and public listing/normal update checks.
Record exact audiences/builds and certification notes; flight certification is
separate from regular-submission approval. Candidate conditions in #508 must be
resolved first. The [official Apple promo-code procedure](https://developer.apple.com/help/app-store-connect/offer-promo-codes/request-and-manage-promo-codes/)
and [Microsoft Package-flight procedure](https://learn.microsoft.com/en-us/windows/apps/publish/package-flights)
support this limited-distribution phase. No Store action has been executed by
recording the plan.

The follow-up binds each actual approved/distributed build to its submitted
source/package/SBOM and records fresh/update installation, trust/identity and
installed workflows (including the Windows registered alias). It cannot be
automatically closed by #508 or this documentation PR. Other environment,
authority/revoked-bookmark, traceability, Linux failure and security/artifact
conditions remain in #508. No customer submission or publication occurred.

Already recorded bounded maintainer decisions remain in force: install before
bundle-icon drop on macOS (read-only DMG drop remains unsupported); Linux
physical dual-display coverage unperformed and Orca excluded from this release
gate; current PDFium 8111 retained with the four specified post-pin omissions.
None makes other unperformed tests pass or approves publication.

Private package/SBOM/result evidence remains outside Git. No raw smoke logs,
clipboard contents, user settings or document paths are included in this report.
Keep #508 open until its remaining gates are completed or explicitly resolved.

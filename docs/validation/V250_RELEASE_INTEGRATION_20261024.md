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

A new [Build Binaries run 40059375645](https://github.com/koichiro/quick-presenter/actions/runs/40059375645)
was dispatched on exact merged main b4910ea to produce a post-merge Windows
submission input and matching SBOM. At this report's initial preparation it is
**in progress**; its package hashes and qualification must be added before
selecting that replacement. The Windows session has been asked to qualify the
exact resulting MSVC payload. Do not publish the old ce42e1d Windows package
(containing 5.4.1) as the updated candidate. CI signatures/unsigned submission
input remain distinct from Microsoft customer Store distribution.

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
| Post-merge Windows replacement | Finish the b4910ea build, record exact package/sidecar hashes, independently verify them, and execute the exact MSVC payload on Windows. Preserve previous failures and limitations rather than silently replacing them with later passes. |
| Customer Store installations — transferred | On October 24 JST the maintainer explicitly moved actual customer Mac/Windows Store 2.5.0 qualification to [#520](https://github.com/koichiro/quick-presenter/issues/520), after review/certification, with all customer checks **unperformed**. This timing condition no longer independently blocks #508; #520 stays open for actual install/update/signature/identity, registered Windows alias and installed workflows. TestFlight/CI/extracted-payload checks remain separate. This decision authorizes neither customer review submission nor publication and leaves every other #508 gate in force. |
| Unavailable Mac environments | No additional Intel/other-supported-macOS machine is available. Testing or an explicit bounded release disposition is needed. |
| Actual product authority | Cross-user rejection and sufficiently attributable foreign-team rejection on the Apple-distributed product remain incomplete. The bounded probe above is supporting evidence only. Genuine OS revocation of persisted security-scoped bookmark authority remains untested; moved files, ACL denial and stopAccessing are not substitutes. |
| First-attempt trace | #493 has verified continuation/reinstall and manual initial picker/TIMEOUT observations, but the original automated preflight/TIMEOUT trace was lost. A pristine-account rerun or explicit acceptance of this evidence limitation is needed. |
| Supplemental Linux renderer failure | Forced winit-femtovg under actual XWayland failed the with-PDF notes-resize assertion on both fixtures (69/1). Default native Wayland and actual XWayland later passed 70/0 each. The maintainer authorized investigation and correction in the existing Linux session on October 24; that work is in progress. Later default passes do not establish its cause. |
| Security/disclosure checkpoint | Preserve the accepted PDFium 8111 four-change residual-risk disposition and five maintenance-warning deferrals. Repeat the review on October 26 or immediately before publication, whichever is earlier; the October 28 candidate checkpoint is unchanged. A fresh review is not promised as background monitoring. |

## Customer Store follow-up disposition

The maintainer chose on **2026-10-24 JST** to perform customer Store acceptance
after review/certification and track it separately in [#520](https://github.com/koichiro/quick-presenter/issues/520).
Customer Mac/Windows checks remain **UNPERFORMED**, not passed. This is an
explicit v2.5.0 timing decision; it does not silently import the v2.0.0 policy.
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

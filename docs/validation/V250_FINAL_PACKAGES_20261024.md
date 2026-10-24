# Final v2.5.0 packages after #523 — 2026-10-24

Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508).
Frozen package source: **f4c018091c6057eb68fc76ac761bc90b10f93cc2**, merged #523,
including shared correction #524 and the final bundled CLI documentation.
The Rust/Slint/dependency/vendor/build/packaging inputs match tested correction
source f596ef8; publication documentation is now included in the actual packages.
This record preserves older source/package identities rather than relabeling them.

## Candidate inventory

[Build Binaries 40088331020](https://github.com/koichiro/quick-presenter/actions/runs/40088331020)
completed successfully on this exact source: dependency audit and all three OS
package/SBOM jobs passed. The CI macOS ad-hoc package remains a layout/fail-closed
check, separate from the signed direct and Store candidates below.

| Candidate | SHA-256 | Exact-package evidence |
| --- | --- | --- |
| Developer ID arm64 `QuickPresenter-2.5.0.dmg` | `2859fef39d709e763a934137d5ef63db7dd47f2f1ecf1855b5d47d9ac34948e4` | Apple notarized/stapled, Gatekeeper accepted; 489 SBOM components / 112 regular app files. |
| Store arm64 build 25011 `Quick-Presenter-Mac-App-Store.pkg` | `c4624c4920c7a84caa3470dd325fdaadb7047cfac424de263d685ebe288b28f7` | Distribution app/installer signature and valid distribution profile; all 119 expanded app files match the signed submission app. Uploaded to internal TestFlight after explicit approval. |
| Store 25011 exact submission-payload DMG | `14594734a9b71521279827bb71ef3cb055651112d326a022d50e8827560b999d` | 496 components / 119 regular files; supports the PKG inventory, not a public direct-release DMG or Apple's download container. |
| Windows x64 Store `QuickPresenter-2.5.0.msix` | `2405a70267124eead9384547390a6e0cf0918f0e409c5727fa6cfc8194ab2114` | Actual CI MSVC Store artifact 12558347508; unsigned Store submission input, 415 components / 62 payload files. |
| Ubuntu x64 `quick-presenter_2.5.0_amd64.deb` | `39134fcb5bcbd356698108021fe90f77d2fc34b044cf020ab50e53d2c645581f` | Actual CI artifact 12557279595; 532 components / 70 payload files/links. |

Exact sidecar SHA-256 values, in the same direct/Store-payload/Windows/Linux order:

- `66d9e2962189bd44ae7dc29b1571043d8ac208b40329851c23d815ce15c3a38c`
- `026bc18d3d38ecec3dc7b838ac9389715561bea302e2667f40f3ae17d24708ea`
- `d188ef9a66474e8c7520879dfab3c1b29353a463684383b88a6cd8ceb67d13d2`
- `ca6f07e5036a4a0a46b6a1143a372b3ffbc3a43d03b859bfd2a5e3da67604ff9`

Root independently matched GitHub ZIP digests for Windows Store
`d8e35003a294316e4ff7aa90e690ad65f25894f2fce41bfe7e930bd1f90392eb`
and Ubuntu `16b1e24059fdc4190c7ad0438770f158ed0f134e1f046950d55c5d12de7a1953`.
Package/source/target/lock/schema/reference/all-payload/license/CPU checks passed.
Target-filtered normal Rust graphs match the final-source metadata: Mac
**358 libraries / 1001 edges**, Windows **334 / 861**, Linux **443 / 1261**.
The Windows graph contains corrected clipboard-win 5.4.3; Mac/Linux do not.
All final packages contain the exact final CLI/protocol documents (Windows
checkout line endings accounted for), license and source-offer content.

Retained PDFium is **158.0.8111.0**. Both signed Mac PDFium copies match official
archive file-backed sections and all 18 licenses; Windows/Linux PDFium matches
official archive bytes, version and licenses. Signatures change whole-file Mac
hashes, so executable-section equality is not described as unsigned-file equality.

## macOS direct qualification — completed

Rebuilt release binaries from f4c0180, staged the final documents and signed with
Developer ID. The pre-submission DMG hash was
`23f4469687fba03cb4b0d874ba727d27a0058deb7d6c70bfd9e86a73aa69bbae`.
After explicit maintainer authorization, Apple notarization submission
`279b89ff-9a70-458d-a413-19659199faed` returned **Accepted**. Stapling and
validation passed. The final post-stapling hash is the inventory value above;
the sidecar was regenerated and verified after this mutation.

Gatekeeper accepted both the final DMG and mounted app as Notarized Developer ID.
Deep/strict signatures passed, the mounted app matched all 112 signed-source
regular files and retained its Applications link. Actual signed-XPC render/notes
and unrelated-file/network/child denial checks passed against both the staged
app and the final mounted DMG. App-owned GUI regression passed **180/0**.
This is current-machine arm64/macOS evidence, not additional hardware coverage.

## Apple-distributed Store 25011 qualification — completed

The maintainer approved the exact 40,799,258-byte PKG upload for internal
TestFlight only and completed Apple's export-compliance declaration directly.
The build was added only to the existing **Release Validation** internal group.
TestFlight updated the installed application from 25010 to **25011**.

Installed build, receipt, deep/strict signatures, product team, Apple-anchor
TestFlight signing requirement and Gatekeeper `source=Testflight` passed.
All **seven Mach-O files** match the submitted source app in inspected
file-backed sections; final bundled documents match byte-for-byte. Apple
re-signing is not claimed to preserve whole-file PKG/installed equality.

Using container-local fixtures, output and temporary paths, actual-XPC
render/notes and unrelated-file/network/child denial passed; the applicable
Store GUI suite passed **179/0**. Authenticated bundled CLI checks passed
**30 calls / 6 watch events**, covering version, Unicode/space paths, exact
empty/English/Japanese notes, full text/context/export, navigation/blackout,
query/mutation rejection, close and authorized reopen. The product XPC suite
also passed read-only queries/export, watch/navigation, goto/blackout,
close/authorized reopen and **32 repeated connections**.

Local execution of the submission-signed candidate terminated with status 137;
that attempt is not counted as a runtime pass. Initial external-path Store
execution encountered sandbox access denial; container-local qualification
retains the sandbox and denial assertions. An immediate CLI invocation during
normal launch returned NOT_RUNNING; after observing a ready GUI, the unchanged
binary passed the bounded CLI and product-XPC suites. These original attempts
remain in private evidence; no application timeout/security setting was relaxed.

Validation GUIs exited normally and the two original settings JSON files were
restored byte-for-byte. The installed TestFlight build remains 25011. No new
foreign-team/cross-user/revoked-bookmark or customer App Store claim is made.
Earlier maintainer manual drop/keyboard/lifecycle evidence retains its actual
source/channel scope; these automated runs do not invent new physical input tests.

## Windows final MSVC qualification — completed

[Windows final handoff](https://github.com/koichiro/quick-presenter/issues/508#issuecomment-6383662700)
confirms **10 CLI checks**, actual rendering/confinement denial and six
actual-final-MSIX GUI runs at **65/0 each**: default short/long twice each,
forced FemtoVG short/long once each, without unexpected failures or retries.
Root independently verified the exact CI input and inventories above. Validation
apps are stopped (0); the customer Store 2.0 installation, original settings,
clipboard and trust state are unchanged. No other build was stopped.

The prior f596ef8 delayed/suppressed/same-size and fitting controls remain
attributed to that source. Final f4c0180 has no changed runtime/smoke inputs,
so no redundant diagnostic compilation is needed solely to reproduce them.
Final short/long forced-software checks each retained the known **41/1**
rendering-notification limitation after notes checks; no full software-renderer
pass is claimed. Actual customer Store installation,
Microsoft signing, update and registered-alias qualification remain in #520.

## Linux final DEB qualification — completed

[Linux final handoff](https://github.com/koichiro/quick-presenter/issues/508#issuecomment-6383689595)
and root package/document/graph verification passed as above. On the Linux machine,
all eight extracted-final-DEB conditions passed **72/0 each**: native Wayland /
actual XWayland × default / forced FemtoVG × short / long notes, with expected
negative-control detection. The maintainer ran the prepared package lifecycle
command in a normal terminal because the agent environment prohibited sudo.
Final installation, removal/registration cleanup and reinstall all passed;
all **70 installed files/links** match the selected final DEB. PDF defaults
were retained.

The actual installed package then passed the same **eight conditions at 72/0
each**, CLI version/contracts, actual isolated rendering/denial and the installed
CLI suite (**66 calls / 18 events**). Validation processes are stopped; original
application/accessibility settings and the default PDF application match their
pre-validation state. The final Linux results are recorded in #508, distinct
from the unpacked-payload evidence and historical candidate failures. Both
subjects also passed 10 standalone CLI contracts, eight plaintext rendering
probes and encrypted-PDF rejection. Final font/frozen-geometry controls rejected
the injected faults and a single dropped geometry request recovered; applied
layout-suppression controls retain their original f596ef8 subject identity.

## Security source comparison

Directly compared security-reviewed source
**80522547ca1e5b2d1a5799f04ab542cc22f13e62** with frozen package source
**f4c018091c6057eb68fc76ac761bc90b10f93cc2**. A full tracked-tree comparison
found exactly five changed paths: `docs/RELEASE.md`, `docs/validation/README.md`,
`docs/validation/SECURITY_REVIEW_20261024.md`, `src/gui_smoke.rs` and
`ui/app.slint`. The reviewed branch did not yet contain #524's final smoke/UI
correction; the earlier description that it already included merged #524 was
incorrect. This incremental review closes that source-attribution gap.

| Input | Direct comparison and security disposition |
| --- | --- |
| Dependencies and audit selection | Cargo.toml, Cargo.lock and the entire .cargo tree are identical Git objects. Lock SHA-256 remains `0918bd018726e361bfd29c3665eae7e850a6348c63064814973bb19d0e180733`; no feature, registry-checksum or advisory-ignore change. |
| Vendor/native selection | Entire vendor tree and scripts/pdfium_manifest.json are identical; the manifest SHA-256 remains `c9dceacc564ff9a1c22f03de657eaaa3769766d5279ca2c70a369d1266ecc201`. The final-package native-byte/section/license checks above bind the selected 8111 content to this source. |
| Build and packaging | build.rs and the complete scripts, packaging, assets and .github trees are identical Git objects. Bundled docs/CLI.md and docs/CONTROL_PROTOCOL.md are unchanged between these revisions. |
| Application Rust | Every src file except gui_smoke.rs is identical, including PDF parsing/brokered rendering, control authentication, native document/file-access bridges and error handling. |
| src/gui_smoke.rs exception | #524 adds observed geometry, actual portrait/menu-layout checks, a 200ms stability requirement and bounded geometry retries within the existing 5s check. These functions are reached through the explicitly selected GUI smoke path. Review found no new parser/native loader/IPC authority, unbounded wait or permission change. |
| ui/app.slint exception | Adds read-only bindings exposing existing slide/notes/menu bounds and names the existing inline-menu Rectangle. It adds no input, action/callback, file access or rendering-authorization path and changes no production layout expression. |
| Three document exceptions | Release/index/security-result records only; none of these three changed documents is bundled by the package scripts. |

The two implementation diffs were inspected directly, rather than treated as
byte-identical merely because the dependencies match. Their geometry-observation
scope does not change the assessed native vulnerability reachability or
identity/permission/renderer enforcement. Final all-OS package execution and
controls are scoped above. The existing Cargo/native advisory findings and
accepted residual-risk dispositions therefore remain applicable after this
incremental source review; this is not a new advisory-feed refresh or an
exhaustive audit of every execution path. These review fixes change documents
only and do not mutate the qualified f4c0180 packages or their sidecars.

## Release decisions and completion boundary

The [fresh pre-publication security checkpoint](SECURITY_REVIEW_20261024.md)
identified no new blocker. The direct comparison and incremental review above
connect its findings to final f4c0180, including the two smoke/UI exceptions.
Retain the accepted PDFium 8111 four-omission disposition and five
maintenance-warning deferrals. A new material disclosure/input change requires
triage; retain the October 26-if-delayed and October 28 checkpoints.

Existing maintainer dispositions remain: install before DMG app-icon drop;
additional Mac environments and specified Mac authority tests excluded and
unperformed; #493 manually completed; Linux physical dual-display unperformed
and Orca excluded; actual customer Store qualification transferred to #520.
These decisions do not turn missing coverage into passing tests.

Final candidate requalification is complete within the explicit scopes above.
#508 records that completed qualification; this PR supplies the final results
record and release/index links for review/merge. There is no remaining execution
blocker identified by these checks.
Customer Store distribution remains separately unperformed in #520.
This report and its release/index links do not change bundled documents or
executable/build inputs; a later results-only merge does not relabel the packages' actual f4c0180 build source.
No release tag/publication or customer Store review submission occurred.

Packages, sidecars, provenance inventories and private original attempts are
preserved under the primary workspace's ignored
`artifacts/v250-release-final-20261024/`, with SHA256SUMS. Commit results only;
raw stdout/stderr, account data, user settings and clipboard contents remain
outside Git.

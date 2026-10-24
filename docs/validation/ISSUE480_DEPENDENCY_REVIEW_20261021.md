# Dependency maintenance review — 2026-10-21

Tracking: #480. Release validation follow-up: [#508](https://github.com/koichiro/quick-presenter/issues/508).
Source baseline: `430cef0` (main, application 2.5.0).
Owner: release maintainer. Next review: **2026-10-23 or before v2.5.0 release,
whichever comes first**. This is a review record, not an advisory exception.
`.cargo/audit.toml` remains `ignore = []`; v2.0.0 and its disclosed deferrals
remain unchanged.

## Cargo dependencies

`cargo audit --json` on 2026-10-21 returned no vulnerability advisories and five
unmaintained warnings. This is Cargo advisory inventory, not a security verdict
for the application or PDFium.

Commands used for reachability include `cargo tree --locked --all-features
--target all -i bincode --edges normal,build` and the equivalent `ttf-parser`
query for macOS arm64, Windows x64, and Linux x64. `--target all` also includes
the other supported target architectures. Application features do not activate
all optional features of transitive crates.

| Dependency | Evidence and impact | Disposition |
| --- | --- | --- |
| bincode 2.0.1, RUSTSEC-2025-0141 | Retained in Cargo.lock as an optional dependency of typed-index-collections; no active normal/build reverse path with all application features and all targets. Application control and renderer IPC use bounded JSON framing, not bincode. | Defer upstream removal; do not patch the lockfile by hand or enable the optional bincode feature. Recheck after Slint/typed-index-collections updates. This graph result does not prove binary absence for arbitrary future configurations. |
| ttf-parser 0.25.1, RUSTSEC-2026-0192 | Active on Linux through winit → sctk-adwaita → ab_glyph → owned_ttf_parser. No active macOS/Windows path in the queried configurations. The path parses UI title fonts in the UI process. | Defer migration pending a compatible upstream title-font replacement and Linux Wayland qualification. Removing title drawing or switching to crossfont/FreeType changes behavior/native dependencies and needs its own validation. No advisory ignore or release waiver is introduced. |
| zbus_names 4.3.4, RUSTSEC-2026-0371 | Newly disclosed maintenance warning, dated 2026-10-16. | Track the owning D-Bus dependencies' migration to zbus 6; no application IPC migration is implied. |
| zvariant 5.15.0, RUSTSEC-2026-0370 | Newly disclosed maintenance warning, dated 2026-10-16. | Track upstream migration; retain the warning for release review. |
| zvariant_derive 5.15.0, RUSTSEC-2026-0372 | Newly disclosed maintenance warning, dated 2026-10-16. | Track with zvariant; retain the warning for release review. |

The upstream typed-index-collections default branch (4.0.0-alpha.3) still declares
optional bincode 2.0.1. The upstream sctk-adwaita default branch (0.12.0) offers
an optional skrifa renderer, but the vendored winit 0.30.13 uses sctk-adwaita
0.10.1 and Smithay 0.19; this is not a drop-in feature toggle for the current
decoration integration. Coordinate through the owning Slint/winit dependency
update rather than overriding incompatible decoration/Wayland types locally.
Sources: [typed-index-collections Cargo.toml](https://github.com/zheland/typed-index-collections/blob/master/Cargo.toml),
[sctk-adwaita Cargo.toml](https://github.com/PolyMeilex/sctk-adwaita/blob/master/Cargo.toml).

The unmaintained advisories list no patched versions. A local API-incompatible
replacement of a transitive parser is not justified by the advisory alone.
For Linux font reachability, installed system/user fonts used by Wayland window
decorations are in scope; PDF fonts are handled independently by PDFium. Renderer
confinement does not isolate the UI title-font parser. A new release must still
review the retained warnings under `docs/RELEASE.md`.

Primary sources:

- [bincode advisory](https://github.com/RustSec/advisory-db/blob/main/crates/bincode/RUSTSEC-2025-0141.md)
- [ttf-parser advisory](https://github.com/RustSec/advisory-db/blob/main/crates/ttf-parser/RUSTSEC-2026-0192.md)
- [zbus_names advisory](https://github.com/RustSec/advisory-db/blob/main/crates/zbus_names/RUSTSEC-2026-0371.md)
- [zvariant advisory](https://github.com/RustSec/advisory-db/blob/main/crates/zvariant/RUSTSEC-2026-0370.md)
- [zvariant_derive advisory](https://github.com/RustSec/advisory-db/blob/main/crates/zvariant_derive/RUSTSEC-2026-0372.md)

## PDFium candidate adoption

The production baseline pins `chromium/8076` (156.0.8076.0). The candidate pin
in this change is `chromium/8111` (**158.0.8111.0**), published 2026-10-19 UTC
(2026-10-20 JST). The manifest is generated with:

```sh
python3 scripts/fetch_pdfium.py --update-manifest --version chromium/8111
```

All six supported archives have upstream SHA-256 digests in the manifest.
The macOS arm64 archive was downloaded, verified, and its extracted VERSION
checked. PDFium's root LICENSE is unchanged from 8076. The extracted license
inventory additionally contains `dragonbox-Apache2-LLVM.txt`, `dragonbox-Boost.txt`,
`harfbuzz.txt`, and `highway.txt`; staging copies the complete license directory.
Do not copy only libpdfium when refreshing a package.

The official [8100 release](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium/8100)
and [8111 release](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium/8111)
change lists include FreeType updates, parser/graphics changes, non-finite text
object handling and an integer-overflow fix in FaxG4GetRow
([commit](https://pdfium.googlesource.com/pdfium/+/686814d06630e1684d302d1629580a991e28d3e5)).
These changes reach functionality relevant to untrusted PDF rendering and
justify preparing the new pin rather than carrying the old release deferral
forward without investigation. Change titles do not establish CVE coverage.

### Security relevance triage

The [2026-10-20 Chrome stable announcement](https://chromereleases.googleblog.com/2026/10/stable-channel-update-for-desktop_02018666648.html)
includes CVE-2026-113212 (PDF memory corruption) and CVE-2026-113022 (PDFium memory
corruption). The official CVE records identify Chrome before 156.0.8078.47;
that Chrome version range cannot be applied numerically to standalone PDFium.
The [PDFium security page](https://www.chromium.org/Home/chromium-security/pdfium-security/)
is policy/context rather than a complete release-specific CVE inventory.

#### Public revision and configuration check — 2026-10-22

The CVE records link Chromium bug IDs, which also appear in public PDFium commit
messages. This establishes the following bug-linked changes, not an upstream
statement that these commits exhaust every aspect of each restricted report.

| CVE and public evidence | PDFium 8076 | PDFium 8111 |
| --- | --- | --- |
| [CVE-2026-113212 record](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/113xxx/CVE-2026-113212.json), bug 572237007. [e764cc088a62](https://pdfium.googlesource.com/pdfium/+/e764cc088a62b5f0510a73bed4e19c88c9af50bc) guards CJS_Runtime against destruction during callbacks; [ac3cf1684db0](https://pdfium.googlesource.com/pdfium/+/ac3cf1684db05bd68338acb84e3823e6feb72b5b) checks form-fill environment teardown during callbacks. | Neither change is present in its ancestry. Do not describe 8076 as containing these fixes. | Both changes are ancestors; the runtime and form-fill teardown checks remain in the source snapshot. |
| [CVE-2026-113022 record](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/113xxx/CVE-2026-113022.json), bug 494352583. [48af8b610572](https://pdfium.googlesource.com/pdfium/+/48af8b61057256b50cfdb4eae55b9b050924b48f) removes OnKeyStrokeCommit to prevent on-load JavaScript from bypassing the user-gesture requirement. | This change is already an ancestor; the removed method is absent from the source snapshot. | The same change is an ancestor and the removed method remains absent. |

The matching Chromium autorolls independently link the same bug IDs to the
PDFium revision ranges: [572237007 roll](https://github.com/chromium/chromium/commit/6b0e7c5fa87f612687603435f09c690b7b962865)
and [494352583 roll](https://github.com/chromium/chromium/commit/aaeb7a5fc5ef57274a2a73f0df2ea415b9400aac).
The second change's user-gesture description is narrower than the CVE's memory
corruption description; retain that distinction rather than inventing a mechanism
or treating the commit message as the full vulnerability report.

Source snapshots checked:

- `chromium/8076`: `8ca5b735df4263f43c830479b78213854a392c22`.
- `chromium/8111`: `a50f1a5953264de698f398fe3a2ef6153677fafa`, also the first
  source revision in the prebuilt release's change list.
- Each snapshot's Gitiles log was checked as a continuous parent chain, including
  the September 18 fix. Matching ancestry and current source contents were both
  checked; version-number ordering alone was not used.

All six manifest-selected archives for **both** 8076 and 8111 were downloaded
and checked against their pinned SHA-256 digests. Their extracted `args.gn`
consistently sets `pdf_enable_v8 = false`, `pdf_enable_xfa = false`, and
`pdf_is_standalone = true`. All six 8111 VERSION files report 158.0.8111.0.
The [8111 fxjs build definition](https://pdfium.googlesource.com/pdfium/+/a50f1a5953264de698f398fe3a2ef6153677fafa/fxjs/BUILD.gn)
and its 8076 counterpart compile `cjs_app.cpp` and `cjs_runtime.cpp` only with V8
enabled. This excludes the identified JavaScript runtime path in these selected
builds. The form-fill teardown hardening also covers non-JavaScript callbacks,
so disabling V8 alone is not evidence that every aspect of CVE-2026-113212 is
unreachable. 8111 includes that hardening as well.

Disposition: the public bug-linked revision/configuration comparison is complete
for these two reports and these archive selections. Adopt 8111 with the identified
changes present; do not claim that the old pin contains all fixes, that arbitrary
PDFium builds are unaffected, or that the two CVEs cover all native-library risks.
Restricted report details and an exhaustive upstream standalone affected-version
statement remain unavailable publicly. Recheck if the reports open or upstream
publishes additional fixes; final distribution qualification remains required.

Initial relevance triage and candidate preparation began 2026-10-21. The
policy's two-business-day triage checkpoint is 2026-10-23 and the seven-calendar-day
candidate checkpoint is 2026-10-28. Candidate preparation is not release approval;
expedite publication once applicable qualification passes if affected-version
analysis confirms relevance. Sandboxing and successful denial tests do not prove
that a native-library vulnerability is fixed.

## Qualification and remaining gates

- Fetch-script unit tests: 13 passed.
- `cargo fmt --check`, `cargo check`, and the complete `cargo test` suite passed
  with the 8111 arm64 library, including supported-authoring-tool rendering,
  speaker-note extraction and IPC/lifecycle tests.
- Main-based release build (no #491 input patch), Developer ID signed app with
  bundled 158.0.8111.0: signed XPC PDF rendering/notes and unrelated-file/network/
  child denial gate passed. Local signing used no timestamp; this is not a
  notarized DMG or Store installation.
- Signed candidate GUI binary SHA-256:
  `40ab4d1f73e8924f60f2e23c1bea37c6674ec7c124c8cc3fa10bdce0736a0f9b`.
- Signed bundled libpdfium SHA-256:
  `5e179ff2a575ec8803d2b1093adc6da9b46e86a58dce2dc2cbb6c67d75ad6a4b`.
  Signing changes Mach-O bytes; these are package-file hashes, not archive digests.
- [Build Binaries on PR #505](https://github.com/koichiro/quick-presenter/actions/runs/39552904847):
  macOS, Ubuntu x64 and Windows x64 packaging jobs passed, along with the
  dependency audit. Ubuntu installed-package rendering/notes and sandbox
  file/network/child denial gates passed; packaged GUI smoke under Xvfb passed
  **70 checks, 0 failures**. Windows extracted-MSIX PDF smoke and AppContainer
  denial gates passed. macOS hosted CI is layout/ad-hoc validation, supplemented
  by the local Developer ID signed XPC gate above.
- Windows x64 development-native validation with an isolated 8111 DLL passed
  rendering of English/Japanese pages, notes and next preview. The complete
  serial Cargo suite passed, including all 14 renderer tests. The 8076
  comparison had one 1-second renderer handshake timeout; an isolated retry
  passed, so it is not described as a clean full-suite comparison. See
  [the #503 report](https://github.com/koichiro/quick-presenter/blob/codex/windows-notes-focus-479/docs/validation/ISSUE479_WINDOWS_20261021.md)
  and [Windows results on #505](https://github.com/koichiro/quick-presenter/pull/505).
  That native test used #503's source, not #505's changed manifest head. It does
  not qualify a signed Store distribution or establish CVE coverage.
- Remaining release gates include final signed/Store native qualification,
  other distributed architectures as applicable, and Linux Wayland UI/font
  migration qualification if that separate dependency migration is adopted.
  The current PDFium pin change does not alter the retained title-font stack.
- Public CVE-to-bug-linked revision and archive-configuration comparison: completed
  2026-10-22 as recorded above. Exhaustive restricted-report details are not
  available publicly; this is a bounded source/configuration assessment.

Merging #505 completes the #480 dependency-maintenance review with the chosen
pin update and explicit Cargo migration deferrals recorded above. The remaining
CVE impact/fix-inclusion assessment, final distribution qualification and release
warning review are tracked in [#508](https://github.com/koichiro/quick-presenter/issues/508)
for v2.5.0. Complete its applicable gates and dispositions before publication;
merging #505 does not close #508. Retained Cargo maintenance warnings still
require release review; this document grants no exception.

# Dependency maintenance review — 2026-10-21

Tracking: #480. Source baseline: `430cef0` (main, application 2.5.0).
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

The [2026-10-20 Chrome stable announcement](https://chromereleases.googleblog.com/search/label/Stable%20updates)
includes CVE-2026-113212 (PDF memory corruption) and CVE-2026-113022 (PDFium memory
corruption). They are relevant triage inputs for this native PDF renderer.
The public component labels do not establish whether the exact 8076 prebuilt
is affected or prove that 8111 contains the relevant fixes. Check the linked
upstream bugs/fix revisions and prebuilt configuration before making either
claim. The [PDFium security page](https://www.chromium.org/Home/chromium-security/pdfium-security/)
is policy/context rather than a complete release-specific CVE inventory.

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
- Windows x64 candidate validation: delegated to the Windows session handling
  #479; record its exact report/PR before closing #480.
- Linux packaged rendering, Wayland font/UI qualification, other supported
  architectures and release CI: pending.
- Public CVE-to-fix revision mapping and affected-build analysis: pending.

The manifest update is a candidate for review. Do not publish it or close #480
until the applicable package gates and dispositions are recorded. Retained Cargo
maintenance warnings require release review; this document grants no exception.

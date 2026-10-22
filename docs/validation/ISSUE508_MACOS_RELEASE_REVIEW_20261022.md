# Issue 508: v2.5.0 security and macOS artifact review

Date: 2026-10-22 (JST). Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508).
This is a bounded source/configuration assessment and a recheck of the existing
internal TestFlight candidate. It does not approve customer publication, waive
unperformed coverage, or qualify a future artifact automatically.

## Source and tested distribution

- Review baseline: main `de4dab138ccd11eefe9968d25d00dabbb7fe056b`.
- Installed Apple-distributed product: **2.5.0 / 25009**, arm64, macOS
  **27.0.1 (26A434)**, `/Applications/Quick Presenter.app`.
- Build source: `3ad3c03666879b0ee3cff133453fdec7a3fca32c`. The difference
  from that source to the review baseline consists only of `docs/CLI.md` and
  the [#433 qualification report](ISSUE433_STORE_CLI_TESTFLIGHT_25009.md).
  Application code, lockfile, vendor patches, entitlements and PDFium pin match.
- Selected pin: `chromium/8111`, PDFium **158.0.8111.0**; source revision
  `a50f1a5953264de698f398fe3a2ef6153677fafa`.
- No build upload, review submission, notarization or customer publication was
  performed in this recheck. The Store submission PKG and internal TestFlight
  installation are separate distribution identities.

## CVE review and v2.5.0 technical disposition

Re-read the official CVE records, their referenced October 20 Chrome notice,
PDFium security page, public bug-linked commits and the selected source snapshot.
The CVE record update timestamps remain October 20 for CVE-2026-113212 and
October 21 for CVE-2026-113022. No additional fix or affected-configuration detail
was identified in those reviewed sources since the earlier October 22 comparison.
The linked Chromium issue details could not be inspected through the available
public browsing interface; the earlier restriction/uncertainty remains unresolved.

| CVE | Fix inclusion and reachable application paths | Technical disposition and uncertainty |
| --- | --- | --- |
| [CVE-2026-113212](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/113xxx/CVE-2026-113212.json), bug 572237007 | The earlier continuous-parent-chain check established that [e764cc088a62](https://pdfium.googlesource.com/pdfium/+/e764cc088a62b5f0510a73bed4e19c88c9af50bc) and [ac3cf1684db0](https://pdfium.googlesource.com/pdfium/+/ac3cf1684db05bd68338acb84e3823e6feb72b5b) are ancestors of 8111 and absent from 8076. Re-reading 8111 confirms the runtime destruction guard and active/passive form-fill callback teardown checks remain. The selected build disables V8, excluding the identified CJS runtime path. PDF loading, page rendering, annotations and document teardown remain application inputs to native PDFium; `pdfium-render` 0.9.1 can initialize a form-fill environment for a loaded document, so lack of an editing UI does not establish non-JavaScript callback unreachability. | Retain 8111 with both publicly identified changes. Treat callback hardening as relevant rather than declaring the whole CVE unreachable. The public CVE describes memory corruption in Chrome PDF; neither Chrome's numeric affected range nor the bug-linked commits establish an exhaustive standalone-PDFium affectedness statement. |
| [CVE-2026-113022](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/113xxx/CVE-2026-113022.json), bug 494352583 | The earlier ancestry check found [48af8b610572](https://pdfium.googlesource.com/pdfium/+/48af8b61057256b50cfdb4eae55b9b050924b48f) in both 8076 and 8111. Re-reading 8111 confirms `OnKeyStrokeCommit` remains absent from the interactive form implementation and header. The identified on-load JavaScript path is excluded by the selected V8-disabled build. | Retain 8111 with the identified change. The commit describes a user-gesture bypass, while the CVE describes memory corruption; do not equate the two descriptions or claim this is an exhaustive fix inventory. |

The [earlier review](ISSUE480_DEPENDENCY_REVIEW_20261021.md) contains the source
ancestry method and all-six-archive configuration comparison. This recheck
reused that unchanged ancestry evidence and freshly verified retained source
contents and the macOS arm64 archive. It did not repeat all six downloads.
The [8111 fxjs build definition](https://pdfium.googlesource.com/pdfium/+/a50f1a5953264de698f398fe3a2ef6153677fafa/fxjs/BUILD.gn)
puts the identified CJS sources behind `pdf_enable_v8`. The verified archive has
`pdf_enable_v8 = false`, `pdf_enable_xfa = false`, `pdf_is_standalone = true`.
Disabling these features does not remove ordinary PDF parsing or form handling.

The bounded technical decision is to keep 8111 for v2.5.0 with the identified
changes present and these uncertainties recorded. No new public evidence was
found requiring a different candidate in this review. This is not a claim that
PDFium is vulnerability-free, that 8076 contains every fix, or that confinement
establishes fix coverage. Reopen the assessment if upstream discloses further
report details/fixes, or source, configuration or pin changes before release.

Primary context:
[Chrome security announcement](https://chromereleases.googleblog.com/2026/10/stable-channel-update-for-desktop_02018666648.html),
[PDFium security page](https://www.chromium.org/Home/chromium-security/pdfium-security/),
[prebuilt 8111 release](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium/8111).

## Artifact, provenance and inventory recheck

All of the following passed against the existing installed candidate:

- Deep/strict signature verification and the existing product validator for
  exact GUI/CLI/broker identifiers, consistent Team ID, App Sandbox/App Group
  entitlements, required Store-control symbols and broker agent layout.
  The App Store receipt is present. These do not turn TestFlight into a customer
  Store installation.
- Private CoreGraphics import scan: **7 Mach-O files**, no private CGS imports.
- Freshly downloaded `pdfium-mac-arm64.tgz` matches the manifest SHA-256
  `46c674298c60123f1195c2a03c65ee08c4783c84874ce4f5b3a054abb3ecaf30`.
- Both installed PDFium copies (GUI resources and renderer XPC resources) have
  file-backed Mach-O sections identical to that verified archive's library.
  Their VERSION, args.gn, root LICENSE and exact **18-file** component-license
  inventory match the archive byte-for-byte, with no extra/missing license files.
- All **7** installed Mach-O files have identical file-backed sections to the
  original signed submission app. This section comparison excludes Mach-O
  headers, load commands, linkedit metadata and signatures; Apple re-signing
  means whole-file hashes are deliberately recorded separately.
- Submission PKG SHA-256 remains
  `d33007412892ff0fd33ad65e75ee05571f23e6536eb2d372355f8fe9fa54e027`.
  `pkgutil --check-signature` validates its Apple-issued installer certificate
  chain. This is an App Store submission package, not a notarized direct DMG.
- The existing CycloneDX 1.5 sidecar was recomputed from the actual mounted
  submission-payload DMG, original source revision, matching Store-feature arm64
  Cargo BOM and pinned schemas; exact comparison passed, **496 components**.
  The verifier used the original build revision explicitly after confirming that
  the review baseline changes only documentation. This inventory describes the
  submission payload, not Apple's later distribution container.

Installed whole-file SHA-256 identities:

| Role | SHA-256 |
| --- | --- |
| GUI | `3f85688b603d7555764ed33468b2171ec7be6a97d936f9b2d5cc86d577f5acbd` |
| CLI | `714f1b46712566d04d8040828e6d537951845d2c8f018e398fd4b219907f13bf` |
| Broker | `9279020db3f3a81d8985647347a972df8365fd32b3c6073360cca6d36b574f2a` |
| Renderer proxy | `178868e9cb2c03e34ffce2ef03aa81a61d696d1bb05b387acfcb1ede36590777` |
| XPC renderer | `64b9606ccb1d8bfade86da364db115570c6d3f69c6c0a42d4a88402e669e60f4` |
| GUI PDFium | `2ef2b21500d926a430a7af7c20e0929edd2c6c92cfde8c004d2b42e38ef61f58` |
| XPC PDFium | `2010e73e803c714b24c24d024312fd5ff1e56e770840ccd8353568ed4b885e8a` |

## Installed runtime recheck

With PDFium overrides unset, an unrelated working directory and a dedicated
three-page PDF in the UI's test-only container directory:

- `check_macos_renderer.py` passed actual signed-XPC PDF rendering/notes and
  unrelated-file read/write, network and child-process denial gates.
- Installed `--gui-smoke` passed **177 checks, 0 failures**, including notes
  sizing/scrolling, asynchronous rendering, navigation, blackout, actual AppKit
  fullscreen reversal and hidden-window rendering recovery. The GUI run finished
  in 20.87 seconds and the renderer gate in 0.22 seconds. These are automated
  checks, not a fresh physical-key or cross-OS visual qualification.
- The dedicated smoke mode bypasses normal startup-settings persistence; no
  user PDF was modified. This run does not establish external-file authorization,
  genuinely revoked bookmark authority or a fresh-user environment.

The [#433 report](ISSUE433_STORE_CLI_TESTFLIGHT_25009.md) retains the earlier
installed picker/cancellation/deadline/reopen/moved-bookmark, identity rejection,
15-minute watch and concurrency evidence. Those results are supporting evidence,
not newly repeated tests in this recheck.

## Cargo maintenance review

Fresh `cargo audit --json` passed with **0 vulnerability advisories** and the
same **5 unmaintained warnings**. RustSec database revision:
`59ef7abf8ce56ea80fe7eba97ba3835449009fa4`, updated 2026-10-22.
`.cargo/audit.toml` remains `ignore = []`.

Rechecked `cargo tree --locked --all-features --target TARGET -i CRATE --edges
normal,build` for all five crates on macOS arm64, Windows x64, Linux x64 and
`--target all`; all 20 queries completed successfully.

| Warning | Current reachability and retained disposition |
| --- | --- |
| bincode 2.0.1 / RUSTSEC-2025-0141 | No active normal/build reverse path, including all targets and application features. Keep the optional feature inactive and defer removal to its owning upstream dependency. |
| ttf-parser 0.25.1 / RUSTSEC-2026-0192 | Active on Linux via winit → sctk-adwaita → ab_glyph → owned_ttf_parser; absent from queried macOS/Windows paths. Retain the compatible title-font stack pending upstream migration and Wayland qualification. UI font parsing is outside renderer isolation. |
| zbus_names 4.3.4 / RUSTSEC-2026-0371 | Active through Linux D-Bus/accessibility dependencies; absent from queried macOS/Windows paths. Coordinate migration with the owning D-Bus stack. |
| zvariant 5.15.0 / RUSTSEC-2026-0370 | Linux D-Bus/accessibility normal/build paths remain active. Retain the explicit upstream migration deferral. |
| zvariant_derive 5.15.0 / RUSTSEC-2026-0372 | Same owning Linux stack, including build-time macro usage. Migrate together with zvariant. |

The warnings have no patched versions in the current audit records. This release
review retains the documented maintenance deferrals, not an advisory ignore or
waiver of new vulnerabilities. Graph reachability does not prove binary absence
for arbitrary configurations. Slint still requires winit 0.30; the upstream blur
fix exists in 0.31 beta while the local fullscreen and backend patches still need
preservation. Removing vendor overrides is not part of this qualification.

## Remaining release gates

[#508](https://github.com/koichiro/quick-presenter/issues/508) remains open:

- Bind the final published source, channel artifacts, hashes and SBOMs; repeat
  affected checks if code, entitlements, pin, signing or packages change. Qualify
  the final Developer ID/notarized/stapled DMG if that channel ships. The Store
  payload DMG above is only a transport/inventory fixture.
- Complete customer Store-installed workflow when applicable. Internal TestFlight
  does not establish customer distribution or review approval.
- Retain Intel/other supported macOS, foreign-team/cross-user rejection and genuine
  bookmark-revocation tests or an explicit maintainer release disposition. No
  additional test Mac is available; none of these cases is claimed as passed.
- Coordinate real fresh-account registration/approval/denial, upgrade and complete
  uninstall/reinstall, and stale/unregistered-agent behavior with
  [#493](https://github.com/koichiro/quick-presenter/issues/493). No account was
  created and no installed application, broker registration or user settings were
  removed to simulate these cases.
- Complete remaining applicable Windows/Linux distribution and shipped-architecture
  qualification. The supplemental Windows run is now
  [complete](https://github.com/koichiro/quick-presenter/issues/508#issuecomment-6335506807);
  its unsigned/uninstalled-package and Store-identity limits remain explicit.
  Further Windows execution belongs in the Windows session.
- Review unresolved coverage/dispositions before publication. The initial triage
  began October 21; the next review is **October 23 or before release**, with the
  existing October 28 candidate checkpoint retained. This report creates no
  scheduled automation and grants no publication approval.

Issues #422 and #433 are closed with their remaining conditions transferred to #493/#508;
those closures do not complete this issue. Raw smoke logs and machine/account
material are kept outside Git; this report records results and artifact identities.

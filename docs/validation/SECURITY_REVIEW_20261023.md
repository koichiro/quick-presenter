# Security review — 2026-10-23

Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508).
Review baseline: main `477d81a3628bb9a2f90d59f6b7b88c76ae1a093a`.
Owner: release maintainer. Next review: **2026-10-26 or before publication,
whichever comes first**, and immediately on a new relevant disclosure or usable
PDFium release. Retain the existing **October 28** candidate checkpoint; this
review does not extend the security-response deadlines or schedule an automation.

## Decision and change from the October 22 assessment

The October 23 review is complete; final release qualification is not.

- Keep `chromium/8111` as the current candidate, not as blanket publication
  approval. It is still the latest published prebuilt release, with unchanged
  selected digests. No pin, binary, license or advisory-ignore changes were made.
- Record the new CISA PoC classification for CVE-2026-113022. The Chrome CNA's
  description/affected-version data and the identified fix are unchanged.
- Broaden the review beyond the two previously assessed CVEs. Public bug-linked
  changes for two additional PDFium CVEs are present in 8111; seven additional
  reports map to Chrome integration/Android code that Quick Presenter does not
  ship. These are public-code/configuration findings, not exhaustive upstream
  standalone-PDFium affectedness statements.
- **Before publication, resolve the four enabled-path post-pin fixes below.**
  They are absent from 8111. Their public commits do not establish CVE identities
  or a demonstrated Quick Presenter exploit, but ordinary parsing, drawing,
  image decoding and text extraction remain relevant. Latest prebuilt status
  and sandbox tests do not justify ignoring them.
- Do not roll back to 8076: it lacks the public bug-linked AGG fix for
  CVE-2026-110155 as well as the previously identified 113212 changes.
  This is also relevant to review of the published v2.0.0/8076 distribution;
  its existing release acceptance does not establish security-fix coverage.
  No published artifact was changed or withdrawn by this review.

## Inputs and inventory

Reviewed the official [Chrome release feed](https://chromereleases.googleblog.com/feeds/posts/default?alt=json&max-results=50),
its October PDF/PDFium notices, the linked official CVE records and public
PDFium/Chromium changes. The feed's newest desktop posts were the October 22
[early stable update](https://chromereleases.googleblog.com/2026/10/early-stable-update-for-desktop_02123641300.html)
and [beta update](https://chromereleases.googleblog.com/2026/10/chrome-beta-for-desktop-update_02146026170.html);
neither publishes an additional named PDF/PDFium security advisory. This does
not infer security coverage from their Chrome version numbers. The reviewed
October 6/13/20 notices also contain older reports outside the earlier two-CVE
assessment; they are included below rather than described as newly disclosed.
The feed window is not an exhaustive historical native-dependency CVE audit.

[Prebuilt latest](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium/8111)
remains **chromium/8111**, published 2026-10-19 UTC, PDFium **158.0.8111.0**,
source `a50f1a5953264de698f398fe3a2ef6153677fafa`. All **six** manifest archive
digests match the current release API metadata. This is a fresh metadata
comparison, not six repeated downloads. Archive extraction/configuration/license
and installed-payload evidence remain in the
[October 22 review](ISSUE508_MACOS_RELEASE_REVIEW_20261022.md).
`Cargo.toml`, `Cargo.lock`, `.cargo/audit.toml` and the PDFium manifest are
unchanged from tested build source `3ad3c03666879b0ee3cff133453fdec7a3fca32c`.

The selected standalone builds disable V8 and XFA. In the inspected source,
CJS runtime files are compiled under V8 and FormCalc under XFA; ordinary PDF
parsing, drawing, image decoding, form-fill lifecycle and text extraction are
not excluded by those flags. `pdf_use_agg` defaults to true in the pinned
[GN configuration](https://pdfium.googlesource.com/pdfium/+/a50f1a5953264de698f398fe3a2ef6153677fafa/pdfium.gni).
The app uses standalone PDFium through `pdfium-render`, not Chromium's browser
PDF viewer, MIME handler, WebPlugin client or Android activity/download code.

## CVE review

### Existing two reports

| Report | October 23 result |
| --- | --- |
| [CVE-2026-113212](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/113xxx/CVE-2026-113212.json), bug 572237007 | The complete retrieved record is unchanged from October 22. Public bug-linked fixes `e764cc088a62` and `ac3cf1684db0` remain ancestors of 8111. V8 disables the identified CJS runtime path, but does not establish that all form-fill callbacks are unreachable. Retain the earlier bounded assessment. |
| [CVE-2026-113022](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/113xxx/CVE-2026-113022.json), bug 494352583 | Record updated **2026-10-22 23:03:56 UTC / October 23 JST**. The CISA ADP SSVC `Exploitation` value changed from `none` to **`poc`**. CNA description/version/reference fields and CISA CVSS 8.8 are unchanged. This is PoC classification, not confirmation of exploitation in the wild or a newly disclosed fix. Public bug-linked `48af8b610572` remains in 8111, with `OnKeyStrokeCommit` absent. Its identified JS path is disabled in the selected build; the commit/CVE-description distinction and restricted-report uncertainty remain. |

The returned [CISA KEV catalog](https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json)
was version `2026.10.21`, released 2026-10-21 UTC; none of the eleven reviewed
PDF-related CVE IDs appeared in that response. This does not prove that no
exploitation exists or override the newer SSVC PoC classification. No exploit
sample was required or executed to establish the record change.

### Additional PDFium reports

| Report | Public fix/configuration comparison and disposition |
| --- | --- |
| [CVE-2026-110155](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/110xxx/CVE-2026-110155.json), bug 570167922 | Public [AGG initialization fix](https://pdfium.googlesource.com/pdfium/+/58e11305710a506ce3369581662eeb14440639cf) is an ancestor of 8111. Both corrected `m_cur_cell` sentinels remain in its source. The 8076 source still has both old sentinels, so that pin must not be described as containing this fix. AGG is not excluded by disabling V8/XFA; keep the newer pin and prioritize applicable release qualification. |
| [CVE-2026-95338](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/95xxx/CVE-2026-95338.json), bug 556535630 | Public [passive form-callback re-entry fix](https://pdfium.googlesource.com/pdfium/+/4a36628222bb647c23b4ef3b8f3fc64e93e1dd3b) is an ancestor of 8111. Callback-state tracking and the corresponding form entry-point guards remain. Non-JavaScript form handling is not categorically excluded. Retain 8111 with the identified change, without claiming exhaustive report coverage. |

Fresh Gitiles inspection followed **247 consecutive parent commits** from the
8111 source through all five identified PDFium fix revisions (the three from
the earlier two-CVE assessment and these two additions). Parent continuity and
retained source contents were checked; version ordering alone was not used.

### Chrome integration reports

The following official CVE records map by bug ID to public **Chromium** changes.
The identified implementations are outside standalone PDFium and outside Quick
Presenter's shipped source/dependency boundary. This is a bounded exclusion of
the observed paths, not a claim that any restricted report has no other aspect.

| CVE | Public Chromium change and component |
| --- | --- |
| [CVE-2026-106204](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/106xxx/CVE-2026-106204.json) | [ff834f798fe3](https://chromium.googlesource.com/chromium/src/+/ff834f798fe3b7649892f36a6a9cb14ad6f01672): Chrome `pdf/pdfium/pdfium_page.cc`, link-handle usage. This is Chrome's wrapper, not a file in the standalone PDFium repository. |
| [CVE-2026-106421](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/106xxx/CVE-2026-106421.json) | [4b49db22352c](https://chromium.googlesource.com/chromium/src/+/4b49db22352c860eed4aa69f35518503e0a76af1): Chrome `pdf/pdfium/pdfium_engine.cc`, destination-handle usage. |
| [CVE-2026-110136](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/110xxx/CVE-2026-110136.json) | [cb21df9dd4c5](https://chromium.googlesource.com/chromium/src/+/cb21df9dd4c5b550d530cd1f461731f0d1d25ad0): browser extension MIME-stream/navigation handling. The critical Chrome severity is not numerically transferred to standalone PDFium. |
| [CVE-2026-110147](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/110xxx/CVE-2026-110147.json) | [e0c230bb1860](https://chromium.googlesource.com/chromium/src/+/e0c230bb186080c81c9e6b36a08deea184d15c59): Chrome renderer `PdfViewWebPluginClient` selection handling. |
| [CVE-2026-113251](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/113xxx/CVE-2026-113251.json) | [0520a5aa3f76](https://chromium.googlesource.com/chromium/src/+/0520a5aa3f76496749fca4c1f55d9cedb12f6517): browser HTTP(S) response interception/PDF viewer loading. |
| [CVE-2026-113026](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/113xxx/CVE-2026-113026.json) | [f38fc7b20380](https://chromium.googlesource.com/chromium/src/+/f38fc7b20380f126bd293b2584f895b54d3773b3): Android navigation/download initiator handling. The CNA limits its described product path to Chrome on Android. |
| [CVE-2026-106328](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/106xxx/CVE-2026-106328.json) | [b868931d1055](https://chromium.googlesource.com/chromium/src/+/b868931d1055af3089b25c61f4742a317510dc44): Android external-intent/local-URI validation; also described by the CNA as Android-specific. |

No named PDF/PDFium advisory newer than the reviewed October 20 notice was
identified in the returned feed. That statement is narrower than an exhaustive
CVE inventory or a proof that the latest source contains no undisclosed issue.

## Relevant changes after the selected pin

The public PDFium main head inspected was
`43d55e429cf72c21e9a1e60475b50feefa16fda9`. Its consecutive history contains
**29 commits after the 8111 source**. These five changes warrant explicit triage:

| Change | Enabled-path assessment and current disposition |
| --- | --- |
| [0a42f6e64f28](https://pdfium.googlesource.com/pdfium/+/0a42f6e64f28603f3aee118a1c140e9bd0647b71): OpenJPEG allocation-failure propagation | JPEG2000 image decoding is an ordinary PDFium path. The pin still has the old `void opj_copy_image_header` and caller behavior. Requires update/backport or explicit evidence-based disposition before publication. |
| [8fdce8e1b14d](https://pdfium.googlesource.com/pdfium/+/8fdce8e1b14def5d34f217c5099936ddcebe8b81): saturating path dimensions/overflow checks | Drawing untrusted PDF paths remains enabled. The pin still has unchecked float-to-int conversions and the changed dimension checks. Requires update/backport or explicit evidence-based disposition before publication. |
| [4bd20bd51662](https://pdfium.googlesource.com/pdfium/+/4bd20bd51662ef0ffca72ccd6d1ac41999f2d009): parser read-failure handling | PDF parsing remains enabled. The pin still uses `CHECK(did_read)` where upstream now returns a regular failure. The exact failure preconditions need consideration; do not assert every PDF triggers it. Requires update/backport or explicit evidence-based disposition before publication. |
| [43d55e429cf7](https://pdfium.googlesource.com/pdfium/+/43d55e429cf72c21e9a1e60475b50feefa16fda9): linear-time repeated-space handling | Text extraction is used by the app's slide/context/export operations. The changed repeated erases can have quadratic work; bounded IPC output alone does not bound native text processing. Requires update/backport or explicit evidence-based disposition before publication. |
| [51665166104c](https://pdfium.googlesource.com/pdfium/+/51665166104c07e0a2656fd25a54a26535668880): FormCalc date-symbol bounds | The changed file is `fxjs/xfa/cfxjse_formcalc_context.cpp`; XFA/V8 are disabled in the selected archive profile. The identified code path is excluded for these builds. Reassess if those build flags change. |

These are publicly observable correctness/security-relevant fixes, not five
newly assigned CVEs or proof of exploitation. They are absent from 8111; the
latest published prebuilt has not yet incorporated them. Existing helper
confinement, deadlines and recovery are mitigations, not inclusion of these
upstream changes. Do not publish a claim that all relevant upstream fixes are
already present. Track an updated prebuilt or a reproducible backport/build
candidate and qualify applicable targets; no unreviewed native binary was
substituted in this documentation review.

## Cargo audit and maintenance disposition

Fresh `cargo audit --json`: **0 vulnerability advisories**, **5 unmaintained
warnings**. Database revision `640bf5a9f06e66d8721e8772766531fe77c8630d`,
updated 2026-10-22 UTC, 1349 records (the earlier review used 1348).
Audited `Cargo.lock` SHA-256:
`b81622fb863fd45e01b465a8c22354229c8883d808f7ac873305e236ff6173a5`.

Warnings remain bincode 2.0.1 / RUSTSEC-2025-0141, ttf-parser 0.25.1 /
RUSTSEC-2026-0192, zbus_names 4.3.4 / RUSTSEC-2026-0371, zvariant 5.15.0 /
RUSTSEC-2026-0370, and zvariant_derive 5.15.0 / RUSTSEC-2026-0372; no patched
versions are listed. `.cargo/audit.toml` remains `ignore = []`.
The graph/build inputs are unchanged, so the earlier target-specific reachability
review and upstream-migration deferrals remain applicable; the twenty graph
queries were not redundantly repeated. This is Cargo inventory, not a verdict
on native PDFium safety or a new release exception.

## Follow-up and validation boundary

Record the completed review in #508 and retain these outstanding gates:

1. Resolve the four enabled-path post-pin changes before publication. Recheck
   the prebuilt feed on the next checkpoint or earlier new disclosure/release;
   prepare and validate a usable candidate without waiting for the normal
   maintenance cadence. Retain the October 28 candidate checkpoint; notices
   predating this review do not gain a new disclosure date or an extended deadline.
2. Keep final artifact/architecture/channel, customer Store, identity/bookmark
   and initial-lifecycle traceability coverage in #508. #493 completed its
   observed TestFlight lifecycle scope; it does not supply a pristine initial
   machine trace or final customer distribution evidence.
3. Implement/validate #492 before binding the final v2.5.0 source and packages;
   reassess any security-relevant source/configuration changes at that point.
4. Link final evidence and unresolved decisions in release notes before
   publication. This review does not merge code, publish, withdraw a release,
   grant a waiver, or alter the historical v2.0.0 acceptance.

Validation performed: fresh Cargo audit; six release-asset metadata comparisons;
complete retrieved-record comparison for the prior CVEs; public bug-to-change
mapping for eleven PDF-related records; retained-source and continuous-parent
ancestry checks; and post-pin source/diff/build-condition inspection. Native
binary hashes and runtime tests were not rerun because this change is documents
only and no candidate binary/configuration was changed. Logs, raw feeds/CVE JSON,
account information and test outputs remain outside Git.

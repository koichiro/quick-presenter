# Pre-publication security review — 2026-10-24

Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508),
publication documentation [#523](https://github.com/koichiro/quick-presenter/pull/523).
Reviewed source: `80522547ca1e5b2d1a5799f04ab542cc22f13e62`, including merged
#524. Official inputs retrieved on **2026-10-24 at 10:36 UTC (19:36 JST)**.
This supersedes the October 23 current-information checkpoint, while retaining
its detailed fix mappings and historical findings.

## Result and release boundary

**No new security blocker was identified in this bounded refresh.** The existing
maintainer decision to retain PDFium 8111 with four identified enabled-path
omissions remains applicable; no additional risk exception is requested or
created. This completes the requested pre-publication information checkpoint,
not final package qualification, an exhaustive security audit or a guarantee
that the application has no vulnerabilities.

#523 can proceed through its normal CI/review/merge process from this security
assessment. After merge, freeze the combined final source and regenerate/verify
affected packages, signatures/notarization, exact-source SBOMs and runtime
evidence in #508. `docs/CLI.md` is packaged; older artifacts must retain their
actual identities. Customer Store qualification remains separate in #520.
No release, Store submission, native update or authentication-policy change was
performed. Excluded Mac authority/environment tests remain unperformed, not passed.

## Rust dependency audit

Fresh `cargo audit --json` and human-readable `cargo audit` both completed:

- **0 vulnerability advisories; no yanked-package warning.** All 617 lockfile
  dependencies were scanned without target filters or advisory ignores.
- The same **five unmaintained warnings** remain: bincode 2.0.1 /
  RUSTSEC-2025-0141; ttf-parser 0.25.1 / RUSTSEC-2026-0192;
  zbus_names 4.3.4 / RUSTSEC-2026-0371; zvariant 5.15.0 /
  RUSTSEC-2026-0370; zvariant_derive 5.15.0 / RUSTSEC-2026-0372.
  Existing upstream-migration deferrals remain; none is silently ignored.
- [RustSec database](https://github.com/RustSec/advisory-db/tree/21d909e4c8bd4e14c7aea58bf980c881a0053ab1):
  **1352 records**, revision `21d909e4c8bd4e14c7aea58bf980c881a0053ab1`,
  last updated 2026-10-24 08:30:13 UTC.
- Cargo.lock SHA-256:
  `0918bd018726e361bfd29c3665eae7e850a6348c63064814973bb19d0e180733`.
  `.cargo/audit.toml` retains `ignore = []`.

The lockfile change since the October 23 review is only clipboard-win
5.4.1 → **5.4.3** and its checksum. Retain the
[Windows impact/source review](V250_CLIPBOARD_WIN_REVIEW_20261024.md): the
checked HTML-fragment fix is included; the inspected application clipboard
route uses Unicode text. Its actual Windows execution/package results remain
separately attributed in #508. Cargo audit does not audit native PDFium.

## PDFium release and upstream changes

The freshly retrieved [official prebuilt release](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium/8111)
is still **chromium/8111**, PDFium **158.0.8111.0**, published October 19 UTC,
source `a50f1a5953264de698f398fe3a2ef6153677fafa`. All **six** selected manifest
archive digests match current release API metadata. This is metadata comparison,
not six repeated downloads; existing extracted VERSION/configuration/license
and package evidence retains its original scope. The manifest is unchanged.

Fresh direct Gitiles retrieval found [main at 7b3f6f93625e](https://pdfium.googlesource.com/pdfium/+/7b3f6f93625e6df97a7d9e677137a558df8648c4),
**30 commits after the pin**, one beyond the October 23 reviewed head. The new
commit adds only `testing/resources/pixel/bug_557320961.in` and its expected
image: a DeviceCMYK regression fixture, with no runtime/build change. It is
not an additional omitted production fix. The fetched parent is the previously
reviewed `43d55e429cf72c21e9a1e60475b50feefa16fda9`. A cached browser response
still showed that older head; the newer direct response was used here.

The four accepted omissions remain absent: OpenJPEG allocation-failure
propagation (`0a42f6e64f28`), drawing conversion/dimension overflow handling
(`8fdce8e1b14d`), parser read-failure handling (`4bd20bd51662`) and linear-time
repeated-space text processing (`43d55e429cf7`). See the
[October 23 fix analysis](SECURITY_REVIEW_20261023.md#relevant-changes-after-the-selected-pin)
and [maintainer disposition](https://github.com/koichiro/quick-presenter/issues/508#issuecomment-6358413430).
Ordinary parsing/drawing/images/text remain relevant despite disabled V8/XFA;
mitigations do not incorporate those fixes. There is no new prebuilt candidate
to substitute in this review. Do not roll back to 8076, which lacks the already
identified AGG fix, or claim that all upstream security-relevant fixes are included.

## CVE, exploitation and release-notice refresh

All **eleven** official CVE JSON records from the October 23 inventory were
retrieved and compared in full. Ten are unchanged. The
[CVE-2026-113022 record](https://github.com/CVEProject/cvelistV5/blob/main/cves/2026/113xxx/CVE-2026-113022.json)
updated on October 23 at 12:51:16 UTC: CISA ADP record/SSVC timestamps changed;
CNA affected-version/description/reference fields, CVSS and SSVC options did
not. **Exploitation remains `poc`**, not confirmed in-the-wild exploitation.
The identified fix/configuration assessment is unchanged.

The freshly retrieved [CISA KEV catalog](https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json)
is version **2026.10.23**, released October 23 at 19:30:18 UTC. None of the
eleven IDs appears. Absence is not proof of no exploitation and does not
cancel the PoC classification.

The official [Chrome release feed](https://chromereleases.googleblog.com/feeds/posts/default?alt=json&max-results=50)
contains five new posts relative to the prior saved response: desktop/Android
Dev, iOS Stable, Android early Stable and ChromeOS Beta updates. None introduces
a named PDF/PDFium security disclosure. This is a comparison of the retrieved
50-post window, not an exhaustive historical CVE search. The eleven-record
public-fix mapping remains as in the earlier review: identified fixes for four
PDFium CVEs are present in 8111; seven identified changes belong to unshipped
Chrome integration/Android code. Restricted-report uncertainty remains explicit.

## Application change boundary and follow-up

Compared with the October 23 baseline, the dependency correction above and
macOS document-delivery bridge were considered alongside the existing
qualification records. Native requests are bounded to one pending file URL,
retain/release the original security-scoped URL, reject modal/busy requests and
use the existing Rust open/renderer path. The reviewed diff does not change
renderer confinement, control authentication, entitlements or bookmark
permission enforcement. #524 changes smoke/layout observation, not native PDF
parsing policy. This is a focused change review, not a fresh adversarial audit
of every application path; excluded cross-identity/revocation tests were not run.

Preserve the **October 28 candidate checkpoint** and security-response deadlines.
Recheck on a new relevant advisory, exploitation update, usable PDFium build or
security-relevant source/configuration change; if publication is delayed beyond
October 26, refresh by then. This review schedules no background monitoring.
Final combined artifacts/SBOMs and release-evidence integration remain open in
#508. Raw feeds, CVE JSON and audit outputs stay outside Git. No Cargo build/test
rerun was needed for these documentation-only changes.

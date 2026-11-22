# RC1 security checkpoint — 2026-11-22

Application source: `3be48aba725b3a46f680eb80d154f40aa076f6ca`.
This is a bounded preparation checkpoint, not completed release security
qualification or approval to publish. No dependencies or native pins changed.

## Dependency audit

`scripts/audit_deps.sh` and `cargo audit --json` succeeded against RustSec
revision `55e1c3dfbd834237a05bbbf7b50ba3b5be27b662` (1,555 advisories;
database updated 2026-11-21). The lockfile SHA-256 is
`3db76d27164b42984c28a105fce3bbb026d11091b29d0f57ac2ee141cf8bdf46`.
No target filter or advisory ignore was added.

The audit reported zero vulnerability advisories and ten warning findings:

| Kind | Package | Advisory |
| --- | --- | --- |
| Unmaintained | bincode 2.0.1 | RUSTSEC-2025-0141 |
| Unmaintained | chrono 0.4.45 | RUSTSEC-2026-0543 |
| Unmaintained | pin-utils 0.1.0 | RUSTSEC-2026-0441 |
| Unmaintained | ttf-parser 0.25.1 | RUSTSEC-2026-0192 |
| Unmaintained | zbus_names 4.3.4 | RUSTSEC-2026-0371 |
| Unmaintained | zvariant 5.15.0 | RUSTSEC-2026-0370 |
| Unmaintained | zvariant_derive 5.15.0 | RUSTSEC-2026-0372 |
| Unsound | rustix 1.1.4 | RUSTSEC-2026-0486 |
| Unsound | tokio 1.53.2 | RUSTSEC-2026-0386 |
| Yanked | async-executor 1.14.0 | No advisory ID |

These are the same warning packages recorded during integration. They remain
visible and require release disposition. In particular, the rustix xattr
zero-length-buffer warning and Tokio AsyncFd ownership warning must not be
dismissed merely because cargo-audit classifies them as informational.
Initial source search found no direct application calls to the named xattr or
AsyncFd APIs; this does not establish absence of transitive reachability.
Current-host reverse dependencies include tempfile for rustix and the
HTTP/WebSocket stack for Tokio. Complete target-specific reachability review,
update affected dependencies, or record an explicit maintainer decision.
The advisory records are available in the
[reviewed RustSec database](https://github.com/RustSec/advisory-db/tree/55e1c3dfbd834237a05bbbf7b50ba3b5be27b662).

## PDFium metadata and outstanding review

The freshly retrieved
[prebuilt release feed](https://github.com/bblanchon/pdfium-binaries/releases)
still selected [chromium/8165](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium/8165)
as latest, published 2026-11-16 at 17:35 UTC. All six selected archive digests
match `scripts/pdfium_manifest.json`. This compares release metadata, not six
new downloads or final installed artifacts. Retain the
[8165 compatibility assessment](PDFIUM_8165_UPGRADE.md) within its original scope.

The fresh
[upstream range](https://pdfium.googlesource.com/pdfium/+log/refs/heads/chromium/8165..refs/heads/main)
contained 30 commits through `66ed17ef8fa00526ba3e2633361b6cc312321dc9`.
It includes text-layout float conversion, BGRA compositing, FreeType updates,
and optional-engine changes. Latest prebuilt status does not establish that
all relevant upstream fixes are included. Complete enabled-path and native
dependency relevance analysis under the
[PDF rendering security policy](../PDF_RENDERING_SECURITY.md).

The inspected [text-layout fix](https://pdfium.googlesource.com/pdfium/+/721a620a0a0a7c26cec2a6fd97ca3c73eec298a9)
replaces overflowing float-to-integer conversions in text extraction and is
relevant to enabled standalone functionality. Its presence after the pin is
an open retention/adoption decision, not evidence of a CVE or exploitation.
The inspected [BGRA change](https://pdfium.googlesource.com/pdfium/+/cb73733715a15f6889364b298ea173ec5efb2c23)
describes a bit-identical SIMD performance optimization; do not classify every
upstream change as a security fix. The inspected
[JS-platform callback guard](https://pdfium.googlesource.com/pdfium/+/66ed17ef8fa00526ba3e2633361b6cc312321dc9)
is described upstream as embedder defense in depth. Check actual enabled APIs
before attributing optional-engine findings to this application.

The retrieved official Chrome 50-post feed includes November 17/18 notices
naming CVE-2026-130976 and November 3 disclosures naming CVE-2026-121166,
CVE-2026-121113 and CVE-2026-121179. Their official CVE references identify the
same bug numbers as the following changes in the retrieved 500-commit history
of the selected 8165 branch:

| CVE / bug | Identified upstream change included in 8165 |
| --- | --- |
| CVE-2026-130976 / 574302978 | [Clear FormCalc value bindings during destruction](https://pdfium.googlesource.com/pdfium/+/c9a34bc1d8b93531cfaa0a0ab3c2c8519d6270bb) |
| CVE-2026-121166 / 566111382 | [Construct the global property array in one call](https://pdfium.googlesource.com/pdfium/+/cce5c5ed801f54cadb5b039d98ed43804994764c) |
| CVE-2026-121113 / 567106301 | [Copy JPEG input before malformed-data repair](https://pdfium.googlesource.com/pdfium/+/4150fb564e0d83109f33a06385d22bb13cc3cd97) |
| CVE-2026-121179 / 567936263 | [Handle reentrant form-field focus transitions](https://pdfium.googlesource.com/pdfium/+/0a21f66dd0c665dc4d6dbdafc150e93528cb0020) |

This establishes inclusion of those identified changes, rather than relying on
Chrome version ordering. It is not an exhaustive fix or affectedness inventory.
Complete the remaining native-dependency and enabled-path assessment before
closing the security gate. The current retrieved CISA KEV catalog is version
2026.11.20; catalog absence alone is not an exploitation assessment.

Sources: [official Chrome release feed](https://chromereleases.googleblog.com/feeds/posts/default?alt=json&max-results=50),
[CVE Program records](https://github.com/CVEProject/cvelistV5/tree/main/cves/2026),
[CISA KEV](https://www.cisa.gov/known-exploited-vulnerabilities-catalog), and
[PDFium security policy](https://www.chromium.org/Home/chromium-security/pdfium-security/).

Retain the source response bodies and raw audit output in local validation
artifacts. Refresh this checkpoint after a material new advisory, a changed
candidate, or a significant delay before publication. This record creates no
risk exception and makes no claim that the candidate is free of vulnerabilities.

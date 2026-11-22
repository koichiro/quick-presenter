# v3.0.0 RC1 direct-channel qualification — 2026-11-22

This is the qualification checkpoint for the selected direct-channel artifacts.
See [the GitHub prerelease](https://github.com/koichiro/quick-presenter/releases/tag/v3.0.0-rc.1)
for actual publication state; a dated record is not proof of publication.

## Frozen application source

The selected binary source is `3fd02c9a914b120a7eb4b7cd949ecc38e951c458`,
merged by [#558](https://github.com/koichiro/quick-presenter/pull/558) as main
`7fc45b74017c2263ff98d9724f3dd61a310201fc` with an identical tree.
The selected tag is `v3.0.0-rc.1`. Later documentation-only records do not move
that tag or relabel a package as having been built from their commit.

[Exact-source Build Binaries 44970626284](https://github.com/koichiro/quick-presenter/actions/runs/44970626284)
was a successful manual run on that exact source, not a PR merge alias. All
three OS jobs passed. Final-head CI, dependency audit and macOS regression also
passed. Local Rust validation on repair commit `10098e5` passed 601 application
tests (two opt-in ignored) and the other suites; Python passed 86 tests with one
platform skip. Subsequent changes to the selected source were Markdown only.
An initial local invocation specified a PDFium directory rather than its library
file and failed loading; the corrected complete run passed. That failure is not
relabeled as a passing result.

CodeRabbit formally reviewed the initial security document; its Windows dependency
attribution finding was corrected and independently verified by the bot. Later
bounded checks verified the historical-source and seven-warning corrections.
The default filter excludes Cargo.lock, so this is not a formal line-by-line
lockfile review. Audit, dependency comparison and all-target CI support that change.
The final wording correction and this record complete the documentation separately
from the frozen package source.

## Artifacts and installed results

| Artifact | SHA-256 | Qualification |
| --- | --- | --- |
| `QuickPresenter-3.0.0-rc.1-arm64.dmg` | `0e4bbe333d2d9042a9d84f23e636450af89618c26d9f91bfaf7deaf5e3f50e51` | Local release build from the frozen source; Developer ID signed, notarized, stapled and Gatekeeper accepted. Mounted payload and separately installed copy verified. |
| `quick-presenter_3.0.0_amd64.deb` | `5b2db6bebd8c307048ad5f056904e1af8dee8f5a2dc76f78851e4cff14d459b4` | Exact-source Ubuntu CI installed the DEB and passed sandbox, CLI, GUI and package/SBOM checks. Independent download/extraction matched all 70 payload files/symlinks against the SBOM. |

macOS actual installed-package checks ran on macOS 27.0.1 arm64: XPC PDF/notes
and private-file/network/child denial checks passed; CLI reported application
3.0.0, Protocol 1 and ten passing checks; GUI smoke passed 323 checks with zero
failures. Native file-picker opening of the public sample, English/Japanese
notes, keyboard page navigation, window restoration and visible PDF rendering
were also observed. The final DMG's SBOM was generated and independently
verified after notarization/stapling against the actual payload and source.
The earlier rehearsal DMG is not a public release artifact.

Ubuntu's installed-DEB Xvfb GUI report records 215 passes and zero failures.
Its DEB version is 3.0.0 and architecture amd64; the extracted payload contains
PDFium build 8165. Independent verification checked the source/lockfile binding,
whole-DEB hash, schema, references, every extracted file/symlink and ELF x86-64
identity. The local independent check was file inspection on macOS, not a new
Linux runtime test. Physical Linux desktop/display behavior is not established
by the Xvfb run.

Cargo/About/CLI/native numeric versions remain 3.0.0. Use the immutable tag,
source, artifact hashes and package-bound SBOMs to distinguish this RC from a
later final release. Sidecars retain their explicit incomplete native-inventory
scope; bundled license evidence is not a complete native component/version scan.
Corresponding source and SHA256SUMS accompany the direct-channel artifacts.

## Owned relay interoperability

The final installed DMG imported an operator-provided configuration without
starting a connection. Explicit start connected; Chrome 158.0.8127.37 sent a
reaction and comment which were observed in the application, and page 0 advanced
to a rendered slide. Normal stop completed and the old participant URL returned
`room_ended`. An independent owner-status refresh retained the original service
use deadline. The minimal live connection did not include service deployment,
clock changes, load tests or customer-data recovery.

In a controlled test environment, only the dedicated validation process was
forcibly terminated. Restart with the same private profile and explicit resume
retained the same observed join URL, durable binding and creation key. The old
Chrome tab reloaded and a new comment reached the application. Safari 27.0.1
joined and sent a reaction observed in the application. Normal stop again denied
the old URL and cleared the durable may-exist/end-intent flags. Recovery files
were mode 0600. The owner Account clock for this crash run was not independently
refreshed; do not claim an additional authoritative clock measurement from it.
Operator identifiers, credentials, configurations and participant URLs remain
in private local records, not this report or the source archive.

## RC-only decisions and remaining coverage

The maintainer selected Developer ID DMG and Ubuntu DEB first. Mac/Windows Store
submission, certification and installed-Store checks remain subsequent work;
raw CI Windows inputs and unsigned macOS layouts are not public installer substitutes.
Previous Windows/iPhone observations retain their pre-repair candidate identity
and are not rebadged as final-package passes.

The maintainer explicitly accepted the remaining seven unmaintained warnings and
PDFium 8165 for RC1 only, with fixes/follow-up in
[#559](https://github.com/koichiro/quick-presenter/issues/559). The audit reports
zero vulnerability advisories, with no new ignores; two unsound and one yanked
findings were repaired. Relevant post-pin text/font fixes remain absent from
8165. Confinement, deadlines and recovery do not repair native undefined behavior
or prove that hostile PDFs are safe. Refresh the risk decision for the final release.

Additional Mac/Intel hardware, physical multi-display combinations, complete
screen-reader routes, all language/first-time routes, private browsing, Android
and final-package mobile/network combinations are not fully qualified here.
Automated assertions and developer-candidate observations do not replace those
unperformed manual checks. This is a scoped prerelease qualification, not a
full hardware/accessibility matrix, Store release or general-availability claim.

# RC1 final dependency and native-security disposition — 2026-11-22

Status: three actionable dependency findings are repaired; native-pin and
maintenance-warning disposition remains pending maintainer acceptance.
This document does not authorize publication or claim absence of vulnerabilities.

## Dependency repairs

The release lockfile updates rustix 1.1.4 to 1.1.6, Tokio 1.53.2 to 1.53.3,
and the yanked async-executor 1.14.0 to 1.14.1. The first two are patched
versions named by [RUSTSEC-2026-0486](https://rustsec.org/advisories/RUSTSEC-2026-0486.html)
and [RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html).
The older rustix 0.38 series is outside the affected range. No advisory ignore,
target filter, source exclusion or lowered test threshold is introduced.
The current audit reports zero vulnerability advisories and seven unmaintained
warnings; the two unsound and one yanked findings are no longer reported.

The change is a lockfile patch update. Resolver consolidation changes arboard's
windows-sys dependency from 0.60.2 to 0.52.0 and removes the unused 0.60/0.53
support packages. Tokio retains windows-sys 0.61.2 in both lockfiles. This is not
a Windows feature-policy change.
All supported-target package checks and application recovery regression must pass
on this new candidate. Earlier runtime/package evidence retains its old source.

## Remaining maintenance warnings

| Package | Advisory | Assessment and follow-up |
| --- | --- | --- |
| bincode 2.0.1 | RUSTSEC-2025-0141 | Present in the lockfile; not selected by the current default-feature all-target reverse tree. Do not claim it is absent from every optional build. Revisit with dependency/feature changes. |
| chrono 0.4.45 | RUSTSEC-2026-0543 | Direct date/time dependency. Migration to jiff/time changes behavior and needs separate compatibility work. |
| pin-utils 0.1.0 | RUSTSEC-2026-0441 | Reached through Slint spin_on/build tooling; replace through a compatible upstream dependency update. |
| ttf-parser 0.25.1 | RUSTSEC-2026-0192 | Font parsing through owned_ttf_parser/ab_glyph. The maintained alternative requires upstream font-stack migration; continue tracking font-related advisories. |
| zbus_names 4.3.4 | RUSTSEC-2026-0371 | Linux D-Bus/desktop/accessibility dependency. Its replacement is the zbus 6 names module, not a patch update. |
| zvariant 5.15.0 | RUSTSEC-2026-0370 | Linux D-Bus wire-format dependency; migration is coordinated with the zbus 6 ecosystem. |
| zvariant_derive 5.15.0 | RUSTSEC-2026-0372 | Derive tooling for the same Linux dependency graph; track with zbus migration. |

These advisories report maintenance status, not proof of a reachable exploit or
proof of safety. Proposed RC-only disposition: retain the existing compatible
versions, keep warnings visible, track replacement work and refresh before the
final release. No old v2.5 risk decision is carried forward automatically.

## Current PDFium review

The current official prebuilt release remains chromium/8165 (November 16, 2026
UTC); the application pin is unchanged. The earlier checkpoint maps four
published CVE fixes to commits included in the selected branch. That is bounded
evidence, not an exhaustive affectedness inventory.

Review of the 30 post-pin PDFium commits and 16 FreeType commits identifies
relevant fixes that are not in the selected prebuilt package:

- [PDFium 721a620a](https://pdfium.googlesource.com/pdfium/+/721a620a0a0a7c26cec2a6fd97ca3c73eec298a9)
  fixes float-to-integer overflow with extreme page dimensions/text scaling in
  text extraction. Application slide-text extraction is an enabled path; do not
  classify the fix as irrelevant merely because ordinary fixtures render.
- [FreeType 79d80cad](https://chromium.googlesource.com/chromium/src/third_party/freetype2.git/+/79d80cadfc3d)
  prevents attacker-controlled blend-count arithmetic from bypassing a stack
  check and causing a very long loop. Font processing is an enabled path.
- [FreeType e2526d30](https://chromium.googlesource.com/chromium/src/third_party/freetype2.git/+/e2526d30e673)
  fixes VARC axis arithmetic overflow; [0eca44e6](https://chromium.googlesource.com/chromium/src/third_party/freetype2.git/+/0eca44e6e819)
  avoids a null variation-metadata dereference. Reachability is not excluded.
- The incremental-glyph hdmx bounds fix and remaining font rolls are retained in
  the review scope; no blanket unaffected claim is made. BGRA SIMD/test/tooling
  changes are not all security fixes by implication.

The latest compatible prebuilt does not yet contain these fixes. Proposed RC-only
retention requires an explicit maintainer decision, a tracked expedited review of
the next fixed prebuilt, and package-specific renderer isolation/deadline/recovery
validation. Process deadlines constrain hangs and crashes; they do not repair
native undefined behavior or establish that hostile PDFs are safe. Do not
publish a stronger security claim or treat this proposal as accepted.

## Publication boundary

The maintainer selected notarized Developer ID DMG and Ubuntu DEB for RC1;
Mac/Windows Store distribution and installed-Store qualification are subsequent
work. Stable remains the latest release. RC1 remains unpublished until the
applicable final artifacts, package-bound SBOMs, runtime results and explicit
remaining-risk/scope decisions are recorded. Store, unexecuted hardware,
accessibility, language and private-browser checks are never relabelled passed.

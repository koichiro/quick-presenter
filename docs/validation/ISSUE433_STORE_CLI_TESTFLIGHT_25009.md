# Issue 433: Store CLI qualification — TestFlight 25009

Date: 2026-10-22 (JST). Tracking: #433; parent #422. Final release security
and artifact dispositions remain under #508, with the environment lifecycle
matrix under #493. Results below identify the tested artifacts; unavailable
coverage is not reported as passed.

## Candidate and installed identity

- Source: main `3ad3c03666879b0ee3cff133453fdec7a3fca32c`, including #502's
  symlink startup fix and #503–#505.
- Version/build: **2.5.0 / 25009**; `macos-store-control`, arm64.
- Host: macOS 27.0.1 (26A434), Xcode 27.1 (27A9275), SDK macOS 27.0.
- PDFium: **158.0.8111.0**, manifest pin `chromium/8111`.
- The app and nested roles require macOS 14.4. The test host does not qualify
  every supported macOS version or Intel hardware.
- Submission PKG SHA-256:
  `d33007412892ff0fd33ad65e75ee05571f23e6536eb2d372355f8fe9fa54e027`.
- Transporter delivered the package at **2026-10-22 16:10 JST**. Apple processing
  completed, and the build was assigned only to the existing one-tester internal
  `Release Validation` group, with maintainer authorization. No customer review
  submission or publication was performed.
- The export questionnaire retained the earlier standard-algorithm classification
  and existing no-France distribution declaration, rather than claiming that
  bundled PDFium contains no cryptography.
- TestFlight updated `/Applications/Quick Presenter.app` from build 25008 to
  **25009**. The installed app has a receipt; GUI, CLI and broker pass deep/strict
  signature and exact identity, sandbox/App Group, symbol and agent-layout checks.
  Their chain is TestFlight Beta Distribution → Apple Worldwide Developer
  Relations → Apple Root CA, Team ID `NP5Q6U6NT5`.

Apple re-signing changes bytes. These installed hashes are separate from the
submission PKG and original prebuilt archive digests:

| Installed file | SHA-256 |
| --- | --- |
| GUI executable | `3f85688b603d7555764ed33468b2171ec7be6a97d936f9b2d5cc86d577f5acbd` |
| ControlCLI executable | `714f1b46712566d04d8040828e6d537951845d2c8f018e398fd4b219907f13bf` |
| ControlBroker executable | `9279020db3f3a81d8985647347a972df8365fd32b3c6073360cca6d36b574f2a` |
| Bundled libpdfium.dylib | `2ef2b21500d926a430a7af7c20e0929edd2c6c92cfde8c004d2b42e38ef61f58` |

## Packaging and source validation

Release Store-feature binaries, private CoreGraphics import scan, nested
signatures, provisioning/profile checks and installer signature verification
passed. The matching Store-feature arm64 Cargo SBOM and CycloneDX 1.5 inventory
for the exact signed submission app payload were generated and verified; the
combined inventory has 496 components. It includes PDFium metadata, the complete
bundled license inventory and payload hashes. This payload inventory does not
claim to describe Apple's later distribution container byte-for-byte.

`cargo fmt --check`, default and Store-feature `cargo check --locked` and full
`cargo test --locked` passed on the recorded source. The existing atomic
`fetch_update` deprecation warning is unrelated. The documentation follow-up
clarifies distinct DMG/Store CLI invocation and user-local symlink targets;
`python3 -m unittest tests/test_cli_packaging.py` passed all six tests.
The documentation edit is not part of the already submitted build-25009 payload.

## Installed runtime results

| Area | Result |
| --- | --- |
| Cold startup through a user-local symlink | With the GUI absent (`NOT_RUNNING`, exit 3), a symlink to the installed nested CLI started build 25009 and presented one picker for a never-authorized PDF with a Japanese/space filename. Selecting the requested PDF returned success and page 1 of 3. No manual broker bootstrap was used after the update. |
| Product integration | `check_macos_store_control.py --testflight` passed status, read-only timer/notes/slide/context/export, watch/navigation, goto/blackout, close, authorized reopen and 32 repeated connections. |
| Missing file, cancellation and contention | The picker disabled Open for a nonexistent path. A competing open returned `BUSY` (exit 8). Cancelling returned `CANCELLED` (exit 10), retained the existing document/page and cleared `opening`. |
| Picker deadline and retry | A new PDF left unselected reached the specific 120-second picker `TIMEOUT` (exit 9), dismissed the picker, retained the existing deck and cleared `opening`. Retrying and selecting that PDF succeeded. |
| Moved PDF | After moving the authorized test PDF and closing the deck, opening its old path resolved the saved bookmark to the new path and succeeded without another picker. |
| Identity rejection | Read-only raw-XPC probes attempted lookup and registration against the installed Apple-distributed broker. An ad-hoc binary with the CLI's identifier but no Team ID, and a Developer ID-signed same-team binary with a different identifier, were both rejected at both services. No endpoint was received; legitimate status still worked and document/page/blackout were unchanged. These are not foreign-team or cross-user tests. |
| Renderer isolation | The actual installed XPC renderer passed PDF rendering/notes and unrelated-file/network/child denial gates. The PDF was placed in the UI's own test-only container directory. An initial raw smoke invocation against an unauthorized repository path correctly failed with `EPERM`; it is not recorded as a renderer defect or a passed external-file authorization test. |
| GUI regression | Installed build 25009 passed **177 checks, 0 failures**, including AppKit fullscreen reversal, presentation state, notes, blackout, window restoration/show/hide and rendering synchronization. This app-driven regression does not claim physical-key or visual qualification on other systems. |
| Watch limits and concurrency | Four watchers were accepted and a fifth returned `BUSY`; disconnected slots recovered within the bounded observation period. 200 status queries using four concurrent callers succeeded. |
| Slow consumer | An undrained watch stdout remained alongside 300 page commands; status stayed responsive and the original page was restored. Maximum command time was 0.073 seconds. Sampled watch-CLI RSS was 13,392–15,904 KiB. The watcher was still running before explicit cleanup; this is bounded workload evidence, not a claim of infinite-duration memory bounds. |

## Current-main distribution regression

[Build Binaries 39738754613](https://github.com/koichiro/quick-presenter/actions/runs/39738754613)
completed successfully on source `3ad3c03666879b0ee3cff133453fdec7a3fca32c`:

- Windows x64: default CLI/protocol tests, MSI and direct/Store MSIX packaging,
  installed MSI and Store-identity MSIX renderer sandbox checks, staged PDF smoke,
  denial gates and SBOM generation/verification passed. These are CI/package
  results, not a customer Microsoft Store installation.
- Ubuntu x64: installed Debian rendering/notes and sandbox denial gates,
  CLI/protocol tests and SBOM verification passed. Installed-package GUI smoke
  under Xvfb passed **70 checks, 0 failures**.
- macOS: bundle/DMG layout, default CLI/protocol, optional Store-feature compile
  and SBOM checks passed. Hosted GUI/signing skips remain explicit; the local
  Apple-distributed checks above supply independent runtime evidence.
- Dependency advisory audit passed under the existing policy. Retained warnings
  still require the #508 release disposition.

## Sustained installed watch

The installed product watch ran for **905.53 seconds**, receiving **303** valid
NDJSON events across page changes, document close and document replacement, with
periodic successful status queries and **zero errors**. The same run verified
four-watcher admission/fifth-watcher BUSY, slot recovery after disconnection and
200 status queries from four concurrent callers. The slow-consumer experiment
above ran during this watch and normal idle periods followed it. This is a
15-minute workload with both activity and idle periods, not an unattended
multi-hour presentation or whole-process memory bound.

Supplemental Windows-session native validation remains separate from the fully
passed current-main Windows package CI above. Its full-suite final result is
tracked under #508; this report does not assert that pending run passed.

## Limits and final release disposition

The maintainer confirmed that no additional validation Mac is available.
Fresh-account registration/approval/denial, cross-user and foreign-team probes,
Intel/other-supported-macOS runtime, genuinely revoked security-scoped bookmarks,
complete uninstall/reinstall and customer Store installation are not established
by these checks. Ad-hoc identity rejection is not foreign-team coverage;
filesystem denial is not genuine bookmark revocation. Update from 25008 on this
account is not a fresh installation.

Coordinate the environment lifecycle matrix with #493 and retain the remaining
artifact/security/coverage decisions in #508 before customer publication. The
historical build-25007 cold-start timeout remains recorded in the earlier report;
later success does not prove every startup environment is unaffected. Retest if
code, entitlements, selected pin or final package changes. No advisory ignore,
release waiver or customer-release approval is granted by this report.

Raw smoke logs, account details, provisioning material and screenshots remain
outside Git; this document contains results and necessary artifact identifiers.

## Maintainer disposition

On 2026-10-22, after reviewing these results and confirming that no additional
validation Mac is available, the maintainer explicitly approved closing #433
for this package/TestFlight qualification while retaining the unperformed
acceptance coverage in #493/#508 before customer publication. #493 owns the
fresh-user, upgrade and uninstall/reinstall environment matrix. #508 retains
supported-OS/Intel, foreign-team/cross-user, genuine bookmark-revocation and
customer-Store coverage/disposition, together with final security and artifact
review. These items remain unperformed, not retroactively accepted as test
passes. Parent #422 remains open for overall release acceptance.

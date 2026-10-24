# v2.5.0 final Ubuntu package qualification — 2026-10-24

Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508).
Companion identity record: [final artifact inventory](V250_FINAL_ARTIFACT_INVENTORY_20261023.md).

## Status and scope

Current-machine Linux qualification is complete for the selected final Ubuntu
x64 package. Default-selection Wayland/XWayland GUI, installed CLI/rendering,
physical desktop/keyboard routes, launcher/Open With and package lifecycle checks
passed as recorded below. The maintainer instructed this run to proceed with
physical dual-display coverage **unperformed** and explicitly excluded **Orca**
from this release gate after inconclusive audible results. Neither is a test pass.

The supplemental forced-FemtoVG failures below remain documented and are not
cleared by successful default-selection retests. This is bounded current-machine
evidence, not every renderer/environment or a blanket publication approval.
Other platform/channel/security/authority conditions remain in #508. No
application source, dependency pin, advisory ignore, package or signing identity
was changed. No release was published.

- Tester: maintainer (physical desktop/audio confirmation), Codex (artifact and runtime checks).
- Test date: **2026-10-24 JST**.
- Distribution: selected final CI Debian package for the Ubuntu direct channel; not a new publication.
- Trust: exact package SHA-256 and bundled/source/license/SBOM verification; no separate Debian signing identity is asserted.
- Manual report filename: `quick-presenter-2.5.0-ubuntu-x64-manual-gui-smoke.md`, preserved locally for the eventual release attachment.

## Exact subject

- Release/application: **2.5.0**, Control Protocol **1**.
- Source: `ce42e1de7754a02530fa85a4187b04222ec2585d`.
- [CI 39907971827](https://github.com/koichiro/quick-presenter/actions/runs/39907971827) passed on the exact source.
- [Build Binaries 39908348895](https://github.com/koichiro/quick-presenter/actions/runs/39908348895): dependency audit and all three platform jobs succeeded.
- Ubuntu artifact **12481852922**, `quick-presenter-ubuntu-x64`.
- Artifact ZIP SHA-256: `021db4cf071383f0ad36899f75a1de7ffaeab3708c07bde520355616a3e459d3`, matching GitHub metadata.
- Final package: `quick-presenter_2.5.0_amd64.deb`.
- DEB SHA-256: `5ed07c1ef3b2a902d3f0717659c5b61f9de75c56d5055950be043fcb6cfb3230`, matching the selected inventory.
- Supplied CycloneDX sidecar SHA-256: `298072c2f4762a434205fdee1cc9c7396b11dc802623f176b2ff227ea84ac1de`.
- Target: **x86_64-unknown-linux-gnu / amd64**, default features. The workflow ships one Linux application architecture; the six PDFium archive pins do not establish additional shipped application targets.
- PDFium: **chromium/8111 / 158.0.8111.0**, V8/XFA disabled.
- Official Linux x64 PDFium archive SHA-256: `bdc60126399f39fe768566a18d4be055a7b3681bc9e248e8a6656b4f6cae0aa2`.

## Environment and installation

Ubuntu **26.04.1 LTS x86_64**, GNOME Wayland; DELL U4021QW,
**2560 × 1080**, approximately **59.978 Hz**, scale **1.0**; one physical display.
Native Wayland and XWayland were exercised in this session. XWayland is not a
separate native Xorg session. Other hardware, architectures, desktop environments,
display configurations and OS versions are not inferred from these results.

The maintainer installed the exact DEB with `apt-get install`, upgrading the
existing **1.0.0** installation to **2.5.0**. `dpkg-query` confirms
`install ok installed 2.5.0 amd64`; `dpkg --verify` reports no differences.
Every installed regular file and executable link matches the selected DEB's
**70-file/link** payload. Both `/usr/bin/quick-presenter` and `/usr/bin/qp`
resolve to the installed companion executables under `/usr/lib/quick-presenter`.

Apt's `_apt` local-file readability notice was informational: it read the
home-directory DEB as root and completed the upgrade. No file permission or
trust change was needed. The agent execution environment retains
`no-new-privileges` even for elevated commands, so system package operations
were performed by the maintainer in a normal terminal.

## Package, PDFium and SBOM checks

- DEB filename, Version **2.5.0**, Architecture **amd64**, runtime dependencies,
  CLI documentation, application license/source offer, desktop entry and hicolor
  icons passed inspection. Package files/directories have no group/other write
  permission; executable and desktop-file modes are appropriate.
- `desktop-file-validate` passed. The system MIME cache lists
  `quick-presenter.desktop` for `application/pdf`; the default remains
  **org.gnome.Papers.desktop**.
- The freshly downloaded official PDFium archive matches its pinned digest.
  All **48** distributed PDFium regular files, including library, VERSION,
  build arguments, headers, root license and all **18** component licenses,
  match the packaged bytes. V8/XFA are disabled in the actual supplied build
  arguments. Application GPL/source-offer notices match the selected source.
- Supplied SBOM: **532 components**, **70 payload files/links**. All payload
  hashes/link targets, final package digest, source revision, target and lockfile
  digest match. Official pinned schema hashes, CycloneDX 1.5 schema validation
  and dependency-reference integrity passed.
- Independent locked Linux-target Cargo metadata matches **443 Rust library
  name/version pairs** and **1261 normal dependency edges** exactly. This
  verifies the declared target graph, not every linked machine-code component;
  the sidecar's native-inventory limitations remain applicable.
- Installed CLI metadata/invalid-request packaging contract: **10 passed**,
  application **2.5.0**, protocol **1**. The extracted candidate passed the
  same checks before installation.

## Runtime qualification

Unless a check specifically tests fail-fast CLI metadata, both PDFium override
variables were unset. Runtime checks use the installed executable outside the
source working directory and isolated configuration/state/IPC directories.

| Check | Result |
| --- | --- |
| Candidate CI uploaded installed-package GUI smoke | **70 passed / 0 failed**; matching GUI artifact ZIP digest `6c4ce791e40291cdb8f088ac9c284c4a0fdeb85b478a4f6f3914299e7556746f`. |
| Installed native Wayland GUI, short and long notes | **70/0** on each fixture. |
| Installed actual XWayland GUI, default selection, short and long notes | **70/0** on each fixture; both Wayland socket variables and SLINT_BACKEND unset. |
| Supplemental actual XWayland with explicitly selected FemtoVG | **69/1** on each fixture; preserve the native notes-resize failure below. |
| Extracted exact-DEB GUI before install | Short/long-note Wayland fixture runs, each **70/0**; nominal X11-labelled runs were also Wayland and are excluded as XWayland evidence. |
| PDF family smoke | Eight plaintext PDF fixtures rendered successfully: Marp, Beamer, Keynote, PowerPoint, Google Slides, long notes, mixed notes, generic comments. Password-protected input rejected. |
| Installed renderer confinement | Actual rendering/notes and unrelated-file read/write, network and child-creation denial passed. Outside-sandbox loopback control succeeded. The extracted candidate passed separately. |
| Installed live CLI | **62 calls / 18 watch events**, including actual CLI companion-GUI cold startup. |
| Cold startup precedence | **8/8** requested-deck cold starts with a different saved deck, using actual XWayland; normal WM close saved the selected deck each time. |
| Normal lifecycle recovery | Subsequent no-argument GUI restoration at page 1 and missing-remembered-file recovery passed; **33 CLI calls** across the eight-cycle and separate lifecycle phases. |

The live suite verified empty/English/Japanese notes; full slide/context;
both all-page export forms without page movement; forward/backward/goto and
boundary no-ops; elapsed timer reads and rejected timer mutations; blackout and
navigation; missing, invalid and password-protected open failures retaining the
active deck/page and later navigation; Japanese-and-space filenames; atomic
PDF hot reload; close/reopen; and a functioning typed event stream.

The GUI smoke exercises real presenter/audience windows, asynchronous rendering,
notes sizing/scroll calculations, navigation/fullscreen/blackout commands and
hidden-slide show/front lifecycle restoration. It does not establish physical
keyboard, compositor placement, screen-reader or multi-display acceptance.

## Supplemental XWayland smoke observations

With WAYLAND_DISPLAY and WAYLAND_SOCKET unset and SLINT_BACKEND=winit-femtovg,
both fixtures produced **69 passed / 1 failed**. The failure is the second
(with-PDF) native notes resize assertion: before **16px**, after **16px**,
reported viewport **289px**, physical window **800 × 650**. The before-PDF
resize assertion and all other checks passed. These are failed runs; a
physical-resize or test-timing cause has not yet been established.

A separate run with the packaged default renderer selection and actual XWayland
passed both notes resize assertions but stopped at **45 passed / 1 failed**:
local-control smoke returned CANCELLED, "Presentation changed during the query;
request the context again." External geometry observation recorded both
800 × 560 and 800 × 650 transitions. This is separate evidence, not a passing
complete suite or proof of a timing-only cause. After the maintainer completed
the physical matrix, subsequent default-selection XWayland runs completed
**70/0** with both fixtures. These final default-path passes do not prove the
cause of the earlier cancellation or clear the forced-FemtoVG resize failure.
Keep that supplemental condition explicit for follow-up; no binary fix or
blanket renderer/environment acceptance is claimed.

## Desktop and lifecycle acceptance

The maintainer explicitly reported **all requested checks 1–5 succeeded** on
the installed final app: initial slide/next-preview/notes display; resizing and
long-note scrolling; both-window Right/Left/Home/End, Space/Return, F5/F/Escape/B;
notes-focused navigation; one-display X explanation; toolbar Tab/Shift+Tab/Space;
picker cancellation and Japanese-and-space PDF selection, immediate Right and
Japanese notes; Open Recent; Window hide/show/front actions; Alt+Tab recovery;
and About 2.5.0. The final requested exit route was corrected from the nonexistent
Linux File > Quit to the presenter title-bar close button before confirmation.
The confirmation is basic requested-route acceptance, not every keyboard variant
or comprehensive accessibility conformance. The earlier manual GUI was closed
before the final package lifecycle run, and its startup-state record was saved.
The corrected lifecycle harness completed successfully, including executable
link/desktop/MIME removal, unchanged default PDF handler, reinstalling the same
DEB, version/CLI verification and restored desktop/MIME registration. This is
package removal/reinstallation with retained user configuration, not a pristine
user-profile test.

Eight independent actual-XWayland cold starts with a different remembered deck
selected the requested Beamer PDF **8/8**. Each normally processed WM close
saved the selected file. A subsequent no-argument normal GUI launch restored
that saved deck at page 1/windowed; a missing remembered file left no automatic
fallback deck and successfully opened a replacement. The separate normal
WM-close lifecycle phase exited **0**. These tests used isolated settings and
do not imply pristine-account or multi-display coverage.

The maintainer separately confirmed the final **OS integration** matrix:
Ubuntu launcher search/name/icon, Dock/Alt+Tab identity, actual file-manager
**Open With > Quick Presenter**, successful PDF loading and subsequent Right
navigation. The existing default remains **org.gnome.Papers.desktop**.

The maintainer observed that Open With starts another GUI process when one
already exists. This matches the existing
[documented multi-instance boundary](../CONTROL_PROTOCOL.md): additional
GUIs can play PDFs, while only one instance per Unix user/runtime directory owns
the control endpoint. `main.rs` logs control installation failure and continues
GUI playback. This was not treated as native document-delivery reuse or as a
new single-instance feature. Use in-app Open or `qp open` to replace the deck
through the controlling instance. No instance-routing source change was made.

The maintainer confirmed that a second physical display cannot be connected
and instructed this run to proceed with that coverage **unperformed**. Physical
dual-display swap/disconnect/reconnect/mixed-DPI remains **not tested**, not a
pass or a broader publication waiver. The one-display X explanation was part
of the successful physical keyboard matrix.

After the maintainer closed the remaining presenter, machine inspection found
**no Quick Presenter GUI/helper process**. Normal desktop startup/recent-file
settings were restored byte-for-byte from their pre-desktop-test backup. The
final 2.5.0 package remains installed. Orca settings/stop are recorded below.

## Dependency review and retained PDFium disposition

Fresh `scripts/audit_deps.sh` exited **0** using RustSec database
`84d943a6deeb8cc6c6f632531aaf4f1c62275239` (**1351 records**).
No vulnerability errors were reported. Five unmaintained warnings remain:
`bincode` 2.0.1, `ttf-parser` 0.25.1, `zbus_names` 4.3.4,
`zvariant` 5.15.0 and `zvariant_derive` 5.15.0. Existing
[October 23 review](SECURITY_REVIEW_20261023.md) migration dispositions remain
applicable; `.cargo/audit.toml` was not changed.

There is one additional warning relative to that checkpoint:
**clipboard-win 5.4.1 is now yanked**. The official crates.io index marks 5.4.1
yanked and 5.4.2/5.4.3 available; the upstream
[5.4.2 change](https://github.com/DoumanAsh/clipboard-win/commit/38c3c3fef175c20ed8f8e33dacb9b1e3bb8eca87)
replaces unchecked HTML-fragment copying with bounds-checked slicing and adds
an invalid-fragment test. The yank reason is not established by that commit
alone, and no CVE or application exploit is inferred. `clipboard-win` is absent
from the independently verified Linux normal graph and supplied Linux SBOM.
It therefore does not change this Linux payload; record the new warning for
Windows/release-wide review without silently accepting it or modifying the
fixed cross-platform candidate.

The maintainer's [retain-8111 disposition](https://github.com/koichiro/quick-presenter/issues/508#issuecomment-6358413430)
remains the final candidate's basis. The four identified enabled-path native
fixes remain absent. Runtime confinement is mitigation evidence, not their
inclusion or an exhaustive CVE assessment. Keep the October 26/pre-publication
security review and October 28 checkpoint; no publication was performed here.

## Evidence retention and limitations

Raw packages, screenshots, configuration, JSON and smoke/audit outputs are
preserved outside Git under `artifacts/issue508-linux-20261024/` in the original
Linux workspace, with SHA256SUMS. The repository report contains results only.
The final manual release report must accompany the unchanged DEB and sidecar
when the release is published.

Test setup corrections are not product passes: the initial nominal X11-labelled
GUI runs retained WAYLAND_DISPLAY and were actually Wayland. Corrected XWayland
runs explicitly unset both WAYLAND_DISPLAY and WAYLAND_SOCKET. The initial
isolated IPC path
exceeded the Unix socket length limit and was replaced with a short path;
the harness initially expected incorrect exit codes for INVALID_PAGE/OPEN_FAILED
and was corrected to the documented **5/10**. These attempts were excluded
from the successful 62-call suite. The unused long-path GUI was explicitly
terminated; that termination is not normal-Quit evidence.

No source changes required Cargo build/tests under the repository's
documentation-only rule. Existing exact-source CI/package results are linked
above. No other platform, customer Store, authority, traceability or
pre-publication gate in #508 is closed by this Linux report.

## Orca execution and explicit release disposition

Orca **50.2**, AT-SPI2 **2.60.4**, Speech Dispatcher **0.12.1** and the
espeak-ng output module were already installed. No additional system package
was required. Original screen-reader/toolkit-accessibility settings and Orca
preferences were saved before the explicitly requested validation.

With accessibility enabled and the final reinstalled app running on native
Wayland, AT-SPI exposed Open, First Page, Previous Page, Next Page, Last Page,
Fullscreen and Black as named buttons. The actual English note was retrieved
through AT-SPI Text as **Presenter note text**, separately from the page-one
**No notes** placeholder. This establishes semantic exposure, not intelligible
speech or comprehensive keyboard/screen-reader usability.

The maintainer heard speech but described it as unintelligible and could not
establish correspondence between speech and focus. The maintainer explicitly
instructed that **Orca be excluded from this release gate**. Record the audible
usability result as **not passed / waived for this release**, not as an Orca
success. No cause in the application, screen reader, voice or environment is
asserted. The existing [accessibility baseline issue #339](https://github.com/koichiro/quick-presenter/issues/339)
remains applicable; no broad accessibility-conformance claim follows.

After testing, the original screen-reader and toolkit-accessibility settings
were both restored to **false**, saved Orca preferences were restored, and the
session-managed orca.service became **inactive** with no Orca process remaining.
The installed Orca/Speech Dispatcher packages were retained unchanged.

The corrected package-lifecycle rerun completed with **PASS package lifecycle
completed**. Post-reinstall dpkg verification is clean, installed CLI checks
pass **10/10**, and the PDF default remains org.gnome.Papers.desktop. All **70**
payload files/links were independently rechecked against the fixed DEB after
reinstall. The final installed renderer denial gate was repeated and passed.
The initial local harness depended on rg, absent from the maintainer's terminal;
that attempt's removed-package MIME check was not counted. The corrected Python
check established removal and registration restoration. Raw logs remain private.

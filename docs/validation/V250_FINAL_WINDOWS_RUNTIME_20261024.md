# Final Windows runtime qualification — 2026-10-24

Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508).
Selected inputs: [final artifact inventory](V250_FINAL_ARTIFACT_INVENTORY_20261023.md).

## Result and fixed subject

The available Windows execution and requested maintainer manual checks passed
on the **selected final-source submission payload**, not a newly built or
Store-installed replacement. The customer Store installation/update/alias gate
remains pending the applicable distribution or explicit maintainer disposition.
This report does not complete Linux qualification or close #508.

- Source: `ce42e1de7754a02530fa85a4187b04222ec2585d`, application **2.5.0**,
  Control Protocol **1**, default features, `x86_64-pc-windows-msvc`.
- [Build Binaries 39908348895](https://github.com/koichiro/quick-presenter/actions/runs/39908348895),
  Windows [job 120266289733](https://github.com/koichiro/quick-presenter/actions/runs/39908348895/job/120266289733),
  Store x64 artifact **12482651689**. Run source and success were verified.
- ZIP SHA-256: `cba59393ac3dba9a2d9cf0b47b36d0c7635fd976005fc9f00f96ab378eff6f36`.
  It matches both the inventory and GitHub's artifact digest; this is archive
  integrity evidence, not independent artifact-attestation verification.
- Selected MSIX SHA-256:
  `4074a3f0e5a8ea074f5c21f7209752918fbba83e9e1ece6bc8307933c365b9c5`.
- Sidecar SHA-256:
  `53feff8cf1c01e3ae4b39c960a3c80104ad42569a667906c69a9217af4343876`.
- Identity: `KoichiroOhba.QuickPresenter`, publisher
  `CN=A3F64AFD-6298-42F8-BAC9-DB0AB1831F51`, **2.5.0.0 / x64**.
- Environment: Windows 11 Pro **10.0.26200 x64**. Later main
  `5b92bb9edf741e0915f759bcf2c841df4afb2388` differs from the fixed source only
  in release/validation documents. Its report changes do not reidentify the
  preserved package. Older run 39738754613 results were not reused as final
  runtime passes.

## Identity, PDFium and SBOM

The selected MSIX was extracted into a new validation directory. It has no
`AppxSignature.p7x` and Authenticode reports **NotSigned**. Its embedded GUI/CLI
have the self-signed CI test signer `CN=Quick Presenter`, thumbprint
`FC49EC0A629CF5DF08F2113BA4982A0781390124`; their local signature status was
Valid. Neither status establishes Microsoft Store customer signing or trust.
The manifest declares `qp.exe` as a console app execution alias; declaration
inspection is separate from actual installed alias registration/execution.

Fresh official win-x64 PDFium download passed the pinned archive SHA-256
`3452d11031050ec65e57f180215b57b9e6d465e3fbedafa8a77ad90885c7d23e`.
Each packaged DLL/VERSION/root LICENSE and the complete **18-file** component
license inventory matched the official bytes. VERSION is **158.0.8111.0**,
selected pin **chromium/8111**; V8/XFA-off configuration and the accepted
post-pin security disposition remain as recorded in the inventory/#508.
DLL SHA-256: `a318c39c137482dc4687c2973b7ec1c27fc1c20253cf2dc31928f922eab89e8f`.
Application license, source offer and CLI/protocol documentation matched their
exact build-source checkout. GUI/CLI/DLL PE headers are x64.

The supplied CycloneDX 1.5 sidecar passed pinned schema and reference checks,
the exact package hash, source/target/version, manifest identity, complete
payload inventory and every file hash: **415 components / 62 payload files**.
PDFium distribution URL/archive hash and all 18 embedded license evidence
records matched the selected inputs. **334 Rust library name/version pairs
and 861 normal dependency edges** exactly matched locked MSVC-target Cargo
metadata from the fixed source. This independently checks package/schema/graph;
it does not claim byte-identical regeneration from the unavailable intermediate
CI cargo-cyclonedx JSON or remove the documented incomplete native inventory.

Committed LF Cargo.lock SHA-256
`b81622fb863fd45e01b465a8c22354229c8883d808f7ac873305e236ff6173a5`
and manifest SHA-256
`c9dceacc564ff9a1c22f03de657eaaa3769766d5279ca2c70a369d1266ecc201`
match the fixed inventory. The Windows checkout hashes are respectively
`accb0a008af597086af3586b50bd689285eb9cfe0dcdbdb3ad0666b76e0f57eb`
and `80b3e6ea0e8e8519528bd8f803e6822ae49c21f8be79319372048d8f79269aa5`;
both were verified as exact LF-to-CRLF conversions, not dependency changes.

## Execution against the extracted final payload

Dedicated APPDATA/LOCALAPPDATA directories isolated the validation settings.
Both PDFium override variables were unset. Checks used the actual extracted
GUI/CLI from the package identified above.

- CLI package contract: **10 passed**, including version/protocol/help and
  typed rejection of unsupported timer mutation without requiring a GUI.
- Renderer: actual helper PDF open/render/notes plus unrelated-file read/write,
  inbound/outbound network and child-process denial gate passed. The external
  loopback control worked, avoiding ambient network failure as a false pass.
- GUI smoke: **63 passed / 0 failed** with the three-page English/Japanese
  notes fixture and independently **63 passed / 0 failed** with long notes.
  These exercise async rendering, notes fitting/scrolling, navigation,
  blackout/fullscreen state and hidden-slide rendering recovery.
- Real CLI cold start, content and watch: **20 calls / 7 NDJSON events** passed.
  Checked empty first-page notes, exact English/Japanese notes, full slide and
  context, all-page export, navigation, blackout and timer contract. File-backed
  output avoided inherited GUI output handles during cold-start capture.
- Warm failure/recovery: **15 calls** passed. Missing and invalid synthetic
  PDFs returned `OPEN_FAILED` and preserved the active document/page. Navigation
  still worked. Close returned an empty presentation and typed `NO_PRESENTATION`;
  reopening a Japanese-and-space filename restored rendering and exact notes.
- Normal Quit/relaunch: **15 calls** passed. Native Alt+F4 normally closed the
  GUI and removed its endpoint (`NOT_RUNNING`). Starting the exact GUI with no
  arguments restored the last Japanese-named PDF at page 1; next-page/English
  notes then worked and state remained Ready during five seconds of polling.
  The selected extracted executable path was verified in the running process.
  A second normal Quit left no validation windows. No forced termination was
  used for these normal-Quit claims.

The same final-source CI Windows job also passed its disposable-runner
installed-MSI and installed Store-identity MSIX identity/render/notes/denial
checks. Its MSIX install uses a separately test-signed copy. This is useful
installed-package CI evidence, not execution of the unsigned selected object
as a Microsoft-distributed customer installation.

## Maintainer physical keyboard, visual and Narrator confirmation

On October 24 the maintainer operated this Windows machine with the extracted
final GUI prepared at page 2/3. They explicitly reported that all requested
visual/physical-keyboard steps succeeded:

- Alt+Tab to the presenter showed the slide and note **before resizing**.
- Notes click: Right to 3/3 with Japanese notes; Left to 2/3;
  Home → Space → Return produced 1/3 → 2/3 → 3/3.
- Tab/Shift+Tab reached Open; Space opened the PDF picker; Escape returned.
- Audience-window Home, F fullscreen, Escape to windowed mode, B blackout and
  B restoration all worked.

With Windows Narrator enabled, the maintainer confirmed that toolbar names
(Open/page controls/Fullscreen/Black) and note content were audible, and that
keyboard operation and understandable focus remained available. They then
confirmed Narrator was stopped; its process was absent in the subsequent check.
This is the requested basic screen-reader/keyboard-route acceptance, not an
exhaustive accessibility-conformance assessment.

The earlier white-client **capture** observation is not reproduced by this
maintainer's unresized initial-display check on the final selected payload.
This supplies the missing actual-display evidence for this run. It does not
retroactively prove the cause of the older capture, claim a code fix, or
qualify other hardware/OS combinations. No Windows application defect requiring
a runtime source change was established by these checks.

## Remaining distribution boundaries and decisions

- The existing Microsoft Store installation remains **2.0.0.0**, package
  `KoichiroOhba.QuickPresenter_2.0.0.0_x64__pma9xfvs5bmmy`, unchanged.
  No package or certificate was installed, upgraded, removed or imported here;
  no trust setting or security guard was changed.
- Actual final **customer Store 2.5.0** install/update, Microsoft signing and
  package identity, registered execution alias, applicable installed rendering,
  CLI/lifecycle and denial checks still require the Store-distributed product
  or an explicit maintainer release disposition. The selected unsigned input
  cannot satisfy those assertions, and the disposable-runner-only script was
  not run or bypassed on this host. This report grants no release waiver.
- Only Windows x64 is the selected shipped Windows target in the fixed
  inventory. No other Windows architecture or supported-OS matrix pass is
  inferred. Requalify affected checks after code/pin/package/signing changes.
- No Linux distro was installed in WSL and no Docker/Linux runtime was
  available on this host. The selected final DEB was not executed here;
  Linux's condition remains separate. No environment was installed for this task.
- Customer publication, Store review submission and security-policy decisions
  are outside this execution. Keep #508's unrelated conditions, including
  the accepted PDFium 8111 residual-risk disposition and further review, intact.

Raw package/log/settings/JSON evidence is retained locally under
`artifacts/windows-v250-final-20261024/`, outside Git. Only this results report
and its release-checklist link are repository changes. No Rust/Slint/build
source changed, so Cargo checks/tests were not repeated for this documentation
update; the actual selected CI artifact and fresh runtime checks are identified
above. #508 remains open; this report does not close the other platform gates.

# v2.5.0 final macOS runtime qualification — 2026-10-23

Tracking: [#508](https://github.com/koichiro/quick-presenter/issues/508).
This follows the [final artifact inventory](V250_FINAL_ARTIFACT_INVENTORY_20261023.md).
Results apply to this Apple Silicon Mac running macOS 27.0.1, not every supported environment.

## Exact artifacts and distribution

Build source remains `ce42e1de7754a02530fa85a4187b04222ec2585d`, including #512; later #513 is documentation only. Version **2.5.0**, protocol **1**, PDFium **158.0.8111.0**, V8/XFA disabled. The previously recorded four post-pin risk dispositions remain unchanged.

- Direct: final notarized/stapled DMG SHA-256 `5f1ae4ee19299ad3aa136a9a73d814ec352624cb124ca13b7632aae0cc6898b0`. Installed with the production bundle identity into a user Applications folder. All **112** regular files match the mounted final DMG; deep/strict signature and Gatekeeper checks passed.
- Store: final build **25010**, PKG SHA-256 `64d8a3697f87afe5a3fa34f928c022370b34078c9de31bcd7d19d7bd66a171b1`. Transporter delivered the authorized exact PKG on October 23 at **16:03 JST**. Apple processing completed. The prior standard-algorithm/no-France export questionnaire classification was retained for the unchanged PDFium dependency. Only the existing one-tester internal **Release Validation** group was assigned; English test notes were saved.
- TestFlight updated `/Applications/Quick Presenter.app` from **25009 to 25010**. Installed version/build and receipt were checked. Deep/strict signature and product GUI/CLI/broker identity, shared team, sandbox/App Group, symbols and launch-agent layout checks passed. All **seven Mach-O files** match the submitted app in every inspected file-backed section. Apple re-signing changes whole-file bytes; this does not assert PKG/installed whole-file equality.

No customer publication or review submission occurred. Internal TestFlight update evidence is separate from a customer App Store installation, a pristine account, or an OS/hardware coverage claim.

## Executed results

| Area | Final direct installed app | Apple-distributed TestFlight 25010 |
| --- | --- | --- |
| Actual signed renderer | PDF render/notes and unrelated-file/network/child denial gate passed. | Same actual-XPC gate passed, using a test PDF in the GUI container. |
| App-owned GUI regression | **178 passed, 0 failed**. | **177 passed, 0 failed**, using container-local fixtures/report. These are the applicable channel suites, not a claim that both run an identical check list. |
| CLI/text/notes | **37 calls / 6 watch events** passed. | **30 calls / 6 watch events** passed; separate product XPC integration also passed queries, export, watch/navigation, goto/blackout, close/authorized reopen and **32 repeated connections**. |
| Notes/text details | Empty, English and Japanese notes; Japanese/space path; slide/context/full/export with active-page preservation; timer query and mutation rejection. | Same details passed. First CLI access to the new Japanese test PDF displayed the authorization panel; selecting the requested file succeeded and later authorized reopen did not require selection. |
| Missing/invalid file | Both failed opens retained the active deck and allowed subsequent navigation. | Temporarily moving an already-authorized test PDF, and separately replacing that test file with invalid bytes, each returned `OPEN_FAILED`, preserved the active document/page and allowed subsequent navigation. Original fixture bytes were restored. |
| Cold CLI | Started from not-running state; explicit requested PDF reached Ready; six seconds of stable status and exact executable identity checked. | Existing installed-product integration and normal relaunch below; a separate final-build cold CLI/clean-account run is not claimed here. |
| Native keys | Notes-focused Right advanced page 2 to 3 and displayed Japanese notes. | Native Right advanced 1 to 2; notes-focused Right advanced 2 to 3 with Japanese notes. |
| Finder Open With | Actual Finder Other selection of the exact final installed app passed warm 3-page and cold 2-page opening; cold explicit PDF took precedence over the previous remembered deck. PDF default remained Preview. | Not separately requalified through Finder in this run. |
| Lifecycle | Normal Quit, removal of only the test app to a rollback location, exact re-copy from the final DMG and relaunch restored the last Audience Drop PDF at 1/2. Re-copy preserved startup-state bytes and all 112 files matched. | Normal Quit/relaunch restored the last Japanese PDF at 1/3 and Ready without another selection panel. This is same-user relaunch, not Store removal/reinstall or pristine first launch. |

The direct cold test was run in a bounded detached process after an earlier shell-lifetime run was unsuitable for stability evidence. A first warm-Finder Right key observation did not advance the page and is not counted as a passed immediate-input test; later native document delivery/readiness and key operations passed. These observations do not establish a persistent product defect.

The initial attempt to reuse the direct negative-open harness for Store paths stopped at its **15-second harness timeout** while a never-authorized missing path awaited user selection. That attempt is not a passed missing-file test or an app crash. The authorization panel subsequently cleared; the separately executed saved-authority missing/invalid tests above establish the reported recovery result.

## Manual direct document-drop acceptance

The maintainer verified all four routes against the installed final production-identity direct app:

| Route | Expected and observed document |
| --- | --- |
| Finder PDF → installed application bundle icon | `Icon Drop.pdf`, **1 / 3** |
| PDF → running Dock icon | `Dock Drop.pdf`, **1 / 2** |
| PDF → presenter window | `Presenter Drop.pdf`, **1 / 2** |
| PDF → audience/slide window | `Audience Drop.pdf`, **1 / 2** |

These are installed-app results. The read-only-DMG icon-drop refusal from the paired candidate checks remains unresolved; installed-app success does not settle or waive it. Final Store manual drop coverage is tracked separately and is not inferred from the direct result.

## Preservation and remaining release gates

Only results are committed. Raw smoke logs, screenshots containing account information, private settings and packages stay outside Git. Exact packages and machine-readable runtime results are preserved under the ignored primary-workspace `artifacts/v250-final-20261023/` directory. Direct pre-test settings were backed up and restored after its tests; Store pre-update settings were backed up before qualification.

No Rust, Slint or build source changed; Cargo checks were not repeated for this documentation-only report. The unchanged source/build/CI checks remain in the preceding inventory.

Keep #508 open. Remaining conditions include read-only-DMG icon-drop disposition; customer Store qualification where applicable; remaining final Windows/Linux channel checks; supported-macOS/Intel coverage (no extra Mac available); foreign-team/cross-user rejection, genuine bookmark revocation and initial-first-attempt trace disposition; and the October 26/pre-publication security review and October 28 checkpoint. This report grants no new release exception or publication approval.

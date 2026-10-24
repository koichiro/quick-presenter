# Validation records

These records identify exact tested sources, packages, environments and limits.
They are maintainer evidence, not installation instructions or a claim that
all historical experiments qualify the current release. For user instructions,
start with [README](../../README.md) or [release usage](../RELEASE.md).

## v2.5.0 release status

- [#508](https://github.com/koichiro/quick-presenter/issues/508) owns remaining
  pre-publication qualification, artifact selection and release decisions.
- [#520](https://github.com/koichiro/quick-presenter/issues/520) owns customer
  Store installation qualification: approved Mac build/promo-code tests,
  Windows Package flights, then public listing/acquisition/update checks.
- Additional Intel/other-Mac/other-macOS environments were explicitly excluded
  from the v2.5.0 release gate; those tests remain unperformed. The maintainer
  accepted [#493](https://github.com/koichiro/quick-presenter/issues/493) as
  manually executed/completed. The maintainer also excluded Mac foreign-team /
  cross-user rejection and genuinely revoked-bookmark authority tests from this
  release gate. These checks remain unperformed, not passed; identity/permission
  enforcement and sandbox configuration are unchanged.
- The post-#521 **f1b5541** CI run
  [40069155697](https://github.com/koichiro/quick-presenter/actions/runs/40069155697)
  passed, but current-machine package checks found repeatable Windows GUI
  resize-convergence failures and an intermittent native-Wayland failure.
  This candidate is **not fully runtime-qualified**. Original failures and
  package identities remain in #508; do not replace them with prior passes.

- The subsequent [shared correction #524](https://github.com/koichiro/quick-presenter/pull/524),
  source **f596ef8**, adds an applied portrait/menu-layout witness. Linux's
  eight local release conditions passed **72/0 each**. Actual CI MSVC Windows
  Store payload regressions completed **65/0 in four runs** after an initial
  PDF-readiness timeout. The maintainer confirmed that parallel compilation in
  another session caused that timeout and accepted it as a non-blocking
  environment condition. The original timed-out run remains recorded as such;
  application/test deadlines were unchanged. CLI/render-denial and package/SBOM
  checks passed. Windows controls
  confirmed delayed application is awaited, suppressed application is rejected,
  and a valid same-size transition needs no additional resize request.
  [Build Binaries 40077854791](https://github.com/koichiro/quick-presenter/actions/runs/40077854791)
  and the actual review cover this source. Earlier f1b5541 failures and38279ce
  successes retain their separate scope. These correction results do not
  qualify the combined code/documentation final packages still to be generated
  after merge. Local software-renderer reporting limits remain explicit in
  #524; no exhaustive renderer pass is claimed.

## Artifact and runtime results

| Record | Exact scope |
| --- | --- |
| [Original final inventory](V250_FINAL_ARTIFACT_INVENTORY_20261023.md) | ce42e1d package/source/hash/SBOM identities, macOS notarization and Store submission inputs. |
| [Mac runtime](V250_FINAL_MACOS_RUNTIME_20261023.md) | Installed direct app and internal TestFlight 25010; not every customer/OS/authority condition. |
| [Mac document delivery](V250_MACOS_DOCUMENT_DELIVERY_20261023.md) | Installed Finder/drop paths and accepted install-before-DMG-icon-drop limitation. |
| [Windows runtime](V250_FINAL_WINDOWS_RUNTIME_20261024.md) | Historical ce42e1d extracted MSVC payload and maintainer desktop checks. |
| [Linux runtime](V250_FINAL_LINUX_RUNTIME_20261024.md) | Historical ce42e1d DEB, actual default Wayland/XWayland and package lifecycle; preserves supplemental failures. |
| [Clipboard dependency review](V250_CLIPBOARD_WIN_REVIEW_20261024.md) | Windows-only clipboard-win 5.4.3 correction and bounded reachability/audit assessment. |
| [Post-merge integration](V250_RELEASE_INTEGRATION_20261024.md) | b4910ea replacement Windows input, preserved Mac/Linux identity rechecks, bounded authentication probe and subsequent gate dispositions. |

A later source, signing, package or bundled-document change requires checking
its affected inventory and behavior. In particular, `docs/CLI.md` ships inside
packages: regenerate affected payload inventories/SBOMs after changing it.
Retain each artifact's actual build source rather than relabeling an old binary
as the latest main. Final candidates supersede older selections only through
an explicit recorded selection and qualification.

## Security and release decisions

The [October 23 security review](SECURITY_REVIEW_20261023.md) and
[#508](https://github.com/koichiro/quick-presenter/issues/508) record the PDFium
CVE/fix assessment and the maintainer's decision to retain **8111** for 2.5.0
with four identified post-pin omissions. That decision is not a claim of no
vulnerabilities. The existing five dependency-maintenance deferrals remain
recorded; the yanked clipboard-win version was replaced separately.

The [October 24 pre-publication review](SECURITY_REVIEW_20261024.md) completes the
fresh checkpoint on source `8052254`: zero Cargo vulnerability advisories, five
unchanged maintenance warnings, no new material CVE/exploitation change or
production PDFium fix. Existing risk dispositions remain applicable. Final
combined artifacts and evidence still require qualification after #523 merges.
Refresh on material new information, or by October 26 if publication is delayed;
retain the October 28 candidate checkpoint. Use the
[PDF processing security policy](../PDF_RENDERING_SECURITY.md) and
[SBOM gate](../SBOM.md#release-gate); this index does not waive them.

## Historical records

Issue-numbered Store IPC probes and TestFlight 25001–25009 reports document the
path to the product implementation. Diagnostic identities, test-only transport
experiments and superseded build numbers are not current installation recipes.
The [v2.0.0 Windows report](V200_WINDOWS_20261016.md) retains its results-only
summary and compact structured evidence. Failed and excluded tests remain
visible in their reports instead of being silently converted into passes.

Keep raw stdout/stderr, generated per-assertion reports, private settings,
clipboard contents, account details and package binaries in ignored local or
CI artifact storage. Commit concise results, provenance and release decisions;
retain reproducible fixture inputs and relevant structured inventories.

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
  manually executed/completed. These decisions do not pass separate
  foreign-team/cross-user or genuinely revoked-bookmark authority checks.
- The post-#521 **f1b5541** CI run
  [40069155697](https://github.com/koichiro/quick-presenter/actions/runs/40069155697)
  passed, but current-machine package checks found repeatable Windows GUI
  resize-convergence failures and an intermittent native-Wayland failure.
  This candidate is **not fully runtime-qualified**. Original failures and
  package identities remain in #508; do not replace them with prior passes.

- The subsequent [shared correction #524](https://github.com/koichiro/quick-presenter/pull/524),
  source **38279ce**, passed Linux's eight local release-mode conditions
  (**72/0 each**) and actual CI MSVC Windows Store payload regression
  (**65/0 in four runs**, short/long notes twice each), plus CLI/render-denial
  and package/SBOM checks. [Build Binaries 40074121830](https://github.com/koichiro/quick-presenter/actions/runs/40074121830)
  passed on that source. These correction results preserve the earlier f1b5541
  failures and remain separate from the combined code/documentation final
  packages to be generated after merge. Local Windows software-renderer
  reporting limits are recorded in #524; no exhaustive renderer pass is claimed.

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

Complete the next review on **October 26 or before publication, whichever is
earlier**, against the intended final source/artifacts. New material
vulnerability/exploitation/fix information requires triage and an updated
release decision. An unchanged assessment can retain the existing disposition.
Use the [PDF processing security policy](../PDF_RENDERING_SECURITY.md) and
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

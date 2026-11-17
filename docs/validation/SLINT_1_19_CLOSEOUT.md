# Slint 1.19.0 migration closeout

## Integration status

This record separates source integration from release certification. The
migration targets `v3.0.0` through
[PR #548](https://github.com/koichiro/quick-presenter/pull/548).
Slint and slint-build remain pinned to 1.19.0, Slint edition remains 1.18,
and the winit 0.30.13 override remains unchanged.

Closeout is in progress. The Windows source/report transfer, combined-source
validation, current-head review/CI, and merge must finish before this record
can declare source integration complete. Earlier green CI and desktop results
must not be relabeled as validation of a later combined source.

## Source and evidence inventory

| Source | Scope | Integration |
| --- | --- | --- |
| `23648de` | Slint 1.19.0, edition 1.18, native menu roles, orderly Quit and backend vendor removal | Already in migration branch |
| `3c376b8` | Backend patch audit and initial validation | Already in migration branch |
| `deee3b6` | Start GUI settling deadline after the first layout observation | Already in migration branch |
| `3aa6f49` | Mac step-4/step-5 results and OS execution assignments | Committed during closeout |
| Linux `fd67ef1`, integrated as `013a26a` | Persistent comment measurement, About readability and Linux reports | Integrated during closeout |
| Windows step-4/step-5 working-tree patches | Measured font-fit fixtures and work-area/DPI correction | Awaiting transfer |

The Linux transfer branch is `codex/slint-1-19-linux-validation`.
Raw logs, private settings backups, screenshots and disposable documents remain
in each host's ignored `artifacts/` storage. Repository reports identify their
actual local source and command scope; copying a report does not re-run it.

## Desktop validation completed before integration

- [Mac step 4](SLINT_1_19_MACOS_STEP4.md) and
  [Mac step 5](SLINT_1_19_MACOS_STEP5.md): native menu and keyboard operations,
  orderly Quit from both window roles, startup restoration, hidden-window
  reveal, notes, reload and fullscreen. The final step-5 GUI run passed 323
  checks. Original settings/history were restored and hash-verified.
- [Linux step 4](SLINT_1_19_LINUX_RESULTS.md),
  [Linux step 5](SLINT_1_19_LINUX_STEP5.md) and
  [About follow-up](SLINT_1_19_LINUX_ABOUT.md): 630 Rust tests passed, 2 ignored;
  tested Wayland/XWayland GUI configurations passed 215 checks each. Native
  launch tokens, recreation, menus, notes, reload, exit/restoration and
  normal/minimum-size About readability were exercised.
- Windows host report (awaiting repository transfer): the work-area fix passed
  fmt/check, 610 Rust tests (2 ignored) and 208 GUI checks. Native 125% DPI and
  process-configured 100% conditions are distinct. Initial fixed placement and
  expanded minimum height could exceed the taskbar-adjusted work area; the
  corrected normal window is fitted after native creation.

The two ignored tests are the explicit sustained-load exercise and the
external-relay fixture requiring a configured running relay. Their ignored
status is not a pass.

## Boundaries retained at source closeout

- All three desktops had one physical monitor. Real display exchange, mixed
  monitor DPI, hot-unplug/rearrangement and corresponding recovery were not
  performed. Logical scale events/process scale settings do not substitute.
- Linux XWayland results do not certify a native Xorg desktop session.
- The existing Linux Bring-to-Front action may leave keyboard focus on the
  presenter. Its unchanged Linux path shows an already visible window without
  transferring focus; the Wayland winit focus operation is also unsupported.
  Explicit OS switching/clicking was tested separately. This limitation is not
  represented as fixed by the Slint upgrade.
- This task closes a source migration, not a new public release. Existing
  package CI can validate build inputs, but signed/Store installation,
  notarization, sandboxed document delivery, Gatekeeper and distribution
  approval retain their separate qualification requirements.

## Final combined-source validation

Pending Windows integration. Required checks are `cargo fmt --check`,
`cargo check --all-targets --locked`, `cargo test --locked`, the existing
macOS Store-feature check, current-head CI/package checks and CodeRabbit
review. Record exact source and results here once they complete.

The intermediate Mac source at `013a26a` (Linux changes integrated, Windows
changes not yet transferred) passed fmt, all-target check, and the complete
default Rust suite: 636 passed, 0 failed, 2 ignored with pinned PDFium 8111.
The Store-feature library/binaries check passed, as did 27 SBOM/CLI packaging
Python tests. These are intermediate integration results, not the final
combined-source acceptance. Raw Rust output is stored locally in
`artifacts/slint-1-19-closeout/macos-test.log`.

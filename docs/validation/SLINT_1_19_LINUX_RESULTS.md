# Native Linux Slint 1.19.0 step-4 results

## Result and scope

Native automated Rust validation passed: **629 passed, 0 failed, 2 ignored**.
The explicit no-default-features configuration passed **54/54**; the requested
Python suites passed **40/40**. Full X11/Xvfb GUI smoke passed **215/215**.
The known pre-PDF notes-resize timeout did not recur at the execution HEAD.

The ambient Wayland full smoke failed **213 passed / 2 failed** on comment
font-width measurement. A focused native Wayland reproduction failed
**142 passed / 2 failed** at the same assertions. This remains unresolved;
its attribution to the migration is unconfirmed. Consequently, this is not an
all-platform GUI approval or a claim that the installed Ubuntu CI failure is closed.

## Host and execution identity

- Date: 2026-11-17, Asia/Tokyo (JST, UTC+09:00); command timestamps and elapsed
  times are in the raw logs. Initial preparation began about 14:38 JST.
- OS: Ubuntu 26.04.1 LTS (Resolute Raccoon), native Linux, no cross compilation.
- Kernel: `7.0.0-43-generic`, build `#43-Ubuntu SMP PREEMPT_DYNAMIC Fri Oct 23 13:25:25 UTC 2026`.
- CPU: Intel Core i5-8400T @ 1.70 GHz, 6 CPUs, x86_64.
- Rust: rustc 1.96.0 (`ac68faa20`, LLVM 22.1.2), Cargo 1.96.0
  (`30a34c682`), rustfmt installed; target `x86_64-unknown-linux-gnu`.
- Python 3.14.4; GCC/cc 15.2.0; GNU ld/binutils 2.46.
  Linux development headers/libraries serve as the SDK; no macOS/Windows SDK used.
- Ambient session: Wayland, `DISPLAY=:0`, `WAYLAND_DISPLAY=wayland-0`;
  `SLINT_BACKEND`, `SLINT_SCALE_FACTOR`, `WINIT_X11_SCALE_FACTOR` unset.
  No launcher activation token was present; token values were never logged or invented.
- Native Wayland protocol evidence reports 2560x1080 and scale 1.0
  (`preferred_scale(120)` in 120ths). This is the real compositor connection,
  even when the ambient smoke command is wrapped in `xvfb-run`.
- Xvfb: X.Org server 21.1.22, virtual X11 1920x1080x24; observed logical/native
  notes geometry is 800x608 (scale 1.0). This is neither native Xorg on a physical
  display nor XWayland. Software GL requested using `LIBGL_ALWAYS_SOFTWARE=1`.
  The build's default winit renderer is FemtoVG (source/configuration evidence);
  a GL vendor/driver version was not separately measured.
- Existing installed Quick Presenter: 2.5.0, preserved. This host was not treated
  as disposable. No application or Rust tests were run with sudo.

Full environment/package versions: [environment-before.log](../../artifacts/slint-1-19-linux-20261117-1440/environment-before.log).
Session details: [host-diagnostics.log](../../artifacts/slint-1-19-linux-20261117-1440/host-diagnostics.log).
That diagnostic's cgroup query returned ENOENT; no cgroup quota is asserted.

## Checkout, dependencies, and preserved state

The starting checkout was `codex/remove-raw-smoke-logs`, HEAD
`390c9ba34f7e1e760d0d6de9ddccc5a16921ad3c`, with no tracked edits and eight
untracked user documents listed below. It was an older 1.4.0 / Slint 1.18.1
checkout, not the requested migration. A read-only fetch obtained the requested
branch; a safe branch switch preserved those documents. The first switch was
blocked by the outer sandbox's read-only Git index; authorized host execution
succeeded. No reset, clean, merge, commit, push, or GitHub write occurred.

All validation executed on `codex/slint-1-19-migration`, HEAD
`deee3b6731e631c86309b8e681a2c3fd20afdaf6` (clean tracked state before execution).
The implementation commit `23648de76b2a4136a5d2f587e0f8353eb6e4e335` and audited
baseline `3c376b8fbe33ea506487cc21f42fdf0c72aa2394` are ancestors.
The one newer commit, `deee3b6`, contains the notes-resize settling-clock fix and
migration-index updates. It is committed here, not an unverified local edit.
Its exact diff is saved in `environment-before.log`.

The fix samples input/render/layout before starting the five-second settling
clock. Snapshot equality, the 200ms quiet period, fitting predicate and geometry
retry behavior are retained. No production layout changes or local repair were
needed to pass Xvfb. No application instrumentation was added.

`build.rs` uses one edition-1.18 compiler configuration for both the ordinary
and generated Linux source. All resolved Slint/internal Slint packages are
1.19.0; `i-slint-backend-winit` is a registry dependency. `vendor/winit` 0.30.13
remains the only winit resolution. See `dependency-audit.log`, `tree-winit.log`
and `tree-all.log`. `Cargo.lock`, Cargo features, build configuration and all
vendor patches were preserved. The existing unused-`futures` warning is not
promoted to an error.

Repository PDFium was chromium/8076 (x86_64 ELF) and mismatched the pinned
chromium/8111 manifest. It was preserved. `scripts/fetch_pdfium.py` downloaded
and SHA-256-verified the pinned native `pdfium-linux-x64.tgz` to
`/tmp/qp-slint119-pdfium-20261117-1440-host`.
Debug/check/test invocations used the full override file path
`/tmp/qp-slint119-pdfium-20261117-1440-host/lib/libpdfium.so`.
Version, architecture and library digest are in `execution-provenance.log`.
The latter exits 1 solely because the referenced Mac report is absent.

The pinned archive digest is
`bdc60126399f39fe768566a18d4be055a7b3681bc9e248e8a6656b4f6cae0aa2`.
No manifest, user PDFium binary, or repository license file was changed.
Packaged checks use a fresh 8111 copy and remove both PDFium override variables.

## Native automatic checks

All commands ran at repository root, with default features empty and no explicit
cross target, except child packaged checks deliberately run from `/tmp`.
Each command's exact arguments, output, exit status and elapsed time are saved
under [the raw evidence directory](../../artifacts/slint-1-19-linux-20261117-1440/).

| Command | Exit | Result / log |
| --- | ---: | --- |
| `cargo fmt --check` | 0 | `fmt.log` |
| `cargo check --all-targets --locked` | 0 | `check-host.log` |
| `cargo test --locked` | 0 | 629 passed, 2 ignored; `tests-host.log` |
| `cargo tree --locked -i winit` | 0 | `tree-winit.log` |
| `cargo tree --locked --prefix none` | 0 | `tree-all.log` |
| `python3 -m unittest tests/test_fetch_pdfium.py tests/test_release_sbom.py tests/test_cli_packaging.py` | 0 | 40 passed; `python-tests.log` |
| Explicit no-default-features library/control tests | 0 | 54 passed; `no-default.log` |
| `cargo build --locked` | 0 | `build-debug.log` |

The no-default command was exactly:

```sh
cargo test --locked --no-default-features --lib --test control_contract --test control_events --test control_ipc --test control_ipc_windows --test control_lock --test control_launch --test control_launch_windows
```

The Python suite has 13 fetch tests, 21 release-SBOM policy tests and 6 CLI
packaging tests (`python-test-discovery.log` records discovery without rerunning).
These are policy/unit tests, not verification of a generated package SBOM.

Full-suite per-target counts (no successful test was rerun just for separate logs):

| Target | Passed | Failed | Ignored | Filtered |
| --- | ---: | ---: | ---: | ---: |
| Library | 24 | 0 | 0 | 0 |
| qp binary | 0 | 0 | 0 | 0 |
| quick-presenter binary | 555 | 0 | 2 | 0 |
| audience_window_layout | 1 | 0 | 0 | 0 |
| control_contract | 5 | 0 | 0 | 0 |
| control_events | 3 | 0 | 0 | 0 |
| control_ipc | 8 | 0 | 0 | 0 |
| control_ipc_windows | 0 | 0 | 0 | 0 |
| control_launch | 12 | 0 | 0 | 0 |
| control_launch_windows | 0 | 0 | 0 | 0 |
| control_lock | 2 | 0 | 0 | 0 |
| macos_fullscreen_transition | 3 | 0 | 0 | 0 |
| presenter_layout | 1 | 0 | 0 | 0 |
| renderer_process | 15 | 0 | 0 | 0 |
| settings_storage_windows | 0 | 0 | 0 | 0 |
| store_ipc_probe example | 0 | 0 | 0 | 0 |
| Doc tests | 0 | 0 | 0 | 0 |

The explicit no-default-features run has library 24, control_contract 5,
control_events 3, control_ipc 8, control_launch 12, control_lock 2, and both
Windows-only control targets 0; all failed/ignored/filtered counts are 0.
Empty cfg-gated targets and the empty example/doc tests do not certify native
Windows/macOS behavior. The macOS fullscreen target runs three pure vendored
tracker tests; it does not create AppKit windows or certify native fullscreen.

Exact required cases passed within the full suite:

- `presenter_panes_resize_without_losing_notes_or_overlapping_the_footer`
  (compiled Slint UI on the software platform).
- `audience_window_does_not_cap_fullscreen_size_to_its_windowed_size`
  (source constraint, not a native fullscreen test).
- `quit_callback_and_presenter_close_share_recoverable_event_loop_exit`
  (software callbacks, exit failure/retry, audience Settings/About forwarding).
- `renderer_helper::environment_tests::linux_environment_keeps_only_reviewed_keys_and_debug_hooks`
  (actual module path; both launcher-token names are tested).
- All 15 real `renderer_process` tests, including
  `helper_does_not_inherit_broker_secrets_and_renders_with_preserved_locale`,
  `broker_survives_helper_fcntl_signal_attack_and_renders_pdf`, and
  `unavailable_landlock_and_flatpak_modes_fail_closed`.

Normal renderer isolation worked on this kernel. Deliberately injected
Landlock-unavailable/Flatpak fail-closed tests passed; they are not host failures.
Local HTTP/WebSocket and Unix IPC tests executed rather than merely compiled.

Ignored tests remain ignored with their original reasons:

- `audience::tests::sustained_local_load_remains_bounded_and_stops_cleanly`:
  “explicit local sustained-load validation; takes about 30 seconds”.
- `audience_relay::fixture_tests::external_relay_room_creation_round_trip_and_termination`:
  “requires a running OSS relay runtime and QUICK_PRESENTER_TEST_CONFIG”.

## GUI execution and diagnosis

All GUI invocations used the repository's bounded-command runner with a
300-second timeout, unchanged assertions/deadlines, and
`tests/fixtures/marp-speaker-notes.pdf`. Exact commands are in `gui-runs.sh` and
matching logs/JSON; separate stdout/stderr are retained in the `*-raw` directories.

| Execution | Passed | Failed | Exit | Evidence |
| --- | ---: | ---: | ---: | --- |
| Debug, ambient Wayland preserved inside Xvfb wrapper | 213 | 2 | 1 | `gui-ambient.txt`, `gui-ambient.log` |
| Debug, X11/Xvfb (Wayland display/socket variables removed) | 215 | 0 | 0 | `gui-x11.txt`, `gui-x11.log` |
| Focused comments, native Wayland with protocol trace | 142 | 2 | 1 | `gui-wayland-comments.txt`, `gui-wayland-comments.log` |

Both full runs passed the pre-PDF placeholder/short/long notes and subsequent
notes-resize/font-fitting checks; PDF loading/rendering and the complete X11
smoke succeeded. This validates the committed clock fix on this native host,
but does not isolate its causal effect: the old harness was not rebuilt here,
and the earlier 0.25-CPU reproduction was not repeated locally.

Both Wayland failures are
`Live comments measure the actual font and enter the logical canvas`, once
before opening the PDF and once afterward. `Hello 世界` has measured width 9.0
instead of the required >100; the history/text/position are otherwise present.
The focused reproduction confirms the same result at compositor scale 1.0.
`gui-diagnostics.log` summarizes report geometry and protocol observations.

Source inspection: `AudienceUi` sets the measurement text then synchronously
reads `comment-measure-width`; this is a hidden 32px, weight-700, no-wrap
`Text.preferred-width`. The preceding reaction test hides and re-shows the
audience, which on Wayland recreates a native window. The compiled font-size
constant is still 32px. This localizes the observation to text measurement in
the native Wayland lifecycle, but does not prove that recreation causes it,
that actual rendered text is clipped, or that Slint 1.19 introduced it.
No controlled Slint 1.18 comparison or speculative production patch was made.
A separate native follow-up must resolve it before claiming green Wayland GUI.

The smoke path still omits production `configure_presenter_menu_bar`;
its initial Linux menu mode differs from normal startup. This predates the
migration and was not changed to mask a result. Normal inline menus are retained.

## Historical CI evidence and host restrictions

The original [Ubuntu job](https://github.com/koichiro/quick-presenter/actions/runs/43899056288/job/133412757409)
failed installed Debian GUI smoke with 3 passed / 1 failed at the initial notes
resize. Its original report was downloaded read-only and preserved in
`historical-ci/ubuntu-deb-installed.txt` (see the directory for the exact file).
The snapshot is physical 800x608, no notes, font 12, viewport 348x203, all
measurement heights zero, slide 368x207, menu height 0 and main y=54.
This is resize/layout synchronization evidence, not a compiler/library failure.

The migration index's throttled Xvfb reproduction and Mac 323-check success
are historical, not local execution. `SLINT_1_19_MACOS_STEP4.md` and the linked
OS handoff documents are absent from this fetched branch, so their full contents
could not be audited here. Earlier PR #538's 215-pass Ubuntu run used a different
snapshot and cannot isolate Slint. No green Ubuntu CI rerun is claimed.

Outer sandbox failures are retained separately: initial PDFium fetching and
Cargo checking failed DNS resolution (`fetch-pdfium.log`, `check.log`), then
succeeded through authorized host execution. `sudo -n apt-get install -y xvfb`
failed because no-new-privileges prevented elevation. Instead, native Ubuntu
Xvfb/xserver-common packages were downloaded and extracted under
`/tmp/qp-slint119-xvfb-20261117-1440`; the Xvfb probe succeeded with existing
host libraries. No application sandbox restriction was weakened. Other required
native build/developer packages were already installed. No rejected automatic
approval is being represented as an application failure.

## Packaged checks

The native release build (`cargo build --release --locked --bins`) and
`scripts/build_linux_deb.sh` succeeded. The diagnostic unsigned package is
`artifacts/slint-1-19-linux-20261117-1440/package/quick-presenter_3.0.0_amd64.deb`.
Its SHA-256 and ELF identities are in `package-identity.log`. A separate package
input directory copies verified PDFium 8111 and references existing repository
assets/scripts, preserving the user's `pdfium/` and installed app.

The workflow's extraction checks were copied into
`verify-extracted-package.sh` and stopped with exit 127 at `python` (not installed
under that name). The retained retry script changes only that invocation to
`python3`; this is a host adaptation, not an application repair.

| Check | Exit | Result / log |
| --- | ---: | --- |
| Native release build | 0 | `build-release.log` |
| Debian package creation | 0 | `package-build.log` |
| Extracted metadata, content, file modes, desktop/icon validation | 0 | `package-extracted-python3.log` |
| Extracted `qp` CLI contract | 0 | 10 checks; same log |
| Bundled PDF smoke, direct executable from `/tmp` | 0 | 3 pages, first render 320x180; same log |
| Bundled PDF smoke, extracted `usr/bin` PATH/symlink from `/tmp` | 0 | same log |
| Extracted renderer sandbox denial gate | 0 | `package-sandbox.log`, `package-sandbox-result.json` |
| Extracted release GUI, X11/Xvfb/software GL | 0 | 215 passed / 0 failed; `package-gui-x11.txt` |

All packaged child invocations removed `PDFIUM_DYNAMIC_LIB_PATH` and
`QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE`, so success depends on bundled PDFium.
The extracted CLI and headless tests follow the corresponding pre-installation
workflow segment, with the package/extraction directories redirected to this
unique run. The added extracted sandbox/GUI checks exercise the release payload,
but do not substitute for an installed package.

**Blocked/skipped installed-package reproduction:** Quick Presenter 2.5.0 is
already installed and this is a non-disposable workstation. Per the assignment,
no system install, replacement or uninstall was attempted. Installed CLI,
installed sandbox/GUI smoke, installation/removal MIME-cache effects and
package-manager integration therefore remain unexecuted. There is no new
`ubuntu-deb-installed.txt` from this host; the preserved one is historical CI.
No final-package SBOM generation/verification or any later CI step was run or
inferred successful. The 21 passing SBOM policy unit tests are separate evidence.

## Pending step 5 and excluded work

Use [GUI_SMOKE_CHECKLIST.md](../GUI_SMOKE_CHECKLIST.md) and
[ISSUE304_LINUX_DESKTOP.md](ISSUE304_LINUX_DESKTOP.md) for separate real-desktop
observations: installed desktop-entry launch, real first-window activation/token
consumption, repeated launches, X11 icon/WM_CLASS, native Wayland app ID and
window recreation, focus, inline menus, notes readability/spacing, fullscreen,
multiple monitors and mixed DPI. Record native Xorg, XWayland and Wayland
separately; Xvfb cannot certify shell focus/icon latency or compositor fullscreen.
The automated Wayland observations above do not complete that manual checklist.

No coverage, signing, certification, upload/publish, merge, release, other OS
execution, external relay fixture or sustained-load ignored test was performed.

## Working-tree state at step-4 completion

At step-4 completion HEAD remained
`deee3b6731e631c86309b8e681a2c3fd20afdaf6` on
`codex/slint-1-19-migration`. The only tracked-file edit is the migration-index
link/summary; the new report is untracked. The original eight user documents
remain untracked and unchanged. No source, lockfile, compiler edition, vendor,
manifest or repository PDFium edits were made. Raw logs and local package files
remain under the repository's ignored `artifacts/` directory; they were not
force-added. See `environment-after.log`, `preservation-audit.log` and
`command-statuses.json` for the final state and exit-status inventory.
Documentation whitespace validation (`git diff --check`) passed; Cargo checks
were not repeated for these documentation-only edits.

The original untracked documents, preserved byte-for-byte (SHA-256 manifest in
`user-files-sha256-before.json`):

- `docs/validation/PR442_ANIMATION_INVESTIGATION.md`
- `docs/validation/PR442_CI_FINAL_RETEST.md`
- `docs/validation/PR442_CI_PACKAGE_RETEST.md`
- `docs/validation/PR442_LINUX_DESKTOP.md`
- `docs/validation/PR442_NOTES_RESIZE_INVESTIGATION.md`
- `docs/validation/V150_LINUX_DESKTOP.md`
- `docs/validation/V190_LINUX_DESKTOP.md`
- `docs/validation/V200_LINUX_20261015.md`

## Authorized Linux transfer after validation

The results above describe the validation checkout and execution-time dirty
state. After validation, the user authorized a transfer branch based on
`deee3b6731e631c86309b8e681a2c3fd20afdaf6`:
`codex/slint-1-19-linux-validation`. It carries the comment-measurement and About
repairs, the comment-layout regression test, these three Linux reports, and
Linux links in the migration index. The existing migration/base branches are
not pushed or merged by this transfer. Raw logs, screenshots, payloads, private
configuration and the pre-existing user documents are excluded from the commit.
Artifact references in these reports remain local-only evidence paths; no
GitHub artifact upload is implied. No tests were rerun solely for the transfer.

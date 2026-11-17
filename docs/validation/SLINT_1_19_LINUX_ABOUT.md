# Slint 1.19 Linux About readability follow-up

## Result

The About clipping has been repaired in `ui/app.slint`. Required formatting,
all-target checking and the full Rust suite pass (**630 passed, 0 failed,
2 ignored**). New debug and release binaries were built.

Actual native Wayland rendering verifies readable application/PDFium license
text and a visible, operable Close button at both 1024×768 and 800×608. The
button's visible pixel and pointer coordinates were checked, and Slint pointer
press/release events at that point dismissed the actual compiled dialog.

**The unlocked-desktop acceptance is complete.** After the user unlocked Linux,
normal release startup and actual Help → About pointer operations were tested
at approximately 17:02–17:06 JST on 2026-11-17. Both full license descriptions
are readable at normal size and at the compositor-confirmed 800×608 minimum.
The real Close button dismissed the dialog at both sizes. The app then exited
normally and its isolated control socket disappeared. No lock bypass or safety
setting change was used. Earlier locked-session diagnostics remain preserved.
A focused XWayland rerun after unlock also reached actual 800×608 and passed
native rendering/Close-event checks.

## Implementation

The old application-license Text had a fixed 54px height; the PDFium Text also
used a constrained stretch area with clipping. The dialog was capped at 420px.

The follow-up:

- Sizes both wrapped descriptions to their preferred text height.
- Places only the license body in a ScrollView, retaining header and Close
  outside it, so longer text remains accessible without pushing Close away.
- Raises the dialog cap to 560px while retaining the existing parent-minus-48px
  constraint for smaller windows.

No metadata, license wording, callback, application state, compiler edition,
dependency, or vendor patch changes are involved. This is an About-only layout
repair, separate from the earlier comment-measurement workaround.

## Classification

An independent identical Text probe used the real license wording, 512px text
width, 16px font and old 54px allocation with Slint 1.18.1 and 1.19.0. Both
reported **preferred height 66px**, exceeding the old allocation by 12px.
The fixed-height constraint itself also predates migration. This reproduces the
layout deficiency on the older stack under matched conditions, so it is not
classified as a migration-only defect. This comparison is not a reconstruction
of every platform style/default from the complete old application.

Sources, separate lockfiles and raw measurement outputs are saved under
`comparison-1.18.1/`, `comparison-1.19.0/`, `compare118-measure.log` and
`compare119-measure.log` in the evidence directory.

## Evidence

Absolute evidence directory:

`/home/koichiro/projects/quick-presenter/artifacts/slint-1-19-linux-about-20261117-1644/`

| Check | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --check` | PASS | `fmt.log`, `patch-and-final-check.log` |
| `cargo check --all-targets --locked` | PASS | `check.log` |
| `cargo test --locked` | PASS, 630 passed / 2 ignored | `tests.log`; includes the existing presenter layout regression |
| Debug/release build | PASS | `build-debug.log`, `build-release.log` |
| Compiled UI software snapshots, normal/minimum | PASS for software rendering | `software-about-retry.log`, `about-software-*`; default software font metrics differ from native styling |
| Native Wayland 1024×768 | PASS for native renderer and Slint input | `native-about-probe-wayland.log`, `about-native-wayland-normal.png` |
| Native Wayland 800×608 | PASS for native renderer and Slint input | `about-native-wayland-minimum.png`; both full license paragraphs and Close visible |
| Native XWayland normal | PASS for native renderer and Slint input | `native-about-probe-xwayland.log` |
| Native XWayland minimum after unlock | PASS, actual 800×608 | `native-about-probe-xwayland-unlocked.log`; earlier locked run remained 1024×768 and is not minimum-size evidence |
| New normal-launch OS pointer Close check, normal/minimum | PASS | `normal-release-open-about-focused.log`, `normal-release-close-about.log`, `normal-release-resize-minimum.log`, `normal-release-close-about-minimum.log` |

The initial temporary software probe had a closure-return compile error. It was
corrected only in the diagnostic example and rerun successfully; both logs are
retained. No application failure is inferred from that harness error.
Temporary examples were removed after their sources were copied to evidence.
The native probe uses the real winit/FemtoVG backend and production metadata,
but opens About programmatically and dispatches Slint WindowEvents. It does not
call the Close callback directly. Pixel checks assert that the click point is
on the visible gray button before testing dismissal. Wayland coordinates were
(757,622) at 1024×768 and (645,542) at 800×608, scale 1.0.

## Independent patch and payload

Apply the existing comment-measurement patch first, then:

`/home/koichiro/projects/quick-presenter/artifacts/slint-1-19-linux-about-20261117-1644/linux-about-readability-fix.patch`

SHA-256: `c0ba9b399b796614624f446d6059e3b9a8a8afdcd9a8cd099062ca38dd2a4651`.
Only `ui/app.slint` is in this patch. Reverse application checking against the
current working tree passed. The earlier comment patch was not overwritten.

The rebuilt local release payload is `payload/` in this new evidence directory,
with verified bundled PDFium 8111. `quick-presenter` SHA-256:
`93dcc1827e4dbfb433543a458babb8d347fa8f41a77707821d260efe168e31c5`.
The older step-5 payload/hash and 215-check release report predate this About
follow-up and remain historical evidence for the comment fix.

Validation ran on `deee3b6731e631c86309b8e681a2c3fd20afdaf6` with the repair
diffs then uncommitted. The later authorized transfer is recorded below.
Existing user files, installed 2.5.0 and repository PDFium were preserved.

## Completed desktop acceptance and cleanup

The new bundled release payload was launched through the isolated GDK Wayland
entry (`normal-release-launch.log`). An initial capture showed Codex in front;
that invalid app-only image was discarded. Clicking the exposed Presenter title
bar first restored the test window, then real compositor pointer events opened
Help → About. No application callback was invoked directly in this desktop path.

Verified app-region images:

- `about-desktop-normal.png`: full GPL and PDFium wording, independent Close.
- `about-desktop-normal-closed.png`: actual Close dismissed the normal dialog.
- `about-desktop-minimum.png`: actual compositor 800×608 configuration, complete
  wording including the final “only.” and “PDFium binaries.” lines, Close in bounds.
- `about-desktop-minimum-closed.png`: actual Close dismissed the minimum dialog.

The current English metadata fits in full at both sizes; scrolling was not
needed to reach the end. The ScrollView is retained so larger text content can
remain accessible without displacing the footer. Normal native Close was clicked
at desktop (1530,818), minimum Close at (1418,704). These are actual compositor
input coordinates, distinct from the earlier client-event probe coordinates.

`normal-release-cleanup.log` confirms process exit, socket cleanup and requested
restoration of the original Codex window focus. Display configuration still has
one physical/logical monitor, 2560×1080 at scale 1.0. The post-unlock XWayland
probe exited normally. Temporary diagnostic examples remain removed. No further
source changes followed the passing Rust suite, so it was not rerun for final
documentation and desktop observations.

No About-specific blocker remains. Overall step-5 physical multiple-monitor /
mixed-DPI qualification and the pre-existing Bring-to-Front limitations remain
separate, as documented in `SLINT_1_19_LINUX_STEP5.md`.

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

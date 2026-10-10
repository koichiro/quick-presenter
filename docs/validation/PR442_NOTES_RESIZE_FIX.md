# PR #442: notes resize smoke correction

Date: 2026-10-10 (JST). Baseline: `74e2ead`.
Linux desktop: Ubuntu 26.04.1, GNOME Wayland, one 2560 x 1080 physical display
at scale 1. Tests also use the desktop's XWayland backend.

## Problem and correction

The old smoke assertion required a larger font after requesting a
1200 x 1282 presenter window. GNOME's XWayland path constrained that request
to 1200 x 1011. The wider slide preview consumed more vertical space, leaving
a 180px notes viewport. The selected 14px font was the correct largest fit;
requiring it to exceed the earlier 19px was incorrect.

The corrected desktop resize assertion checks the settled notes viewport:
all 13 measurements must be finite and positive, the selected whole-pixel font
must remain within 12–24px, it must fit with bottom padding, every larger
candidate must overflow, and visible content must fit. It does not assume
native window requests are honored.

A separate strict growth assertion changes the slide preview aspect ratio
from 0.6 to 4.0 while keeping the native window size unchanged. This releases
space for notes without another oversized window request. The assertion
requires a larger actual notes viewport, an increased font reaching 24px,
and fitting visible content. It runs before opening a PDF and with a PDF open.
The original aspect ratio is restored by the existing cleanup.

Production notes rendering, Linux typography, native resize plumbing, and
the paragraph synchronization helper from #430 are unchanged. #425 introduced
the Linux paragraph/line spacing fix; #430 addressed stale paragraph
measurements. #428 addressed Windows MSIX process supervision and did not
change notes checks. The current failure is distinct from those fixes.

## Local verification

- `cargo fmt --check`, `cargo check --locked -j 2`, and
  `cargo build --locked --bins -j 2`: passed.
- `cargo test --locked -j 2 -- --test-threads=1`: 496 passed, one ignored.
  The ignored test is the existing sustained audience-load test.
- An initial parallel run encountered `ExecutableFileBusy` (`Text file busy`)
  in three `control_launch` tests while executing temporary shell scripts.
  The complete serial run passed. These startup tests and fixtures were not
  changed by this correction.
- Native Wayland, complete GUI smoke with
  `tests/fixtures/marp-speaker-notes.pdf`: 131 passed, zero failed.
  Resize checks selected 24px in 418px/414px viewports; controlled growth
  increased 180px viewports to 658px/654px and fonts from 14px to 24px.
- XWayland, complete GUI smoke with the same PDF: all notes checks passed,
  including both new assertions in both contexts. Before opening a PDF,
  the constrained viewport selected 14px at 180px; controlled growth increased
  the viewport to 387px and selected 24px. With the PDF open, the resize
  viewport was 414px and controlled growth was 180 -> 654px, 14 -> 24px.
  The overall run ended with 104 passed and three failures: two reaction
  animation assertions and a control GUI smoke timeout. Overlay timing and
  control failures were also encountered during the earlier diagnostic
  investigation. This is a limitation of this complete development-binary
  run; it is not recorded as a successful comprehensive XWayland run.

The runs use the matching PDFium from the PR's previous Linux CI package,
isolated configuration/state directories, and the development binary built
from this source. They are source validation, not a newly built package
qualification. Logs are retained locally under
`/tmp/qp-pr442-validation/notes-investigation`; raw logs are not committed.
macOS and Windows were not tested on local physical machines.

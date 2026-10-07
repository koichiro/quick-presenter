# PR 397 macOS startup restoration validation

Validated on October 7, 2026, with macOS 26.7 (25G229), Apple Silicon, and a
5120 × 2160 display at scale 1. The current debug build was staged with
`scripts/stage_macos_app_bundle.sh` and Developer ID signed using
`scripts/sign_macos_app.sh`. PDF operations used the signed XPC renderer.

## Findings and fixes

- Command-Q initially terminated through AppKit before startup settings could
  be saved. The vendored native menu now exits the Slint event loop, allowing
  the normal shutdown capture and persistence path to finish.
- Restoring a non-default window initially shifted it down by 16 points on
  each restart. Restoration now verifies the outer frame as well as client
  size, recalculates position after frame changes, and avoids restarting size
  requests during position-only retries.

## Observed results

| Check | Result |
| --- | --- |
| Open the speaker-notes fixture, advance a page, and quit with Command-Q. | Startup JSON was saved with the active PDF and native window geometry. |
| Relaunch without a PDF argument. | The remembered deck opened at page 1 with presenter controls focused and navigation working. |
| Change native window size using macOS Zoom, quit, and relaunch. | Native size was saved; the oversized geometry was clamped on restoration. |
| Seed a 900 × 600 non-default placement and perform successive normal launches/exits after the frame fix. | Size and relative center remained identical in the persisted native captures. |
| Open the 4:3 Beamer fixture through the native Open dialog, then explicitly launch the 16:9 demo. | Both opened successfully; the 900 × 600 saved size survived both aspect ratios. The explicit PDF superseded the remembered PDF. |
| Enter fullscreen, advance a page, and quit from presenter controls with Command-Q. | The exact prior windowed placement was retained. The next launch reopened the demo at page 1 in windowed mode. |
| Close the presenter window using its close button. | The process exited normally and saved startup state. |
| Move the remembered temporary PDF out of the way and relaunch. | A short reopen error appeared without opening another recent PDF. Quitting cleared the saved PDF. The following launch showed an empty usable presenter; Open and navigation worked with a replacement deck. |
| Run both smoke modes against a different fixture with logging/report options. | Successful exit; startup JSON remained byte-for-byte unchanged. |
| Run both smoke modes with a missing PDF. | Expected exit 1; startup JSON remained byte-for-byte unchanged. |
| Run the final signed GUI smoke. | 39 passed, 0 failed. |

The signing script's signed XPC render/notes and sandbox denial gate also passed.
The original recent-files settings were backed up before testing and restored
afterward; the newly generated startup settings were moved out of the user's
configuration directory after testing.

## Compile and test checks

- `cargo fmt --check`
- `cargo check`
- `cargo test --quiet`: 387 unit tests and 13 integration tests passed.
- `scripts/check_windows_cross.sh`: all Windows targets compiled successfully.
- `git diff --check`

The existing macOS `fetch_update` deprecation warning remains unchanged.

## Remaining manual coverage

Physical display disconnection, rearrangement, mixed scale factors, display
swapping, and quitting during a pending replacement open were not exercised in
this run. Direct mouse dragging of native window borders/title bars could not
be completed because the computer-use bridge returned `windowNotFoundAtPosition`;
size capture was exercised through native Zoom and non-default restoration
through the seeded record. Windows and X11 runtime checks remain outstanding.

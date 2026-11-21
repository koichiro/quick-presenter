# Linux browser activation and reaction smoke timing

## Browser activation

The Join URL click callback was reached on the Ubuntu GNOME desktop. A
temporary click probe exercised three positions along the link and recorded
each callback. The focused UI probe passed all 17 checks.

The previous `webbrowser::open` path selected `firefox_firefox.desktop` and
spawned `/snap/bin/firefox`. It returned success when the process was spawned,
but the child then failed with `snap-confine` reporting a missing
`cap_dac_override` capability. This was observed when launching the application
from the restricted test environment; it does not establish that every Linux
desktop has the same failure.

Linux now requests browser activation through the desktop OpenURI portal,
off the UI thread. The response subscription is installed before requesting
activation, and only a successful Response is accepted. Cancellation,
rejection, and a 30-second timeout report an error without launching a second
browser. When the session bus or portal interface is unavailable, the existing
launcher remains the fallback. Windows and macOS retain their existing path.
The failure is also placed in the main status text because the audience status
is refreshed periodically.

A direct desktop portal request started Firefox through the desktop service.
A temporary Rust probe using the new helper also received a successful portal
response. Both probes used a local diagnostic URL; no audience secret was
included in the report. Temporary probe code was removed.

Protocol references: [OpenURI](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.OpenURI.html)
and [Request](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html).

## Reaction observation budget

Earlier instrumentation measured approximately one second per render during
the XWayland smoke sequence. A nominal 50 ms event-loop wait could therefore
span most of a reaction's 1.8-second lifetime, and the next observation could
find an expired row. The user separately confirmed that normal operation
visibly animates reactions.

Only the Linux GUI smoke instance now uses a six-second reaction lifetime.
The ordinary application still uses 1.8 seconds; other platforms retain their
existing smoke lifetime. The smoke still requires movement, normalized bounds,
unchanged PDF render generation, suppression during blackout and hiding, and
expiry without new input. Its expiry deadline follows the observation budget.
Unit coverage verifies smoke motion, expiry, stale input rejection, and the
unchanged production expiry.

## Validation

- `cargo fmt --check`: passed.
- `cargo check --locked -j 2`: passed.
- `cargo test --locked -j 2 -- --test-threads=1`: 500 passed, one existing
  ignored test, no failures.
- Initial XWayland run: both motion checks and all reaction expiry checks
  passed. The run stopped later with a renderer-helper spawn failure during
  the control smoke reopen check (106 passed, one failure). Cargo tests were
  building executables concurrently; a sequential rerun is recorded below.
- Sequential XWayland run: 131 passed, no failures, including both motion
  checks, expiry, control reopen, and hidden-window recovery.
- Native Wayland run: 131 passed, no failures. The backend emitted its existing
  renderer-selection fallback warning and used FemtoVG on Wayland.

The GUI runs use the matching extracted CI PDFium library and default VSync.
Raw local evidence is under
`/tmp/qp-pr442-ci-retest/url-investigation`. These are source-build results;
the package built by CI after these changes needs separate qualification.

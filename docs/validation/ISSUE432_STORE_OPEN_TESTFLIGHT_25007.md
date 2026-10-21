# Issue 432: Apple-distributed Store `qp open` TestFlight build 25007

Date: 2026-10-21. PR #501 commit `a158c83d40f3eacb0a3053e8176c277cdc121e0c`
was packaged as version 2.5.0, build 25007, on macOS 27.0.1 arm64.
The Mac App Store package SHA-256 was
`f40815fcd6cb00d687d2b111efdb7152dd47b63f46aed034b596e75301421ecc`.
The private CGS import, bundle signature, and installer signature checks passed.
Transporter delivered the package at 2026-10-21 11:02 JST. After processing,
the build was assigned only to the internal `Release Validation` group with
one tester. No external beta review or customer release was requested. The
export-compliance answers followed the same standard-encryption and no-France
distribution declaration recorded for
[build 25006](ISSUE432_STORE_OPEN_TESTFLIGHT_25006.md).

TestFlight updated `/Applications/Quick Presenter.app` to build 25007. The
installed app had an App Store receipt, passed deep strict signature checks,
and passed the product GUI/CLI/broker identity, entitlement, and LaunchAgent
layout checks after Apple re-signing. The first CLI cold-start request returned
`TIMEOUT` after its 15-second startup wait, although the GUI subsequently
opened the previously selected three-page PDF and reached ready rendering and
notes. A subsequent `qp status` succeeded. The integrated product script
passed status, content queries, watch and navigation, goto and blackout,
close, bookmarked reopen, and repeated connections.

For a new PDF, the GUI showed a picker preselected on the exact requested
path. With Finder active, `qp close` returned successfully, the pending
`qp open` returned `CANCELLED` (exit 10), and the picker disappeared. A second
new-PDF picker left unanswered with Finder active disappeared at 120 seconds;
`qp status` reported `opening = false`. A fresh `qp open` displayed one new
picker and user cancellation returned `CANCELLED` (exit 10). This confirms the
inactive-window cancellation fix after Apple re-signing.

The unanswered request exposed a response-deadline race: its CLI result was
the generic `TIMEOUT` message `Control request timed out; query status before
retrying a mutation.` rather than the GUI's specific picker-timeout response.
The GUI's 120-second selection deadline and XPC server's 120-second response
deadline were identical. PR #501 commit `e453abc` gives the XPC response
an additional three seconds and the CLI an additional two seconds after that.
The corrected response passed on
[Apple-distributed build 25008](ISSUE432_STORE_OPEN_TESTFLIGHT_25008.md).

This build does not establish clean-account startup, Intel behavior, other
supported macOS versions, or a final customer Store installation. The 15-second
cold-start timeout did not recur on a stopped-GUI retry, and the first
stopped-GUI open after the update to build 25008 also succeeded. The initial
failure remains an observed startup limit rather than a demonstrated pattern.

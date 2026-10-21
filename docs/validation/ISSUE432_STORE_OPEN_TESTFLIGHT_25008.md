# Issue 432: Apple-distributed Store `qp open` TestFlight build 25008

Date: 2026-10-21. PR #501 code commit `e453abc` was packaged as version
2.5.0, build 25008, on macOS 27.0.1 arm64. The Mac App Store package SHA-256
was `708229eeaf7fd57079ba1ccada23af13a135842a23590f02c5e20247488a6492`.
The package passed the private CGS import check, deep app signature check,
installer signature check, and product identity and entitlement checks.
Transporter delivered it to App Store Connect at 2026-10-21 11:22 JST. After
processing, export compliance and English internal test notes were saved, and
the build was assigned only to the existing one-tester `Release Validation`
internal group. No external beta review or customer release was requested.
The export-compliance answers followed the same standard-encryption and
no-France distribution declaration as
[build 25006](ISSUE432_STORE_OPEN_TESTFLIGHT_25006.md).

TestFlight updated `/Applications/Quick Presenter.app` to build 25008. The
installed app had an App Store receipt and passed deep strict signature
verification. Its GUI, CLI, and broker passed the product identity,
same-team App Group, entitlement, control-symbol, and LaunchAgent layout
checks after Apple re-signing. From a stopped GUI, the installed CLI opened a
previously authorized three-page PDF and the app reached ready rendering and
notes. `scripts/check_macos_store_control.py --testflight` passed status,
content queries, watch/navigation, goto/blackout, close, authorized reopen,
and repeated connections.

For a new PDF, the GUI displayed a picker preselected on the exact requested
path. With Finder active and the picker unanswered for 120 seconds, the CLI
returned the GUI's specific `TIMEOUT` response (exit 9): `PDF selection timed
out; run qp open again if needed.` The picker disappeared and `qp status`
reported `opening = false`. A fresh `qp open` displayed one new picker;
selecting the requested PDF returned a successful mutation, and status reached
ready rendering and notes on page 1 of 3. After `qp close`, reopening that
path completed through its saved bookmark without another picker. For a
second new PDF, `qp close` while Finder was active dismissed the picker and
the pending `qp open` returned `CANCELLED` (exit 10).

The previously observed response-deadline race in
[build 25007](ISSUE432_STORE_OPEN_TESTFLIGHT_25007.md) is therefore resolved
on the tested Apple-distributed build. Build 25007's one failed 15-second CLI
cold start did not recur on a subsequent stopped-GUI retry, and build 25008's
first stopped-GUI open also succeeded. A clean user account, Intel hardware,
other supported macOS versions, and final customer Store installation remain
separate qualification gates.

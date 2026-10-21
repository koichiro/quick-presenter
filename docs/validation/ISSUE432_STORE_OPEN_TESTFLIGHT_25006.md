# Issue 432: Apple-distributed Store `qp open` TestFlight build 25006

Date: 2026-10-21. Commit `564975c7bdb67f10eb15d2d231191af10dc016f1`
from PR #501 was built as version 2.5.0, build 25006, on macOS 27.0.1 arm64
with the pinned PDFium `chromium/8076`. The Store-control release binaries
were packaged with the Mac App Store profile and Apple Distribution/Installer
identities. The signed package SHA-256 is
`18d74466f125f77970fb59487622bdd3771d3e04e2eb930c79983466b28f3b07`.
Package creation passed the private CGS import check, deep signature check,
and installer signature check. Transporter delivered build 25006 to App Store
Connect at 2026-10-21 09:25 JST; processing completed. The build was assigned
only to the existing internal `Release Validation` group with one tester. The
test notes explicitly limit it to internal `qp open` validation. No external
beta review or customer release was requested.

For export compliance, the build's bundled PDFium includes standard PDF
cryptography, and App Store Connect showed France as unavailable for this app.
The build questionnaire was answered with standard algorithms and no planned
France distribution. This preserves the existing availability and does not
make a blanket claim that the binary contains no cryptography. Reassess the
declaration if the bundled code or intended distribution changes.

TestFlight installed build 25006 at `/Applications/Quick Presenter.app`. The
installed bundle reported `CFBundleVersion = 25006` and had an App Store
receipt. Outside the restricted development shell sandbox, deep strict code
signature verification passed. The GUI, CLI, and broker passed the product
identity, same-team App Group, entitlement, control-symbol, and LaunchAgent
layout checks after Apple re-signing.

With no GUI process running, the installed CLI's `qp open` launched the GUI and
presented its native picker for a PDF that had never been authorized. Selecting
the requested `beamer-annotations.pdf` returned a successful JSON mutation;
the GUI displayed page 1 of 3 and reached ready rendering and notes. The
product XPC script then passed status, read-only queries, watch/navigation,
goto/blackout, close, authorized open, and repeated connections. An authorized
CLI reopen completed without another picker. A separate first-time selection
was cancelled: the CLI returned `CANCELLED` (exit 10), while status retained the
previous PDF, page 1 of 3, and ready rendering. A selected invalid PDF returned
`OPEN_FAILED` (exit 10), also retaining that deck. After quitting the GUI, the
CLI cold-started build 25006 and reopened the first-time PDF through its saved
bookmark without a picker; status again reached ready rendering and notes.

This validates the GUI-mediated Store `qp open` path on the Apple-distributed
TestFlight build and tested host. A clean account, Intel hardware, other
supported macOS versions, a full 120-second selection timeout, and final
customer Store installation remain untested. The broader signed-fixture cases,
including spaces and non-ASCII paths, concurrent startup, wrong selection,
moved bookmarks, filesystem access denial, and scope cleanup, are recorded in
[the local implementation validation](ISSUE432_STORE_OPEN_IMPLEMENTATION_20261021.md).

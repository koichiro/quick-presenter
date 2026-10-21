# Issue 432: Mac App Store `qp open` feasibility, 2026-10-21

## Scope and observed product behavior

This pre-implementation investigation used the Apple-distributed TestFlight 2.5.0 (25005) app on
macOS 27.0.1 arm64. It evaluates a file-authority design for issue #432; it
does not claim that `qp open` is implemented or that cold GUI startup works.
The subsequent implementation and signed fixture results are recorded in
[the #432 implementation validation](ISSUE432_STORE_OPEN_IMPLEMENTATION_20261021.md).
The XPC broker and the other CLI commands passed the signed-product tests in
[the build 25005 record](ISSUE431_STORE_CONTROL_TESTFLIGHT_25005.md).

The installed CLI returned exit code 7 and
`{"code":"UNSUPPORTED_PLATFORM","message":"Store CLI file opening requires authorized file access."}`
for `qp open <absolute PDF path> --json`. This is intentional in both
`src/control/client.rs` and `src/control/store_xpc.rs`. The Store CLI and broker
carry only the sandbox and shared App Group entitlements; the GUI additionally
has user-selected read-only file and app-scoped bookmark entitlements. The
broker relays an authenticated request, but neither the request's path nor the
App Group itself conveys file authority.

In the same installed app, a PDF outside its container was selected through
the GUI Open panel. `qp status --json` then reported the exact selected path,
page 1 of 3, and `render_state = ready`. This confirms the existing GUI
user-selection path on the Apple-distributed build. It does not test an
XPC-initiated selection. The current GUI stores security-scoped bookmarks and
resolves them for recent files; `src/macos_file_access.rs::restore` currently
returns an unmodified path when no bookmark exists, so it cannot be used as an
authorization test for a new CLI path.

## Signed preauthorized-file experiment

An uncommitted, limited experiment removed the two Store Open rejections and
added a GUI-side bookmark-existence check before dispatching `Command::Open`.
Only a path already present in the GUI's bookmark history could proceed. The
experiment passed `cargo fmt --check`, Store-feature `cargo check --locked`,
and Store-feature `cargo test --locked` (with the pinned PDFium library supplied
from the main checkout). A Developer ID-signed fixture was staged with sandbox
and App Group entitlements for the GUI and CLI. Its GUI and CLI connected
through the installed, Apple-distributed 25005 broker; the fixture was not
uploaded to TestFlight.

The fixture GUI restored the PDF previously selected in the TestFlight GUI.
After `qp close`, its CLI `qp open` on that exact path returned a successful
mutation; a following status showed page 1 of 3 and `render_state = ready`.
Opening a different PDF with no saved bookmark returned exit code 7 and
`UNSUPPORTED_PLATFORM`, while status retained the first PDF at page 1 of 3.
This is signed-runtime evidence that authorized bookmark resolution and the
existing XPC Open dispatch can work together, and that the experimental guard
can reject an unapproved path without replacing the current deck. The
experiment was reverted after the check because it does not handle first-time
authorization or absent-GUI startup.

## Feasible authority boundary

Apple documents that selecting a file in `NSOpenPanel` extends the selecting
app's sandbox, and that a stored security-scoped bookmark must be resolved and
balanced with `startAccessingSecurityScopedResource` and
`stopAccessingSecurityScopedResource`. The existing GUI already uses that
model. The supported design is therefore:

1. For a previously GUI-authorized PDF, the GUI checks for an existing bookmark,
   resolves it, and opens it without another prompt. A missing or invalid
   bookmark must not fall through to a raw path open.
2. For a first-time PDF, the GUI presents its own Open panel, suggested at the
   requested directory and filename. It opens only the file explicitly selected
   in that panel, and verifies that the selected file matches the requested
   target before reporting `qp open` success. Cancellation or a different
   selection leaves the current deck intact. The GUI creates or refreshes the
   bookmark and retains the scope; the renderer continues to receive only its
   read-only descriptor.
3. The CLI may locate and activate the containing GUI, but must not claim that
   a command-line path, CLI-side bookmark, or broker relay grants the GUI
   access. Silent first-time arbitrary-path opening is not established by this
   investigation.

`rfd::FileDialog` in the pinned 0.17.2 dependency has `set_directory` and
`set_file_name`, so the requested path can be offered as a picker hint. The
current macOS `pick_file()` call is synchronous on the GUI event loop. A CLI
request that waits for the user cannot simply reuse it: the control dispatcher
also runs on that loop, and the protocol has a 30-second request deadline.
Implementation needs an asynchronous panel completion path and an explicit
response policy for a person who takes longer than the deadline. An early
`authorization_required` result followed by a separate user action, or a
bounded interactive request with clear cancellation semantics, requires a
documented Control Protocol contract decision.

Apple provides `NSWorkspace.openApplication(at:configuration:completionHandler:)`
for app activation. Whether the sandboxed packaged CLI can use it to start the
installed GUI and then discover the broker reliably is still a signed-runtime
test, not an observed result. The current Store `launch::send` calls only
`client::send` and does not start the GUI.

## Validation required before enabling `qp open`

- Extend the Developer ID-signed fixture to prove an XPC-triggered GUI picker
  for a new PDF. Check cancel, wrong selection,
  missing/revoked/moved files, and preservation of the current deck and scopes.
- Prove absent-GUI startup, concurrent `qp open` calls, readiness waits,
  exactly-once mutation submission, and no automatic replay after uncertain
  outcomes. Keep CLI stdout clean and typed errors stable.
- Repeat first-time and previously authorized opens with an Apple-re-signed
  TestFlight build. Cover spaces and non-ASCII paths, relaunch/bookmark refresh,
  failed replacement, and renderer denial of unrelated files.

The present evidence supports a GUI-mediated authorization design. It does not
yet establish acceptance of issue #432 or support for `qp open` in build 25005.

## References

- [Apple: Accessing files from the macOS App Sandbox](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox)
- [Apple: NSOpenPanel](https://developer.apple.com/documentation/appkit/nsopenpanel)
- [Apple: NSWorkspace app activation](https://developer.apple.com/documentation/appkit/nsworkspace/openapplication(at:configuration:completionhandler:))
- [Issue #432](https://github.com/koichiro/quick-presenter/issues/432)

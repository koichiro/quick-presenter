# Issue 432: Store `qp open` implementation validation, 2026-10-21

## Behavior

The optional v2.5.0 Mac App Store control build accepts `qp open <file>` over
authenticated XPC. If the GUI is absent, the bundled CLI activates its
containing app with `NSWorkspace`, waits up to 15 seconds for a status response,
then submits the open once. The broker does not receive PDF bytes or file
authority. A previously selected PDF is opened through the GUI's saved
security-scoped bookmark. A new path opens the GUI's native file picker with
the requested directory and name suggested. Only the exact selected file may
replace the current deck. The selection has a 120-second deadline; cancellation,
a different choice, or `qp close` cancels the request without replacing the
current deck. A concurrent open returns `BUSY`.

The CLI path is a hint, not a sandbox grant. The GUI owns bookmark creation,
resolution, scope lifetimes, and the read-only descriptor passed to the
existing isolated renderer. Successful replacement keeps only the new active
scope; failed replacement releases candidate scopes; close releases all active
scopes. A moved bookmarked file may resolve to its new path, which is reported
in the mutation response. A nonexistent new path leaves the picker with Open
disabled; cancellation returns `CANCELLED`, or the deadline returns `TIMEOUT`.

## Local validation

Source was built as release binaries with `macos-store-control`, staged in a
separate `Quick Presenter.app`, and signed with the local Developer ID identity
using the same sandbox, App Group, user-selected read-only file, and bookmark
entitlements as the Store control product. The fixture used the installed
Apple-distributed TestFlight 25005 broker of the same team and bundle identity.
This is a signed development fixture, not an Apple-re-signed build of the new
source. The existing 25005 `qp open` remained unsupported; the later
[Apple-distributed build 25006 validation](ISSUE432_STORE_OPEN_TESTFLIGHT_25006.md)
qualifies the new source on the tested host.

On macOS 27.0.1 arm64, the signed fixture demonstrated:

- Absent-GUI startup and bookmark-backed open with clean JSON stdout, followed
  by status `render_state = ready`.
- First-time picker selection of a new PDF, including a filename containing
  Japanese characters and a space; the CLI returned the opened path and the GUI
  rendered page 1 of 2.
- Cancellation and selection of a different PDF returned `CANCELLED` (exit 10)
  while the existing deck remained ready.
- A second open during selection returned `BUSY` (exit 8). `qp close` cancelled
  the outstanding picker and request. Two simultaneous opens from a stopped GUI
  produced one GUI process; one succeeded and the competing request returned
  `BUSY`, with no replay.
- Close/reopen and quit/relaunch reused the persisted bookmark without a picker.
  Moving the selected PDF refreshed its bookmark and reported the new path.
- A selected invalid PDF and a bookmarked PDF whose filesystem permissions
  were removed returned `OPEN_FAILED` (exit 10), preserving the previous deck.
  A missing PDF had no selectable target; cancellation left the deck intact.
- `scripts/check_macos_store_control.py` passed the signed product checks,
  including wrong-peer rejection, status, content queries, watch/navigation,
  blackout, close, authorized open, and repeated connections.

`cargo fmt --check`, default and Store-feature `cargo check`, and default and
Store-feature `cargo test` passed. The standard test suite requires execution
outside the restricted development sandbox on this host because its Unix socket
and settings tests receive `EPERM` there. The Store-feature test suite passed
within that sandbox. The existing `fetch_update` deprecation warning and two
unused test-helper warnings are unrelated to this change.

## Remaining release validation

The first-time picker, cold startup, bookmark persistence, failure handling,
and CLI status were repeated after Apple re-signing in internal TestFlight
build 25006. Other supported macOS versions, Intel hardware, a clean user
account, and final Store release remain separate qualification gates. A long
interactive timeout and a genuinely revoked macOS security-scoped bookmark
were not separately exercised; the bounded timeout path is implemented, and
filesystem read denial was tested.

CodeRabbit identified that a standalone picker might remain visible when the
app is inactive during cancellation. The cancellation helper now searches all
application windows as well as attached sheets. The revised source passed the
Store-feature tests and a fresh Developer ID-signed fixture build. A live
inactive-window cancellation check remains outstanding because the Mac was
locked during this follow-up.

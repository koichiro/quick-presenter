# macOS Finder document delivery validation — 2026-10-23

Issue: [#492](https://github.com/koichiro/quick-presenter/issues/492).

## Candidate and scope

The candidate is based on main `477d81a3628bb9a2f90d59f6b7b88c76ae1a093a`
plus this change, using PDFium `158.0.8111.0`. Validation ran on macOS 27.0.1
(26A434), arm64. An isolated bundle identifier preserved the installed TestFlight
2.5.0/25009 application. Developer ID signing used Team `NP5Q6U6NT5`.
The final GUI SHA-256 is
`5ccbb745d255f0677f8f11096a5c9066354587e717af3e5d5e4f0e0579006e91`.
The read-only candidate DMG SHA-256 is
`09aed05080ca45c021f59205e608e24ad6d3038e56b207823f28e53821fd50ff`.

The final candidate uses the Store UI sandbox/read-only selected-file/bookmark
entitlements with a Developer ID signature. This is a local sandbox validation,
not an Apple-processed TestFlight build, notarization, or customer distribution.
No default PDF association was set. Finder continued to identify Preview as the
default application during Open With verification.

## Automated results

- `cargo fmt --check`, `cargo check`, and `cargo test`: passed, 502 Rust tests.
- `cargo check --features macos-store-control`: passed.
- Release builds with the default and Store-control configurations: passed.
- CLI packaging and native document bridge: 7 Python tests passed.
- Native bridge compilation for arm64 and x86_64: passed; no Intel runtime claim.
- Native tests exercise the actual delegate selector, bounded multi-file delivery,
  modal rejection without replay, subsequent event acceptance, non-file URL
  rejection, short output-buffer cleanup, and window-drop enqueueing.
- Rust tests verify idle/manual/automatic-restoration/file-dialog admission.
  Existing renderer tests cover missing files, failed candidates, and supervised
  recovery. These tests do not substitute for the remaining GUI checks below.

An existing `AtomicUsize::fetch_update` deprecation warning is unrelated.

## Observed package behavior

- A signed Developer ID app opened a PDF from a cold Launch Services request,
  showing the requested filename, page 1/2, and Ready. An already-running request
  loaded a second PDF; the Right key immediately advanced to page 2/2.
- Finder > Open With > Other opened the requested PDF without the original
  unsupported-format alert. Always Open With remained unchecked. A temporary
  directory required the All Applications filter; Launch Services subsequently
  listed the signed mounted-DMG candidate as an application for this PDF type.
- An invalid PDF reported an error while retaining the previous document. A later
  valid native request recovered to page 1/2 and Ready.
- With UI sandbox enabled, cold and already-running requests loaded both external
  fixtures; a read-only bookmark was persisted. Quit/relaunch without a new file
  restored the second PDF without another selection dialog.
- The final read-only-DMG candidate opened the first PDF despite a saved second
  PDF, confirming native explicit-open precedence over remembered restoration.
- With the final candidate's native file dialog visible, a second Launch Services
  request was rejected. Cancelling the dialog retained the first PDF, displayed
  the rejection message, and did not replay the second file. The Right key then
  advanced the original PDF to page 2/2.
- Deep/strict signature verification passed. The initial Developer ID signing
  gate also passed supervised PDF rendering and private-file/network/child denial.

## Remaining acceptance checks

- Finder drag-to-app/Dock and drops into both application windows need a visible
  end-to-end result. The attempted temporary-folder icon drop did not demonstrate
  delivery; it is not a pass or evidence of an application regression. Subsequent
  Finder keyboard/focus automation stopped producing the requested UI transitions.
- A missing-file native event and a second native request during a deliberately
  held renderer open need full package-level verification. The deterministic
  native/Rust boundary tests cover these admission/error paths only in parts.
- Repeat Finder Open With on the final mounted sandbox candidate and qualify the
  rebuilt production package under #508 before publication.

Keep #492 open until its remaining package-level acceptance checks are completed.
This report records results only; raw smoke/signing logs are not committed.

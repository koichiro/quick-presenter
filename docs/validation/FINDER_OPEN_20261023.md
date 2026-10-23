# macOS Finder document delivery validation — 2026-10-23

Issue: [#492](https://github.com/koichiro/quick-presenter/issues/492).

## Candidate and scope

The candidate is based on main `477d81a3628bb9a2f90d59f6b7b88c76ae1a093a`
plus this change, using PDFium `158.0.8111.0`. Validation ran on macOS 27.0.1
(26A434), arm64. An isolated bundle identifier preserved the installed TestFlight
2.5.0/25009 application. Developer ID signing used Team `NP5Q6U6NT5`.
The preceding candidate GUI SHA-256 is
`5ccbb745d255f0677f8f11096a5c9066354587e717af3e5d5e4f0e0579006e91`.
The preceding read-only candidate DMG SHA-256 is
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
- The maintainer manually verified PDF drops onto the presenter window and the
  running Dock icon using the final signed candidate on 2026-10-23. Both opened
  the dropped PDF successfully. This is a user-reported end-to-end result,
  distinct from the earlier unsuccessful automated temporary-folder icon drag.
- Deep/strict signature verification passed. The initial Developer ID signing
  gate also passed supervised PDF rendering and private-file/network/child denial.

## Missing-file follow-up

A direct Open Documents Apple event (`aevt` / `odoc`, direct-object list with
one `typeFileURL`) was sent to the actual running signed process for the absent
`missing-native-request.pdf` fixture. Standard `NSWorkspace.openURLs` rejected
that absent URL before application delivery with Cocoa 260 / OSStatus -43; that
preflight rejection was not counted as an application-level pass. The direct
send used never-interact / do-not-prompt-for-consent options and required no new
Automation permission.

The original candidate retained the active PDF and recovered on a later valid
request, but incorrectly displayed an Unexpected error message. The diagnostic
record confirmed `No such file or directory (os error 2)`. The render-open failure
UI boundary now supplies the missing operation context, so the existing PDF-open
classification displays `Could not open PDF. Choose another file.` Specific
engine, signing, and protected-PDF classifications still take precedence.

The follow-up candidate was rebuilt with this error-context change, signed with
the same Developer ID team and UI sandbox entitlements, and launched from the
read-only `QP492Missing` DMG. The GUI SHA-256 is
`afed677673f808374e18a1c3ad1b966939efaf7e002994bcc9ed49e38652bc86`;
the DMG SHA-256 is
`2aec69dae0130c6a81738046dc05c09336f446cd1e973769e7cde9846beb43c4`.
Earlier manual presenter/Dock results above apply to the preceding candidate;
this follow-up changes error presentation only.

The follow-up package passed all of these observed steps:

1. Open the first normal fixture through Launch Services: page 1/2, Ready.
2. Deliver the absent-file Open Documents event: the short PDF-open error appears;
   the first fixture and page 1/2 remain visible.
3. Press Right: the first fixture advances to page 2/2 after the error.
4. Open the second normal fixture through Launch Services: page 1/2, Ready.
5. Press Right: the second fixture advances to page 2/2.

Deep/strict signature verification, `cargo fmt --check`, `cargo check`,
`cargo check --features macos-store-control`, all 502 Rust tests, and the seven
native bridge/packaging Python tests passed after the change. The native bridge
regression now explicitly asserts that an absent file URL retains its exact path
for the Rust error-handling path. Raw event replies and smoke logs are not committed.

## CodeRabbit follow-up: first-use window-drop authority

The review identified that the original `DroppedFile(PathBuf)` route discarded
native URL authority before reconstructing a file URL. The earlier manual drop
success did not establish a clean first-use permission state and is not used to
dismiss this finding.

The local winit window delegate now forwards the original dragging pasteboard
to opt-in application handlers before converting paths. The native bridge reads
file NSURL objects with `readObjectsForClasses:options:` and retains the original
object in the bounded queue. Both windows register for modern file-URL drags;
handled native drops do not emit a duplicate path-only event. The application no
longer has a path-to-URL reconstruction entry point for window drops.

The native test now covers a real URL pasteboard, modal rejection, URL lifetime
after clearing the pasteboard, and an identity assertion that fails if the bridge
reconstructs the URL from its path. All 502 Rust and seven bridge/packaging tests,
default/Store checks, the release build, x86_64 native compilation, and deep/strict
signature verification passed. Binary CI now includes the two window-integration
source paths and the vendored winit boundary in its PR filter.

A new, previously unused sandbox identifier,
`app.quickpresenter.first-drop-review-20261023`, was verified to have no container
before launch and no bookmark store after launch. Its signed UI started with
No PDF opened. Two separate Desktop fixtures were prepared for first-use
presenter and audience drops without a prior dialog, Dock open, or bookmark.
The current review candidate GUI SHA-256 is
`575ccefb610765a824f37a5b4b9b30943f10da8e0d2f617b0a462c653ed97555`;
its read-only DMG SHA-256 is
`68644ad92a88b39885a91672a0145c2763b02a93a9445bd9405adf5d1c63b5dc`.
The maintainer then reported successful first-use drops of both dedicated
fixtures into the presenter and audience windows, without opening them through
a dialog or Dock first. The new container's bookmark store was checked afterward
and contains both dedicated fixture paths. Thus the first-use authority concern
has package-level confirmation in addition to the native identity regression.
Earlier manual results above apply to the preceding candidate. See Apple's [file drag guidance](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/DragandDrop/Tasks/DraggingFiles.html)
for the sandbox distinction between paths and URL delivery.

## Remaining acceptance checks

- First-use presenter and audience drops passed on the current review candidate;
  Dock drops passed on the preceding candidate. A Finder application-bundle icon
  drop has not been separately verified. The earlier temporary-folder icon attempt remains
  inconclusive; subsequent Finder focus automation did not complete.
- Missing-file native-event error/recovery passed on the follow-up signed package.
  A second native request during a deliberately held renderer open still needs
  full package-level verification; deterministic boundary tests cover admission.
- Repeat Finder Open With on the final mounted sandbox candidate and qualify the
  rebuilt production package under #508 before publication.

Keep #492 open until its remaining package-level acceptance checks are completed.
This report records results only; raw smoke/signing logs are not committed.

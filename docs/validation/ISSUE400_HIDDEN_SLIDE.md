# Issue #400: hidden slide investigation

Investigated and repaired on October 7, 2026, on macOS 26.7 (25G229).
The initial investigation is recorded below; subsequent repair validation
is recorded in the final section.

## Finding

The stale audience image reproduces in both the retained pre-#399 signed
application and the exact signed candidate named in #400. It is an existing
macOS window lifecycle defect, rather than a regression introduced by
display-aware rendering.

Closing the audience window hides it through Slint. Both **Show Slide Window**
and **Bring Slide Window to Front** then reveal the native NSWindow without
restoring Slint's visibility. The backend continues to skip rendering, leaving
the last displayed backing image on screen.

Fullscreen and PDF replacement are not necessary: closing the slide, advancing
the page in the presenter, and choosing Show Slide Window also leaves the
audience on the old page.

## Artifacts and method

Tests used normal application entry points, native Open/Window menus, the
audience close button, and the signed XPC PDF renderer. No GUI smoke entry
point, application code modification, or unsigned replacement was used.

| Artifact | Executable SHA-256 |
| --- | --- |
| Retained PR #397 baseline: `/private/tmp/qp-macos-pr397/Quick Presenter.app` | `29e139b65d9d2a2e1c128adfd86f069b82008b3e4f469b793f0426b8bc5b9cf5` |
| Exact #400 candidate: `/private/tmp/qp-issue271-implementation/Quick Presenter.app` | `568f88be47499302b8b1465d5c18daeca09ac1329f98e8d909ce4ff83ab41e2e` |

Both bundles passed `codesign --verify --deep --strict` outside the execution
sandbox and report TeamIdentifier `NP5Q6U6NT5` with hardened runtime enabled.
Verification inside the sandbox incorrectly reported invalid signatures; the
unsandboxed verification succeeded without changing either bundle. The
baseline process's executable path was also checked during reproduction.

PDF replacements affected only `/private/tmp/qp-issue400/watched.pdf`. The
replacement portrait fixture was copied from the retained
`/private/tmp/qp-issue271-tall.pdf` (two 200 x 400 pt pages labeled
"Tall baseline test"). The speaker-notes fixture was
`tests/fixtures/marp-speaker-notes.pdf`.

## Reproduction

1. Open a temporary copy of `marp-speaker-notes.pdf` through the native Open
   dialog and advance to page 2.
2. Choose Window > Bring Slide Window to Front and verify **Sample 2**.
3. Click the audience window's red close button. The presenter remains open.
4. Overwrite the watched temporary PDF with the portrait fixture.
5. Wait until the presenter displays **Tall baseline test**, page **2 / 2**,
   and no speaker notes. In the first runs it also displayed **PDF reloaded.**
6. Choose Window > Bring Slide Window to Front.

On both signed artifacts the audience displayed **Sample 2**, while the
presenter had already displayed the replacement portrait PDF. The candidate's
audience image also remained unchanged after Left/Right key presses.

## Additional observations

| Artifact / sequence | Observed result |
| --- | --- |
| #400 candidate: close button, portrait reload, Bring Slide Window to Front | Stale Sample 2 image. |
| PR #397 baseline: close button, portrait reload, Bring Slide Window to Front | Same stale Sample 2 image. |
| Fresh PR #397 launch: Hide Slide Window, reload from portrait to Marp, Show Slide Window | Correct replacement Sample page displayed. |
| Same baseline session after that successful show: close button, reload from Marp to portrait, Show Slide Window | Stale Sample page displayed. |
| Fresh #400 candidate session, with Marp visible: close button at page 1, presenter Next Page, Show Slide Window; no PDF replacement during this sequence | Presenter displayed Sample 2 and its note; audience remained on Sample (page 1). |

The close-button/Show failure was observed on a separate candidate launch from
the initial reload/Bring failure. These observations narrow the trigger to
Slint hiding the window, rather than hiding the native window alone.

## Causal chain in the source

References below describe the investigation checkout at `86bebf9`. The same
native early-return show path is present in the pre-adaptive source at
`cfc22ca`.

1. Only the presenter has an application `on_close_requested` callback
   (`src/main.rs`, `wire_presenter_close_request`). The audience close request is
   propagated by `start_slide_placement` and follows Slint's default hide path.
2. Slint 1.18.1's `api.rs` handles `WindowEvent::CloseRequested` by calling
   `hide()` when `request_close()` permits it. `WindowInner::hide()` calls the
   window adapter's `set_visible(false)` and releases its visible-component
   reference.
3. The vendored winit adapter's `set_visible(false)` sets
   `WindowVisibility::Hidden`. Its `draw()` immediately returns when that state
   is Hidden (`vendor/i-slint-backend-winit/winitwindowadapter.rs:841`).
4. Both native menu callbacks call `show_slide_window`
   (`src/main.rs:971`, `src/main.rs:1001`). That function sets the placement
   visibility flag, calls the macOS helper, and returns on native success
   **before `slide.show()`** (`src/window_controller.rs:817`).
5. The macOS helper activates, deminiaturizes, and orders the NSWindow to the
   front (`src/macos_window.rs:14`). It never changes Slint's visibility. Its
   success boolean reports finding/invoking the native window, not restoring
   the rendering lifecycle.
6. Thus Rust menu/placement state and native visibility say "shown", while
   Slint's adapter remains Hidden. Updating `slide.page-image` cannot overcome
   `draw()`'s hidden-state guard. AppKit reveals the old backing image.

The native Hide Slide path explains the successful comparison: it calls
`NSWindow.orderOut` and returns before `slide.hide()`. From a fresh, normally
shown window, this leaves Slint's rendering state shown. A later native raise
therefore does not encounter the same permanent hidden-state draw guard.
Using Hide Slide after a close-button failure does not restore Slint's state.

The adaptive sampler checks `window.is_visible()` before accepting a surface
width (`src/window_controller.rs:18`). Missing sizing transitions after a
Slint hide are consistent with this lifecycle mismatch. More importantly,
the pre-#399 reproduction and the no-reload navigation reproduction establish
that adaptive sizing and PDF replacement are not prerequisites.

## Repair boundary and remaining qualification

The repair belongs in the window controller/macOS lifecycle boundary. Showing
a Slint-hidden audience must restore Slint's lifecycle before native focus or
raise handling can be treated as successful. Native and Slint hide/show state
should remain consistent, including the menu and placement flags. Requesting
another PDF render or reassigning the image alone cannot bypass the Hidden
draw guard. Merely requesting a redraw cannot do so either.

Focused coverage should check close/hide/show/raise visibility transitions and
the newly rendered frame, including a closed window whose model image changes.
A signed-package recovery test must additionally establish that the current
image appears without exposing an unrelated old document and that navigation,
notes, black screen, window placement, and fullscreen remain usable.

This investigation did not implement or qualify that repair, repeat the
original fullscreen sequence, or test explicit Open replacement while hidden.
It does not clear the release blocker. Runtime adapter visibility was not
instrumented; the lifecycle explanation follows the actual close/show paths
and backend guard, supported by the signed UI reproductions above. No
dedicated screenshot files or tracing log were saved; the observed presenter
and audience screenshots are in the investigation chat's computer-use output.

The original `recent-files.txt` was backed up and restored byte-for-byte after
all test applications exited. `startup-state.json` did not exist before the
investigation; the generated file was moved to the investigation backup
directory. Only this report changes the repository, so Cargo checks and
coverage were not run.

## Repair and validation

The window controller now calls Slint show before native macOS activation or
ordering, and hides through Slint on every platform. Closing the audience also
updates the Rust menu visibility state. Native focus handling remains available
for an already shown or minimized window; it no longer substitutes for Slint's
show/hide lifecycle. Failed shows return before the native window is raised.

The vendored macOS backend resets its first-frame marker on hide. The next show
attempts to render the current component before mapping it. If Metal cannot
render while hidden, the existing `RevealOnFirstFrame` guard masks the backing
image until a frame is submitted. This avoids restoring rendering only after
an unrelated old frame has already been revealed. As with the existing startup
guard, a rendering error ends the wait instead of leaving the window permanently
invisible.

The GUI smoke now covers all four close/Hide and Show/Bring combinations in
both windowed and fullscreen modes. Each case changes the hidden window's image
to a distinct solid color, verifies hidden menu/Slint state, checks that showing
restores visibility and produces a new AfterRendering notification, and checks
the center pixel of a renderer snapshot. These are main-thread GUI checks,
rather than window creation inside parallel Rust unit tests.

Final source validation passed:

- `cargo fmt --check`
- `cargo check`
- `cargo test --quiet`: 398 unit tests and 13 integration tests passed.
- `cargo build`
- Developer ID signing and `codesign --verify --deep --strict`.
- Signed XPC PDF render/notes and private-file/network/child denial gate.
- Final signed GUI smoke: **63 passed, 0 failed**.

The worktree reused the existing PDFium library through
`PDFIUM_DYNAMIC_LIB_PATH=/Users/koichiro/projects/quick-presenter/pdfium/lib/libpdfium.dylib`.
Initial test attempts without that library location failed during PDFium
initialization; the correctly configured full suite passed. The existing
macOS `fetch_update` deprecation warning remains unchanged.

Final signed executable SHA-256:

```text
fef61491716c5ad9b825fe128d7bafaaeb1c023809679bf04b5b601a785d9833
```

Normal-entry-point manual recovery tests passed on the first signed repair
artifact (`78f030dfefff2dc7c87c54f1e33d7d0e6a80e6cf578e60801fb6af78569490be`):

| Sequence | Observed result |
| --- | --- |
| Close Sample 2, replace the watched PDF with portrait, Bring Slide to Front | Both windows showed the portrait replacement at page 2. |
| Enter fullscreen, replace portrait with Marp, navigate to page 2, exit fullscreen, close, replace with portrait, Bring Slide to Front | Current portrait image displayed; old Sample 2 did not remain. |
| Close portrait, explicitly Open the Marp fixture, advance to page 2, Show Slide | Audience displayed Sample 2; presenter showed its note. |
| After recovery, use Left and B in the audience window, then B again | Page returned to 1, black screen toggled and cleared, presenter remained synchronized. |

The final artifact adds fullscreen cases to GUI smoke; production repair code
is identical to the manually tested artifact. Its signed GUI run covers all
eight lifecycle combinations. Manual testing used the scale-1 external display
from the initial investigation. Mixed-DPI, display disconnect, and Windows/Linux
runtime qualification remain outside this fix. The manual screenshots verify
settled output; they are not a frame-by-frame measurement of reveal timing.
Application settings were again backed up and restored after manual testing.
No PDFium version, ownership, or native-library payload was changed.

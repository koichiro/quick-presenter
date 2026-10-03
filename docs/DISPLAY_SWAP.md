# Presenter and Slide Display Swap

## Status

This document proposes the implementation design for
[GitHub issue #384](https://github.com/koichiro/quick-presenter/issues/384).

The feature is not implemented by this document. Until the implementation lands,
`X` has no display-management behavior.

## Goal

Provide the same fast recovery path presenters expect from Keynote: pressing the
unmodified `X` key swaps the displays occupied by the presenter window and the
audience slide window.

The swap changes window placement only. It must not change which window owns
presenter controls or which window renders the audience slide.

## Behavior Contract

- `X` invokes the swap from either Quick Presenter window when that window has
  keyboard focus.
- The presenter and slide windows exchange their current displays. With three or
  more displays, unrelated displays are not involved.
- Repeating `X` swaps the same windows back when the display topology and window
  placement have not changed.
- The shortcut works before or after a PDF is opened.
- The current page, page images, notes, elapsed timer, black-screen state, render
  cache, render queue, and PDF session remain unchanged.
- The app-controlled slide fullscreen state remains unchanged. A fullscreen slide
  becomes fullscreen on the destination display without intentionally passing
  through a visible windowed state.
- A windowed window keeps its logical size and approximately the same relative
  center within the destination display. Its final position is clamped so that a
  useful part of the window remains reachable.
- Keyboard focus returns to the window that received `X`. A successful swap must
  not require an extra click before the next presentation shortcut.
- A hidden slide window is not shown as a side effect. Its last known display can
  participate only when the backend can still provide a reliable native window
  snapshot; otherwise the command is a no-op.
- When both windows resolve to the same display, only one usable display exists,
  either window cannot be inspected, or the backend cannot place both windows,
  the command is a no-op. It must never perform a one-sided move.
- If the display topology changes while a swap is being applied, the controller
  keeps or returns both windows to available displays and reports a short status
  message in the presenter window.

The first implementation does not persist an explicit presenter-display or
slide-display preference. Issue #31 remains responsible for any placement
persistence across launches.

## Non-goals

- A display picker or display-configuration UI.
- Automatically choosing an audience display when the app starts.
- Moving windows when a display is connected or disconnected without an `X`
  command.
- Changing mirroring or extended-desktop settings owned by the operating system.
- Moving other application windows.
- Persisting monitor handles, names, coordinates, or topology across launches.
- Treating renderer cache resolution as part of the swap. Display-aware render
  tuning remains separate from placement.

## Ownership Boundary

Display discovery, swap planning, native window placement, verification, and
recovery belong in Rust. Slint adds one callback and the `X` key binding to each
window, and continues to own only presentation layout and event forwarding.

The operation is a window-management command, not a `PresentationCommand`.
Adding it to the page-navigation command path would incorrectly couple native
display availability to PDF and page state. A small controller under
`window_controller` should own the operation and accept the window role that
initiated it:

```rust
pub enum WindowRole {
    Presenter,
    Slide,
}

pub fn request_display_swap(
    windows: &AppWindowRefs,
    initiated_by: WindowRole,
    controller: &mut DisplaySwapController,
) -> DisplaySwapOutcome;
```

`DisplaySwapOutcome` should distinguish `Applied`, a safe `NoOp` reason, and a
failed or interrupted operation that requires presenter-facing recovery text.
The result must not contain or mutate `AppState` presentation fields.

## Native Backend

Slint 1.18 exposes position and fullscreen setters but does not expose monitor
enumeration or targeted fullscreen through its stable `Window` API. The
implementation should enable Slint's `unstable-winit-030` feature and contain all
use of `slint::winit_030::WinitWindowAccessor` in the Rust window-management
boundary.

The accessor supplies the native Winit windows needed for:

- `current_monitor()` and the current monitor handle;
- monitor origin, physical size, scale factor, and optional name;
- window outer position and size;
- physical window placement; and
- `Fullscreen::Borderless(Some(target_monitor))` for a targeted fullscreen
  transition.

Do not add a second independently versioned Winit dependency. Use the Winit API
re-exported by the exact Slint version so the window types cannot diverge. Treat
this feature as an intentionally narrow unstable dependency: only the adapter
module may expose Winit types, and a Slint upgrade must compile and manually
verify that boundary.

The normal packaged desktop builds use the Winit backend. If a different Slint
backend is selected or either native window is unavailable, return a safe no-op
instead of attempting a Slint-position-only fallback.

## Platform Capability

The controller must decide whether the complete two-window operation is
supported before moving either window.

| Platform/session | Initial behavior |
| --- | --- |
| macOS | Supported through Winit monitor and window APIs. |
| Windows | Supported through Winit monitor and window APIs. |
| Linux X11 | Supported through Winit monitor and window APIs. |
| Linux Wayland | Safe no-op for the first implementation because the compositor does not provide general top-level window placement. |
| Non-Winit Slint backend | Safe no-op. |

Targeted fullscreen support alone is not enough to claim swap support. If the
presenter window cannot also be placed on its destination, the controller must
not move the slide window. Log the technical reason and show a concise message
such as `Display switching is unavailable on this desktop.`

This limitation is preferable to a partial swap that unexpectedly exposes the
presenter view or leaves controls on an unreachable display.

## Snapshot and Pure Swap Plan

The native adapter should first collect immutable snapshots for both windows and
the current topology. A snapshot contains only immediate-operation data:

```rust
struct DisplaySnapshot {
    id: DisplayId,
    origin: PhysicalPosition,
    size: PhysicalSize,
    scale_factor: f64,
}

struct WindowSnapshot {
    role: WindowRole,
    display_id: DisplayId,
    outer_position: PhysicalPosition,
    outer_size: PhysicalSize,
    fullscreen: bool,
}
```

`DisplayId` is an application-owned ephemeral identifier used by the pure
planner and tests. Production code maps it to live Winit `MonitorHandle` values
only for the duration of one request. Monitor names are diagnostic metadata,
not identity: they can be absent or duplicated.

The pure planner returns either two complete target placements or a no-op. It
must reject:

- fewer than two usable displays;
- two windows on the same display;
- a missing or stale source display;
- an unavailable window position or monitor;
- an unsupported placement capability; and
- invalid or empty display geometry.

For a windowed window, map the center of the source window into the destination
display using its normalized source-display position. Preserve the logical
window size across differing scale factors. Clamp the resulting outer position
to keep the title/control region reachable; if the window is larger than the
destination, align it to the destination origin rather than producing an
off-screen coordinate.

Fullscreen placement ignores the source window rectangle and targets the whole
destination monitor.

## Apply Sequence

The UI thread performs one bounded state-machine operation:

```text
Idle -> Prepared -> Applying -> Verifying -> Idle
                       |             |
                       +--recover----+
```

1. Reject another `X` command while a swap is active. Do not queue repeated
   display moves behind a topology that may already be stale.
2. Capture both window snapshots and all live monitor handles without mutating a
   window.
3. Build the pure two-window plan. A no-op ends here.
4. Reconfirm that both target handles are still in the live topology.
5. Apply both placements on the UI thread:
   - request the presenter window's windowed destination position;
   - for a windowed slide, request its windowed destination position;
   - for an app-controlled fullscreen slide, retarget it with
     `Fullscreen::Borderless(Some(target_monitor))` without first clearing
     fullscreen.
6. Request focus for the initiating role after both placement requests have been
   issued.
7. Verify on a later event-loop turn that each window reports the planned target
   monitor and that the expected fullscreen state remains active.

Preparing the complete plan before step 5 prevents known precondition failures
from producing a one-sided move. Native window setters do not report compositor
acceptance synchronously, so verification remains necessary.

Do not hide either window during the normal sequence. Hiding the fullscreen
slide can flash the desktop or presenter content on the audience display. Direct
targeted borderless fullscreen is the preferred path specifically because it
does not intentionally exit fullscreen.

## Verification and Recovery

Verification should be bounded and event-loop-driven; it must not block the UI
thread. Use a small fixed retry schedule shared with tests rather than an
unbounded poll. The implementation can finish early as soon as both windows
report their intended displays.

Before each retry, enumerate the topology again. If both target displays still
exist but one move has not settled, repeat only the requested native placement.
If the topology changed or the retry budget expires:

1. stop applying the stale plan;
2. place each window on an available display when the backend permits it,
   preferring its current valid display and then the primary/first available
   display;
3. preserve the slide's app-controlled fullscreen boolean, black-screen state,
   and presentation state;
4. restore focus to the initiating window when it remains available; and
5. show `Displays changed; check presenter and slide placement.` in the
   presenter status area and log the detailed topology and failure reason.

Recovery is best-effort because a display can disappear between any two native
calls. It must never loop indefinitely, panic on an absent monitor, or replace a
valid current placement with an unverified stale coordinate.

## Fullscreen and Chrome Synchronization

`AppState::fullscreen` remains the authoritative app-controlled intent. A display
swap does not toggle that state and does not invoke the existing toggle command.
After a targeted fullscreen request, existing native window-state synchronization
must continue to report fullscreen as active.

The slide title-bar compensation remains zero while fullscreen and is recalculated
through the existing `sync_slide_chrome()` path after verification. Windowed
swaps also call that path after the new scale factor and native placement have
settled.

Native or desktop-environment fullscreen modes that Quick Presenter did not
initiate are outside the first implementation contract. They must not cause a
panic; when they cannot be retargeted without changing mode, return a no-op and
log the reason.

## Focus and Status

The Slint callback must identify whether the presenter or slide window received
`X`. Focus restoration uses the existing window focus helpers or a small
role-based extension of them. It runs after native placement requests and may be
repeated once during verification on platforms whose window manager changes
focus during a cross-display move.

Use the existing presenter status surface for failures and unsupported cases.
Status text is short and does not include monitor names or coordinates:

| Outcome | Presenter text |
| --- | --- |
| Same/only display | `Connect a second display before switching screens.` |
| Unsupported backend/session | `Display switching is unavailable on this desktop.` |
| Topology changed or verification failed | `Displays changed; check presenter and slide placement.` |

A successful swap needs no transient status message because the visual result is
immediate and overwriting rendering or black-screen status would be misleading.
Detailed monitor metadata and native errors belong in diagnostics only.

## Slint Changes

Add `swap-displays()` callbacks and unmodified `X` bindings to both exported
windows. The callbacks only forward the event to Rust. They do not inspect
screen geometry or mutate presentation properties.

Add a `Swap Displays` / `X` row to `KeyboardBindingsPanel`. If the fixed panel
cannot fit the extra row at the minimum presenter size, adjust its layout rather
than removing another presentation-critical binding.

Do not add a display picker or new persistent setting. A future menu action may
invoke the same Rust command, but it is not required for issue #384.

## Race and Failure Rules

| Situation | Required result |
| --- | --- |
| One usable display or both windows on one display | No-op; do not move either window. |
| Three or more displays | Swap only the two displays currently occupied by the Quick Presenter windows. |
| `X` pressed again while applying | Ignore the repeated request; do not queue it. |
| Either native window or current monitor unavailable | No-op before mutation. |
| Slide hidden and its monitor cannot be inspected | No-op; do not show it. |
| Slide is blacked out | Preserve black-screen state and pixels while moving the window. |
| Slide is app-controlled fullscreen | Retarget fullscreen directly; do not intentionally show windowed chrome. |
| Target disappears before apply | Abort the plan before mutation. |
| Target disappears during apply | Stop retries, recover to available displays, and warn the presenter. |
| One placement does not settle | Retry within the fixed budget, then recover and warn. |
| Backend cannot place presenter window | No-op; never move only the slide. |
| PDF open/render/reload is in progress | Move windows without cancelling or rescheduling PDF work. |

## Test Plan

Unit tests for the pure planner should cover:

- swapping two distinct display identities;
- preserving roles while exchanging only display assignments;
- normalized placement between equal and unequal display sizes;
- mixed scale factors while preserving logical size;
- clamping near every destination edge;
- a window larger than the destination;
- one display, same-display windows, missing snapshots, and invalid geometry;
- selecting only the two occupied displays from a three-display topology; and
- producing no partial plan for an unsupported backend.

Controller tests with a fake native adapter and fake event-loop ticks should
cover:

- both windowed placements applying and verifying;
- a fullscreen slide receiving a targeted fullscreen monitor without an exit;
- focus restoration for each initiating role;
- repeated `X` while active being ignored;
- delayed native placement followed by successful verification;
- topology change before apply;
- topology change and one-sided movement during verification;
- bounded retries and recovery; and
- presentation, fullscreen intent, black-screen, and render state remaining
  untouched.

Manual verification must use extended-desktop mode and include macOS, Windows,
Linux X11, and a documented Linux Wayland no-op:

- windowed presenter and windowed slide;
- fullscreen slide;
- `X` from each focused window;
- repeated swaps;
- different resolutions and scale factors;
- three displays;
- black screen and active page navigation;
- a hidden slide window;
- disconnecting a target immediately before or during the command; and
- verifying that the next keyboard navigation command works without a click.

The implementation PR must run `cargo fmt --check`, `cargo check`, and
`cargo test` after enabling the Slint Winit feature and adding the new Rust and
Slint code.

## Implementation Sequence

1. Add the pure topology/swap planner and unit tests.
2. Add the Winit adapter behind the window-management boundary and verify the
   packaged backend selection on all platforms.
3. Add the controller state machine, bounded verification, and fake-adapter
   tests.
4. Connect presenter and slide callbacks, `X` bindings, focus restoration, and
   short status messages.
5. Add the keyboard-help row and manual multi-display smoke coverage.

This order establishes complete, failure-safe planning before any keyboard
event can trigger native window movement.

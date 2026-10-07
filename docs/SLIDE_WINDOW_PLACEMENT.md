# Slide Window Placement Persistence

## Status and Scope

Proposed design for [issue #31](https://github.com/koichiro/quick-presenter/issues/31).
Remember the audience slide window's last usable windowed placement to reduce
setup work on the next launch. Reuse the Rust native-window boundary introduced
by [display swapping](DISPLAY_SWAP.md).

The feature saves one placement for the application, independent of the PDF.
Presenter placement, fullscreen intent, visibility, PDF paths, and presentation
state are outside the saved record. Startup remains windowed. There is no
display picker, persistent monitor identity, per-deck preference, or background
monitor-management service.

## Behavior Contract

- Restore a valid logical slide-window size and place the window near its last
  location when the saved display geometry still matches a live display.
- When the geometry is stale, select a currently available fallback display and
  clamp the window into it. Never submit the saved desktop coordinates blindly.
- Prefer the presenter's current usable display for fallback, then the primary
  display, then the first usable display. If discovery or placement is
  unsupported, leave placement to the existing startup path and window manager.
- Missing, invalid, or newer-version settings behave like a first launch.
- Remember the last settled windowed placement before fullscreen or hiding.
  Fullscreen dimensions, minimized geometry, and hidden-window artifacts must
  never replace the normal size.
- A successfully verified `X` swap updates the remembered slide destination.
  Failed, cancelled, and intermediate swaps do not save partial movement.
- Persistence failures do not prevent startup, PDF loading, navigation, or exit.
  Log a short diagnostic without opening a modal dialog or replacing presentation
  status text.

The first implementation saves on orderly application exit. A crash, forced
termination, or power loss can retain the previous run's placement. Continuous
disk writes during a presentation are unnecessary for this issue.

## Existing Integration Points

`AppWindows::apply_initial_positions()` currently sets fixed presenter and slide
positions. `initialize_slide_window_size()` initializes the slide to a fitted
16:9 size. Later, `fit_slide_window_to_aspect_ratio()` resizes the slide when the
initial PDF page is accepted. Restoration must integrate with all three paths;
otherwise opening a PDF immediately overwrites a restored size.

`window_controller.rs` already provides native snapshots, monitor enumeration,
Wayland capability detection, desktop-coordinate conversion, clamping, and
bounded display-swap verification. Extract only the small shared geometry helpers
needed by both operations. Keep live Winit handles inside this boundary.

## Ownership and Data

Add `src/window_placement.rs` for serializable records, validation, the pure
restore planner, and an in-memory `SlidePlacementController`. Native capture and
application stay in `window_controller.rs`. `main.rs` owns their lifetime and
connects startup, window events, visibility, fullscreen, swap completion, and
shutdown. Slint continues to expose dimensions and forward user events.

The version 1 JSON record contains:

| Field | Meaning |
| --- | --- |
| `version` | Schema version, initially 1. |
| `platform` | Coordinate interpretation for macOS, Windows, or Linux X11. |
| `source_display` | Display origin, extent, and actual scale factor at capture. |
| `relative_center` | Window outer center normalized within that display. |
| `logical_inner_size` | Client width and height in logical pixels. |

The source rectangle is a conservative geometry hint, not monitor identity.
Do not persist monitor names, enumeration indices, Winit handles, native IDs,
or an entire display topology. Two displays exchanging physical identities while
retaining the same desktop rectangles cannot be detected by this design.

Use macOS logical desktop points for origins and extents, following the existing
`desktop_point()` and `set_desktop_position()` convention. Windows and X11 use
physical desktop coordinates for origins and extents. Save the platform tag to
reject incompatible records copied between platforms. Store client size in
logical pixels on every platform; scale it using the destination's live scale.
Negative desktop origins are valid. Clamp the normalized center to `[0, 1]`
when capturing a partially visible or oversized window; restoration prioritizes
reachability over reproducing an out-of-bounds center.

Validate before planning: finite numbers, positive sizes and scale, centers in
`[0, 1]`, supported platform and schema, and checked conversions to native integer
coordinates. Limit the file read to 16 KiB. Invalid records are ignored as a
whole; do not reinterpret incomplete data as zero coordinates.

## Restore Planning and Application

1. Load settings before showing windows. Apply the existing default size and
   positions provisionally. Keep capture disabled during initialization.
2. After the slide's native window exists and chrome is configured, capture the
   live display topology and frame insets. On Linux, run this step inside the
   deferred initial-show path, rather than before the event loop creates windows.
3. Match the saved source rectangle and scale to one live display. Integer
   rectangles must match; compare scale with a small numerical tolerance of
   `0.001`. If the match is absent or ambiguous, use the fallback display order
   above. Do not guess identity from a monitor name or nearest stale coordinate.
4. Clamp client size to live display bounds after accounting for frame insets and
   the Slint minimum of 640 by 360 logical pixels. Preserve logical size when it
   fits. If the display cannot contain the minimum, keep the minimum and align
   the outer window to the display origin so controls remain reachable.
5. On a matching display, reconstruct the outer center from the normalized
   anchor, subtract half the planned outer size, and clamp to that display.
   For fallback, center the window in the chosen display and clamp it. Use widened
   arithmetic before checked conversion to native positions.
6. Reconfirm the target is available, then apply client size and outer position
   on the UI thread. Update both Slint dimension properties and the native size
   through one shared sizing function to avoid competing preferred sizes.
7. Verify on later event-loop turns using the existing bounded swap delay
   schedule. Read actual native size after resize requests settle, recompute the
   clamp for actual frame geometry, and allow only bounded corrective moves.
   Success requires a reachable control region on the intended live display;
   a nearest-monitor handle alone is insufficient.
8. If the target disappears or verification expires, stop the stale plan and
   attempt one placement on a newly available fallback display. If that cannot
   be confirmed, stop moving and retain window-manager placement. Enable normal
   capture only after the startup operation terminates.

When no settings exist on a supported backend, validate the provisional default
rectangle against the live topology and clamp it to the fallback display if
necessary. This also covers desktops whose origin is no longer `(0, 0)`.

Keep restoration within a controller generation. Manual move/resize input,
fullscreen, hiding, and `X` cancel any remaining startup verification before
performing their action. Delayed callbacks check their generation before moves
or completion. Native move notifications alone cannot reliably distinguish user
drags from requested moves, so compare observed geometry against the pending
plan and cancel on conflicting movement; conservatively prefer user control.
Never restore focus to the slide as a side effect. Preserve the current startup
ordering that brings the presenter forward.

## PDF Sizing Precedence

A valid saved size becomes the run's initial size preference, including when its
display requires fallback. Automatic fitting on PDF acceptance must respect that
preference: skip `fit_slide_window_to_aspect_ratio()` for that run. `SlideView`
already fits the page within the viewport, so a different PDF aspect ratio can
produce margins without losing page content.

When no valid size was restored, retain the current PDF fitting behavior. Keep
this decision in the placement controller, outside PDF session state. Deferred
restoration and the first PDF render must consult the same flag so their order
cannot determine the final size. Subsequent manual resizing is captured normally.

## Capture and Display Swap

Register one slide `on_winit_window_event()` observer for moved, resized, and
scale-factor changes. Return `EventResult::Propagate` for every event. Coalesce
notifications into a deferred snapshot after 250 ms of inactivity; capture
actual native geometry rather than the raw event payload. These snapshots update
memory only. Require a visible, non-minimized, windowed slide, valid display
geometry, reachable controls, and no active placement transaction.

Capture synchronously before entering fullscreen, hiding, or requesting orderly
quit when the native window is available. If it is unavailable, retain the last
good in-memory record. The shutdown writer must not rely on accessing a native
window after `run_event_loop()` has returned.

Add a verified-completion hook to `DisplaySwapController`; its immediate
`Applied` outcome currently means requests were submitted, not that both moves
settled. Suspend normal capture from preparation through verification/recovery.

For a successful windowed swap, sample the settled slide rectangle. For a
successful fullscreen swap, preserve the last known logical windowed size and
normalized center, and replace its source-display hint with the verified
destination. Clamp the virtual windowed placement there for the next launch.
If no valid windowed size was captured, use the validated default size. Never
save fullscreen bounds as the windowed size. On failed or cancelled swaps,
retain the previous record and resume capture only after normal windowed
geometry becomes stable. Fullscreen exit captures the actual resulting windowed
placement once it settles.

Hide/show during a run retains the current native placement. Validate reachability
on show and use the same fallback planner if displays changed while hidden; do
not replay startup settings or start persistent monitor polling.

## Storage and Failure Handling

Store `slide-window-placement.json` beside `recent-files.txt` in the existing
platform configuration directory. Extract the configuration-directory resolver
and atomic byte writer from `recent.rs` into a small shared storage module;
preserve recent-file behavior and its existing tests. Reuse same-directory temp
files, Unix mode `0600`, Windows replacement semantics, and temp-file cleanup.
Existing `serde` and `serde_json` dependencies suffice.

Read once at GUI startup and write at most once after the GUI event loop ends.
Skip the write if no valid record was captured or the record is unchanged.
Keep writes out of UI/native callbacks. A write failure preserves the old file
and does not change the application exit result. Simultaneous instances use
last-successful-writer semantics; no lock or merge policy is needed for one
placement. Helper, headless smoke, and GUI smoke runs must not touch user settings;
tests inject a temporary store path.

Missing settings are silent. Invalid settings, unsupported versions, and IO
failures produce bounded diagnostics. A stale display is ordinary fallback,
with no modal warning. Remove the settings file while the application is closed
to reset placement. No new settings UI is required.

## Platform Support

| Backend or session | Version 1 behavior |
| --- | --- |
| macOS Winit | Capture and restore using the existing logical desktop conversion and slide chrome synchronization. |
| Windows Winit | Capture and restore using physical desktop coordinates and destination scale. |
| Linux X11 Winit | Capture and restore after deferred native-window creation. |
| Linux Wayland | Keep compositor placement; ignore the loaded record and do not apply or overwrite placement settings in this version. |
| Other Slint backends | Keep existing startup behavior and preserve existing settings. |

Work-area APIs for excluding docks and taskbars are outside the initial scope.
Clamp to monitor rectangles and verify the existing reachable-control predicate;
manual checks must confirm title controls remain usable with normal desktop
chrome on supported platforms.

## Validation and Delivery

Unit tests should cover record round trips, bounded reads, corrupt and unknown
schemas, incompatible platforms, invalid numeric values, negative origins,
permission and write failures, and unchanged-record write suppression.

Pure planner tests should cover unchanged geometry, removed or rearranged
displays, changed resolution/DPI, mixed scales, fallback ordering, ambiguous
matches, edge clamping, oversized windows, minimum-size constraints, no usable
topology, and integer overflow. Controller tests with fake snapshots and ticks
should cover deferred native availability, resize settling, topology changes,
retry exhaustion, conflicting user movement, cancelled generations, hidden and
minimized windows, fullscreen capture suppression, and verified fullscreen/windowed
swap updates. Test both orderings of initial PDF render versus restoration and
ensure page, timer, notes, black screen, and fullscreen intent remain unchanged.

Manual smoke checks on macOS, Windows, and X11:

1. Move and resize the slide onto a second display, quit normally, and relaunch
   with and without a startup PDF. Confirm position, size, and presenter focus.
2. Repeat with different PDF aspect ratios and mixed display scale factors.
3. Quit while fullscreen, including after `X`; confirm the next launch is
   windowed on the remembered destination with normal windowed dimensions.
4. Disconnect or rearrange displays between runs and during startup verification.
   Confirm an available, reachable placement and working shortcuts.
5. Hide the slide, change displays, show it, and quit; confirm no hidden or
   intermediate geometry is persisted.
6. Test corrupt settings, an unwritable directory, very small displays, taskbars,
   docks, and a Wayland session preserving a pre-existing X11 record.

Implement in three stages: shared storage and pure planning; native startup and
capture integration with PDF size precedence; then verified swap/fullscreen and
hide/show integration. Each stage includes its corresponding unit tests.

The implementation must pass `cargo fmt --check`, `cargo check`, and `cargo test`.
Update `README.md`, `PRIVACY.md`, `FULLSCREEN.md`, and the GUI smoke checklist to
describe the delivered behavior, storage/reset path, and platform limitations.
This proposal does not claim that persistence is already implemented.

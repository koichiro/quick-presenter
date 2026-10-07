# Slide Window and PDF Startup Restoration

## Status and Scope

Proposed design for [issue #31](https://github.com/koichiro/quick-presenter/issues/31).
Remember the audience slide window's last usable windowed placement and reopen
the PDF that was active at orderly exit. Restore both on the next normal launch
to reduce presentation setup work. Reuse the Rust native-window boundary
introduced by [display swapping](DISPLAY_SWAP.md) and the existing asynchronous
PDF open pipeline.

The feature saves one placement and one last-active PDF path for the application.
Placement remains independent of the PDF. Presenter placement, fullscreen intent,
visibility, page index, timer, notes, and black-screen state are outside the saved
record. Startup remains windowed and a reopened PDF starts at its first page.
There is no display picker, persistent monitor identity, per-deck preference, or
background monitor-management service.

## Behavior Contract

- Restore a valid logical slide-window size and place the window near its last
  location when the saved display geometry still matches a live display.
- When the geometry is stale, select a currently available fallback display and
  clamp the window into it. Never submit the saved desktop coordinates blindly.
- Prefer the presenter's current usable display for fallback, then the primary
  display, then the first usable display. If discovery or placement is
  unsupported, leave placement to the existing startup path and window manager.
- Missing, invalid, or newer-version settings behave like a first launch.
- Without an explicit startup PDF, automatically reopen the PDF that was active
  at orderly exit. An explicit `--pdf` or positional PDF path takes precedence.
- If no PDF was active at exit, save an empty last-PDF value. Failed or pending
  replacement opens must never become the remembered PDF.
- A missing, inaccessible, or invalid remembered PDF leaves the app open with
  a short presenter-facing error and the normal file-opening controls available.
- Smoke modes open only their explicitly supplied test PDF and bypass startup
  restoration entirely, including settings reads, placement, and exit-time saves.
- Remember the last settled windowed placement before fullscreen or hiding.
  Fullscreen dimensions, minimized geometry, and hidden-window artifacts must
  never replace the normal size.
- A successfully verified `X` swap updates the remembered slide destination.
  Failed, cancelled, and intermediate swaps do not save partial movement.
- Persistence failures do not prevent startup, PDF loading, navigation, or exit.
  Log a short diagnostic without opening a modal dialog or replacing presentation
  status text.

The first implementation saves on orderly application exit. A crash, forced
termination, or power loss can retain the previous run's placement and PDF path.
Continuous disk writes during a presentation are unnecessary for this issue.

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
shutdown. Add a small `src/startup_state.rs` boundary for the shared settings
envelope, store, and pure startup-PDF selection. PDF selection must not depend on
native placement support. Track the committed active PDF path in Rust session
state; placement capture must not own or inspect a native PDF document. Slint
continues to expose dimensions and forward user events.

The version 1 JSON envelope contains:

| Field | Meaning |
| --- | --- |
| `version` | Schema version, initially 1. |
| `window_placement` | Optional placement record; absence retains default sizing and placement. |
| `last_pdf` | Optional absolute native PDF path and its platform/encoding tag; null means no active PDF. |

The optional placement record contains:

| Field | Meaning |
| --- | --- |
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
coordinates. Limit the settings read to 256 KiB, including native path encoding.
Reject malformed JSON or an unknown envelope version as a whole. Validate the
placement and last-PDF sections independently: invalid geometry must not prevent
a valid PDF from reopening, and an invalid path must not prevent valid placement
restoration. Do not reinterpret incomplete geometry as zero coordinates.

## Restore Planning and Application

1. For a normal GUI launch, load settings before showing windows. Apply the
   existing default size and positions provisionally. Keep capture disabled
   during initialization.
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

## Automatic PDF Reopening

Classify the execution mode before accessing user settings. For a normal GUI
launch, load settings and choose exactly one startup PDF in Rust:

| Request | PDF to open |
| --- | --- |
| Normal GUI launch with `--pdf` or a positional PDF path | The explicitly requested PDF, even if opening it fails. |
| Normal GUI launch without an explicit PDF | The validated saved `last_pdf`, if present. |
| No explicit or saved PDF | Start without a document. |
| Help, renderer helper, headless smoke, or GUI smoke | Keep existing behavior and bypass user startup settings. |

Use `load_startup_pdf()` and the existing scheduler/helper open and render path
for both explicit and remembered PDFs. Schedule the open after showing the
windows; IO, PDFium, sandboxing, operation deadlines, and errors retain their
existing boundaries. Do not synchronously probe or parse the PDF on the UI
thread. Reopen the current file contents at that path, rather than storing a
copy, cached pixels, file identity, or an older PDF version.

A reopen is a new session: show the first page, reset the timer, and start with
black screen and fullscreen disabled. Placement restoration proceeds independently
of PDF opening and cannot block it. The saved-size flag must be available before
scheduling either operation. A manual open during startup supersedes the automatic
open through the existing session IDs; late automatic-open results must not
replace the user's selected deck.

Capture the path only when a PDF session is successfully committed, using
`OpenedSessionOutcome::loaded_path` and the corresponding active session path.
Resolve relative input against the working directory of that open and store an
absolute path. Preserve native path data without lossy conversion: use a UTF-8
string for representable paths, or tagged Unix byte/Windows UTF-16 arrays for
other paths. Validate encoding, platform, absolute-path semantics, and absence
of NUL before selection. The path platform tag identifies the operating system,
not X11 versus Wayland. Paths copied between operating systems are ignored.
Do not use the most recent file list or `pending_open` as the source of truth.
A watcher failure must not erase a successfully committed PDF path.

At shutdown, snapshot the path of the committed active document alongside the
placement snapshot. If deck A is active while deck B is still opening or failed
to open, save A. If nothing was committed in this run, save `last_pdf: null`,
including after an automatic reopen failure. Thus a missing remembered file is
not retried on every future launch once a subsequent orderly save succeeds.
If the user opens another PDF after the failure, save that successfully committed
PDF instead. Successful hot reload keeps the same path; it does not save the
current page.

Attempt the remembered open once using the existing bounded open pipeline; no
extra retry loop, file search, or fallback to another recent PDF is added.
Report a concise message such as `Could not reopen the previous PDF. Open a PDF
to continue.` through the existing presenter error surface, with technical
details in diagnostics. Explicit startup failures retain existing explicit-open
error handling and must not silently open the saved PDF instead.

## Smoke Mode Isolation

Both `--smoke-open-pdf <PATH>` and `--gui-smoke <PATH>` disable the entire startup
restoration feature for that process. Determine this from parsed startup options,
before constructing the startup store, resolving its configuration path, or
reading settings. Preserve the existing renderer-helper dispatch before normal
CLI parsing and the existing help and CLI error paths.

| Options | Restoration behavior |
| --- | --- |
| `--smoke-open-pdf <PATH>` | Open/render only the test PDF; no remembered-PDF or placement restoration. |
| `--gui-smoke <PATH>` | Use only the test PDF and the smoke harness's own window setup. |
| `--gui-smoke <PATH> --gui-smoke-report <PATH>` | Remain in GUI smoke mode; report output does not enable restoration. |
| Either smoke mode with `--log-file <PATH>` | Remain in smoke mode; diagnostic output does not enable restoration. |
| `--gui-smoke-report` without `--gui-smoke`, or conflicting smoke/PDF options | Keep the current CLI error; do not fall through to a normal restoring launch. |
| `--log-file <PATH>` without a smoke mode | Normal GUI rules apply; a log destination alone is not a smoke request. |

Smoke mode must not load or save `startup-state.json`, reopen a remembered PDF,
use the saved-size preference, install restoration/capture timers or window
observers, or register an exit-time startup-state writer. It must not update the
user's recent-file list through automatic reopening. Ordinary exit and all smoke
failure paths preserve user settings. Keep the harness's existing page, size,
focus, report, error, and exit-code behavior; a failed test PDF must never trigger
a remembered-PDF fallback or extra restoration work.

Keep this policy centralized in a pure execution-mode decision shared by startup
selection and shutdown wiring. Disabling only PDF selection is insufficient:
otherwise restored geometry or a save of the test PDF could affect smoke results
or the user's next launch. Restoration tests may inject temporary stores directly
through unit/controller test boundaries; production smoke options remain isolated
even when settings exist.

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

Store `startup-state.json` beside `recent-files.txt` in the existing
platform configuration directory. Extract the configuration-directory resolver
and atomic byte writer from `recent.rs` into a small shared storage module;
preserve recent-file behavior and its existing tests. Reuse same-directory temp
files, Unix mode `0600`, Windows replacement semantics, and temp-file cleanup.
Existing `serde` and `serde_json` dependencies suffice.

For a normal GUI run, read once at startup and write at most once after the GUI
event loop ends. Smoke runs never construct this store or register its writer.
Save placement and last-PDF fields together in one atomic replacement. Skip only
when the complete normalized record is unchanged. A null last-PDF value is a real
update and must be written even if no new placement could be captured. On a
backend without placement support, preserve any previously valid placement
section while updating the PDF field; otherwise use null for absent placement.
Keep writes out of UI/native callbacks. A write failure preserves the old file
and does not change the application exit result. Simultaneous instances use
last-successful-writer semantics; no lock or merge policy is needed for this
single startup record. Helper, headless smoke, and GUI smoke runs must not touch
user settings; tests inject a temporary store path.

Missing settings are silent. Invalid settings, unsupported versions, and IO
failures produce bounded diagnostics. A stale display is ordinary fallback,
with no modal warning. Remove the settings file while the application is closed
to reset placement and automatic PDF reopening. The saved path is local user
data and can reveal deck and directory names; document it in `PRIVACY.md` with
its location and reset procedure. `Clear Recent Files` clears only the separate
recent-file list; it does not erase the active PDF or this startup record. No
new settings UI is required.

## Platform Support

| Backend or session | Version 1 behavior |
| --- | --- |
| macOS Winit | Capture and restore using the existing logical desktop conversion and slide chrome synchronization. |
| Windows Winit | Capture and restore using physical desktop coordinates and destination scale. |
| Linux X11 Winit | Capture and restore after deferred native-window creation. |
| Linux Wayland | Keep compositor placement and preserve its saved section; reopen and save the last PDF normally. |
| Other Slint backends | Keep existing window placement and preserve its saved section; reopen and save the last PDF normally. |

Work-area APIs for excluding docks and taskbars are outside the initial scope.
Clamp to monitor rectangles and verify the existing reachable-control predicate;
manual checks must confirm title controls remain usable with normal desktop
chrome on supported platforms.

## Validation and Delivery

Unit tests should cover record round trips, bounded reads, corrupt and unknown
schemas, incompatible platforms, invalid numeric values, negative origins,
permission and write failures, and unchanged-record write suppression. Include
independent validation of placement and PDF fields, native path round trips,
relative-to-absolute path capture, and null-PDF saves without new placement.

Pure planner tests should cover unchanged geometry, removed or rearranged
displays, changed resolution/DPI, mixed scales, fallback ordering, ambiguous
matches, edge clamping, oversized windows, minimum-size constraints, no usable
topology, and integer overflow. Controller tests with fake snapshots and ticks
should cover deferred native availability, resize settling, topology changes,
retry exhaustion, conflicting user movement, cancelled generations, hidden and
minimized windows, fullscreen capture suppression, and verified fullscreen/windowed
swap updates. Test both orderings of initial PDF render versus restoration and
ensure page, timer, notes, black screen, and fullscreen intent remain unchanged.

Startup/session tests should cover explicit PDF precedence (including explicit
failure), no saved PDF, one remembered-open request, missing/inaccessible/invalid
PDF failures, first-page/timer/fullscreen defaults, and a manual open superseding
a pending restore. Cover A active with B pending or failed at exit, watcher
failure after commit, hot reload retaining the path, no active PDF clearing the
saved value, and smoke/helper modes leaving user settings untouched. Verify PDF
reopening works even when placement is unsupported or invalid.

Smoke isolation tests should cover both smoke options, GUI reports, diagnostic
output options, and CLI errors for invalid combinations. Use a fake startup store
to assert zero reads and zero writes, zero remembered-open requests, no saved-size
override, and no restoration timers on success and failure. Prepopulate temporary
user settings with valid, corrupt, missing-PDF, and inaccessible-file cases;
assert the requested smoke PDF, harness geometry, reports, and exit codes are
unaffected and settings bytes remain unchanged after exit. Include a normal GUI
launch with only `--log-file` to confirm that restoration still works there.

Manual smoke checks on macOS, Windows, and X11:

1. Move and resize the slide onto a second display, quit normally, and relaunch
   without arguments. Confirm the same PDF reopens at its first page with the
   remembered window size/position and presenter focus. Repeat with an explicit
   different PDF and confirm it takes precedence.
2. Repeat with different PDF aspect ratios and mixed display scale factors.
3. Quit while fullscreen, including after `X`; confirm the next launch is
   windowed on the remembered destination with normal windowed dimensions.
4. Disconnect or rearrange displays between runs and during startup verification.
   Confirm an available, reachable placement and working shortcuts.
5. Hide the slide, change displays, show it, and quit; confirm no hidden or
   intermediate geometry is persisted.
6. Test corrupt settings, an unwritable directory, very small displays, taskbars,
   docks, and a Wayland session reopening the PDF while preserving X11 placement.
7. Delete, move, or deny access to the remembered PDF; confirm a usable empty app
   with a short error. Open another deck, quit, and confirm that deck reopens.
8. Quit with no committed document or while a replacement PDF is pending; confirm
   the saved PDF is null or the previous active deck respectively.
9. Save a PDF and unusual slide size in normal mode, then run each smoke mode
   against a different PDF, including report/log options and a failing test PDF.
   Confirm only the test PDF is attempted, harness geometry is unchanged, and the
   following normal launch still restores the original PDF and placement.

Implement in three stages: shared startup storage, pure placement planning, and
PDF selection; native startup/capture and asynchronous PDF reopening with size
precedence; then verified swap/fullscreen, hide/show, and orderly-exit integration.
Each stage includes its corresponding unit tests.

The implementation must pass `cargo fmt --check`, `cargo check`, and `cargo test`.
Update `README.md`, `PRIVACY.md`, `FULLSCREEN.md`, and the GUI smoke checklist to
describe the delivered behavior, storage/reset path, and platform limitations.
This proposal does not claim that persistence is already implemented.

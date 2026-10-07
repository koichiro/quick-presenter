# Issue 271: display-aware rendering design and validation

Validated on October 7, 2026, against commit `694aadc` (app version 1.5.0),
on macOS 26.7 (25G229), Apple Silicon.

Issue: [Design display-aware render cache tuning](https://github.com/koichiro/quick-presenter/issues/271).
This is a design and baseline verification report. Display-aware rendering is
not implemented in this revision; the existing 1600/600/180 px widths and
64-entry / 96 MiB default remain in effect.

## Decision

Adopt the bounded physical-width policy below as the design for a future
display-aware implementation. Make only current-slide renders display-aware;
keep preview and thumbnail widths fixed. Derive the byte budget from the selected
width, and require exact-request event handling and width-aware eviction before
enabling the policy. Keep the current fixed policy until that implementation and
its tests are delivered. This completes the design scope of #271 without changing
the fixed-budget decision from #258.

The choice trades some large-screen sharpness (a 2560 px ceiling) and some
letterbox over-rendering for bounded buffers and a small platform boundary. The
initial polling/stabilization interval is a design starting point; implementation
must measure resize responsiveness and visible-render latency before enabling it.

## Chosen sizing and platform boundary

Use a Rust adapter near `window_controller` to read the slide window's client
size and scale factor. Slint 1.18.1's `Window::size()` returns **physical** pixels,
excluding the native frame. `Window::scale_factor()` converts logical pixels to
physical pixels; do not multiply the result of `size()` by it again. The scale
factor is useful diagnostic information and for converting any logical surface
insets. Monitor desktop coordinates used by placement/display swapping must not
be reused as render dimensions: their macOS normalization serves a different
purpose.

The adapter should expose physical slide-surface dimensions, excluding any
application-owned titlebar compensation area. A pure `RenderSizingPolicy`
selects render widths and the cache budget. Slint supplies layout and events;
Rust owns selection, scheduling, and cache changes.

For the initial policy, use physical surface width as a conservative target:

```text
bounded = clamp(physical_surface_width, 1600, 2560)
current_width = 1600 if bounded == 1600
                else min(2560, ceil(bounded / 128) * 128)
preview_width = 600
thumbnail_width = 180
```

Preserve exactly 1600 when the surface width is at or below 1600. Above that
floor, the first bucket is 1664. Upward rounding avoids undersampling within a
bucket. Bucket stability applies only within a bucket; small changes across a
boundary can still change width. Poll at a modest interval (proposed: 250 ms),
and require the same candidate on two consecutive samples before scheduling a
change. Coalesce superseded requests instead of rendering every resize sample.
Invalid, zero-sized, or hidden-window samples keep the last valid policy; start
at 1600 until a valid sample exists. A scale change with unchanged physical
surface width needs no new render.

This intentionally uses the full surface width rather than the width of the
letterboxed image. It can over-render portrait slides on wide surfaces. A later
fitted-image policy would need page aspect ratio and surface height, but is not
required for the first implementation.

| Scenario (illustrative, not measured hardware) | Physical surface width | Selected width |
| --- | ---: | ---: |
| Windowed, scale 1, 1024 logical px wide | 1024 | 1600 |
| Windowed, scale 2, 1024 logical px wide | 2048 | 2048 |
| High-DPI, scale 2, 1280 logical px wide | 2560 | 2560 |
| External 4K surface | 3840 | 2560 |
| Resize within one bucket | 1921 to 2047 | 2048 in both cases |
| Resize across a bucket boundary | 2048 to 2049 | 2048 to 2176 after stabilization |

Normal DPI alone does not imply 1600: a large scale-1 fullscreen surface also
selects a larger width. Select by physical demand, not monitor classification.

## Cache budget verification

Use integer arithmetic and checked/saturating operations:

```text
rgba_bytes(w) = 4 * w * ceil(3 * w / 4)
working_set = 5 * rgba_bytes(current_width)
            + rgba_bytes(600)
            + 17 * rgba_bytes(180)
max_entries = 64
max_estimated_bytes = clamp(ceil(working_set * 3 / 2), 96 MiB, 192 MiB)
```

The 4:3 aspect ratio is conservative relative to typical 16:9 slides, not all
PDFs. Actual cache accounting must continue to use rendered pixel dimensions.
Portrait or unusually tall pages can require eviction inside the preload radius.
The byte limit describes cache-owned RGBA data, not whole-process RSS: queued
pixels, last-good images, UI/GPU references, PDFium, and helper processes also
consume memory.

The following values were recalculated with integer pixel dimensions:

| Current width | One 4:3 page (bytes) | Working set (bytes) | Working set (MiB) | Selected budget (bytes) | Budget (MiB) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1600 | 7,680,000 | 41,132,400 | 39.227 | 100,663,296 | 96.000 |
| 1664 | 8,306,688 | 44,265,840 | 42.215 | 100,663,296 | 96.000 |
| 1920 | 11,059,200 | 58,028,400 | 55.340 | 100,663,296 | 96.000 |
| 2048 | 12,582,912 | 65,646,960 | 62.606 | 100,663,296 | 96.000 |
| 2304 | 15,925,248 | 82,358,640 | 78.543 | 123,537,960 | 117.815 |
| 2560 | 19,660,800 | 101,036,400 | 96.356 | 151,554,600 | 144.534 |

The 192 MiB ceiling is not reached by this initial policy. At 2560, the working
set slightly exceeds the old 96 MiB budget, supporting a width-dependent budget
when display-aware rendering is introduced. These are buffer estimates, not
render-latency measurements or evidence that the current fixed policy is unsafe.

## Integration findings and required safeguards

1. **Width-aware eviction is required.** `RenderRequest.width` is already part
   of the key, but `is_protected_request()` and `is_visible_current_request()` in
   `rendering.rs` ignore it. All widths of the visible page are currently exempt
   from final eviction. Keeping all nine proposed widths of one 4:3 page would
   consume 116,797,440 bytes, exceeding the lower 96 MiB budget. Extend
   `CacheContext` with the desired current-slide width; protect that width inside
   the preload radius and exempt only the exact desired visible request. Old
   widths must become ordinary eviction candidates. Preserve the last-good image
   separately during replacement, without granting every old width exemption.
   Thumbnails and previews should still be evicted before protected slides.

2. **Both success and failure events need exact-request checks.**
   `commit_page_rendered_state()` currently accepts any current-purpose render
   for the visible page as `last_good_current`. `commit_page_render_failed_state()`
   likewise reports failure without checking width. Compare with the desired
   visible request as well as the active session. Old-width results may enter
   the bounded cache but must not replace the displayed desired-width result;
   old-width failures must not mark the current request failed or change status.
   Keep a valid last-good image while the new request is pending or fails.

3. **Thread the policy through every current-slide path.** Render plans,
   preload plans, view lookups, initial open, prepared hot reload, and smoke
   assertions currently use fixed width assumptions. In particular,
   `RenderEvent::ReloadPrepared` does not identify its render width; add width
   metadata before caching that result under a variable-width request. A metrics
   change during open/reload must schedule the newly desired visible request
   without invalidating document-session ownership.

4. **Update budgets without clearing the cache.** Add a budget-update operation
   that enforces the new limit with the new width-aware context immediately,
   including when shrinking from 2560 to 1600. Retain useful old-width entries
   only while space permits. Coalesce obsolete queued widths, prioritize the
   desired visible page, and avoid blanking the audience image on a monitor move
   or fullscreen transition. Existing pending-work and pending-pixel limits
   still apply; cache budgeting must not replace them.

5. **Respect renderer geometry limits.** `renderer_limits::render_dimensions()`
   caps each dimension at 4096. A width of 2560 is valid for 16:9 and 4:3, but
   fails for sufficiently tall pages (aspect ratio below approximately 0.625).
   Before requesting an upgrade, validate the page geometry. If the selected
   width fails while the baseline width succeeds, retain/use the baseline
   request and its last-good image, and do not retry the rejected upgrade every
   poll. Pages invalid even at baseline keep the existing error path. Width
   alone is not proof that a request fits the helper's allocation limits.

## Verification performed

- `cargo fmt --check`: passed.
- `cargo check`: passed.
- `cargo test --quiet`: 387 unit tests and 13 integration tests passed.
- Reviewed the installed Slint 1.18.1 API source for physical size and scale-factor
  semantics, plus render planning, cache retention/eviction, view synchronization,
  session completion/failure, reload events, and renderer geometry limits.
- Recalculated the table above using `4 * width * ceil(3 * width / 4)` and the
  proposed 5/1/17 working set, with 1 MiB = 1,048,576 bytes.

The existing `macos_renderer.rs` atomic `fetch_update` deprecation warning remains.
No production code or default cache policy changed. Existing tests verify the
fixed-width baseline; they do not verify the proposed policy or its safeguards.

## Tests required with implementation

- Normal window, high-DPI, 4K clamping, exact 1600 floor, upward bucket rounding,
  same-bucket resize, boundary oscillation, invalid/hidden samples, and avoiding
  double scaling.
- All selected widths produce monotonic budgets within 96–192 MiB; exact values
  at 1600 and 2560 match the table.
- Thumbnail/preview bursts preserve desired-width nearby slides; stale widths
  remain evictable, including multiple widths of the visible page.
- Shrinking a budget enforces it immediately; the single oversized desired
  visible image remains the explicit exception.
- Old-width success/failure after new-width success, navigation during a pending
  upgrade, and open/hot reload during a metrics change preserve the desired image
  and correct status.
- Tall-page geometry falls back without repeated failing upgrade requests;
  requests invalid at baseline still report the existing error.

## External-display desktop verification

Performed interactive baseline verification on October 7, 2026, using the
Developer ID signed debug app staged for commit `694aadc` at
`/private/tmp/qp-macos-pr397/Quick Presenter.app`. Its executable SHA-256 is
`29e139b65d9d2a2e1c128adfd86f069b82008b3e4f469b793f0426b8bc5b9cf5`.
Current `main` commit `cfc22ca` contains the same application source after the
PR 397 squash merge. The test app is not a release distribution artifact.

An unrestricted `system_profiler SPDisplaysDataType` check identified a single
online DELL U4021QW external display: physical resolution 5120 × 2160, UI size
5120 × 2160 at 60 Hz, main display, mirroring off (scale 1). The earlier
sandboxed query exposed only the GPU; that omission was an observation-access
limitation, not evidence that no external display was connected.

| Interactive check | Observed result |
| --- | --- |
| Open `docs/samples/quick-presenter-demo.pdf` through the native dialog | Page 1/8, current image, next preview, notes, and thumbnails were visible; status was Ready. |
| Advance to page 2 and enter fullscreen | Presenter showed 2/8 and Exit Fullscreen. Bringing the slide window forward showed page 2 across the external display with side letterboxing and the correct 16:9 ratio. |
| From the fullscreen slide, press Right, Right, Left, Left, then Escape | The windowed audience image returned to page 2; presenter also showed 2/8 and the Fullscreen button. No failure status or placeholder was present at the observed endpoints. |
| Open `tests/fixtures/latex-beamer.pdf` and advance | Page 1/2 and next preview appeared; page 2/2 showed the expected speaker note and No next slide. This fixture is 16:9, as specified by its TeX source. |
| Quit normally | App exited; recent-file and startup settings were restored from the pre-test backup. |

The audience image was visibly enlarged from the fixed 1600 px render on the
5120 × 2160 surface. Text remained legible in the captured output, with softened
edges consistent with upscaling. This supports investigating higher render
widths, but does not quantify a quality improvement or validate the proposed
2560 px policy. A surface-width implementation would select 2560 here; even at
that ceiling, fullscreen output on this ultrawide display would still upscale.

The same signed app's semi-automated GUI smoke ran against the demo on this
display: **39 passed, 0 failed**, successful exit.
There was one transient ScreenCaptureKit observation error during fullscreen
entry; a subsequent observation succeeded and confirmed the resulting state.
Continuous frames, latency, RSS, and cache counters were not measured.

The earlier [PR 397 macOS validation](PR397_MACOS_STARTUP_RESTORATION.md) records
a 5120 × 2160, scale-1 external display and successful fixed-policy fullscreen
and GUI checks. That is useful baseline evidence, but did not exercise mixed
scale factors or the proposed render-width changes.

## Implementation verification procedure

After implementation, record artifact/commit, OS, display physical and logical
sizes, scale factors, PDF, selected width, cache bytes, pending pixel bytes,
visible-render latency, and observed output for each check:

| Desktop check | Expected observation | Status |
| --- | --- | --- |
| Normal-DPI window, 16:9 and 4:3 decks | Baseline width on small surfaces; correct image and navigation | Pending |
| High-DPI or external display | Width follows physical surface up to 2560; readable output | Pending |
| Fullscreen enter/exit | Stable bucket after transition; last-good image stays visible | Pending |
| Move between displays with different scales | No double scaling, obsolete requests coalesced, budget updates | Pending |
| Rapid next/previous navigation during resize/move | Desired page wins; stale success/failure cannot replace it | Pending |
| Thumbnail scrolling while width changes | Background eviction precedes desired nearby slides | Pending |
| Return from large display to small window | Budget shrinks; old widths do not accumulate beyond it | Pending |
| Tall PDF on a wide display | Valid baseline fallback; no repeated upgrade error loop | Pending |

## Acceptance criteria

| Issue criterion | Evidence |
| --- | --- |
| Design for current-slide sizing and cache-budget selection | Decision, sizing formula, budget formula, numerical table, and integration safeguards above. |
| Normal DPI, high DPI, and external monitor scenarios | Scenario table, physical-pixel adapter contract, stabilization, and monitor/fullscreen transition policy. |
| Any implementation has focused selection/eviction tests | No implementation is introduced in this PR; required tests are specified for the implementation. Existing fixed-policy tests passed. |
| Manual verification notes cover high DPI or an external display | New interactive verification and GUI smoke on the DELL U4021QW external display above. |

These satisfy the design issue's closure criteria. Mixed-DPI monitor moves,
hotplug, continuous resize/navigation, actual adaptive-width output, and Windows/
Linux runtime verification remain implementation gates. Pending entries in the
procedure above describe those future checks and are not claimed as completed
adaptive-render validation.

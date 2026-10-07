# Render Cache Budgets

## Current display-aware policy

The slide window's physical client width selects the current-slide render width.
At or below 1600 px, it stays at 1600; above that floor it rounds upward in
128 px buckets, capped at 2560. Preview and thumbnail widths remain 600 and
180 px. The Rust window adapter reads Slint's physical size directly, without
multiplying by the scale factor. Hidden, zero-sized, or invalid metrics retain
the last policy. A 250 ms poll requires two consecutive identical candidates.

The cache keeps 64 entries and derives its RGBA byte budget from five 4:3
current-slide images, one preview, and seventeen thumbnails. It uses 1.5 times
that working set, clamped to 96–192 MiB (about 144.53 MiB at 2560 px). Cached
bytes are measured from actual buffers; this is not a process RSS limit.

New pages and prepared reloads render at 1600 first. Current-slide pixels supply
the page aspect ratio; low-resolution auxiliary images do not choose widths.
If an upgrade would approach the helper's 4096 px height limit, that page stays
at baseline. The policy reserves two height pixels for rounding uncertainty.
An invalid baseline still follows the existing render error path.

Width changes enforce the new budget immediately without clearing the cache.
The last-good image remains visible while the replacement is pending. Nearby
images at the selected policy width are protected; a tall page's baseline image
is also protected when it is visible. Other baseline or old-width entries can
be evicted. Only the exact desired visible request can exceed the byte budget.
Queued widths for the same session/page/purpose are coalesced in both queue
layers. Old-width completion and failure events cannot replace the desired
visible result or report its failure. Reload events carry their render width
so a baseline image cannot be cached under a newer width.

See the [design](validation/ISSUE271_DISPLAY_AWARE_RENDERING.md) and
[implementation validation](validation/ISSUE271_IMPLEMENTATION.md) for numerical
estimates, regression coverage, and desktop results.

## v1.0.0 fixed-width baseline

Quick Presenter used a fixed render cache budget for v1.0.0:

- `max_entries`: 64 rendered pages
- `max_estimated_bytes`: 96 MiB

The budget is intentionally fixed while slide rendering uses fixed target widths:

- current slide: 1600 px
- next preview: 600 px
- thumbnail: 180 px

Because these widths are not derived from the active display scale factor, the
fixed-policy cache footprint does not grow on high-DPI or external displays. A
future display-aware render-width policy is tracked separately because it affects
render quality, memory pressure, render latency, and platform display detection.

## Representative Scenarios

The following estimates use RGBA pixel buffers, matching the cache's
`actual_render_bytes()` accounting. They are deterministic budget measurements,
not PDF-content measurements; PDF complexity affects render time, but the cached
image size is determined by rendered pixel dimensions.

| Scenario | Current slide | Next preview | Thumbnail | Protected working set |
| --- | ---: | ---: | ---: | ---: |
| Normal 16:9 slides | 5.5 MiB | 0.8 MiB | 0.1 MiB | about 29.4 MiB |
| Large 4:3 slides | 7.3 MiB | 1.0 MiB | 0.1 MiB | about 39.2 MiB |
| High-DPI / external display | same as fixed-width scenario | same | same | bounded by fixed render widths |

The protected working set assumes:

- presentation cache radius 2, so up to 5 current-slide renders are protected
- one next-slide preview
- thumbnail cache radius 8, so up to 17 thumbnails are kept around the current
  page

The large 4:3 estimate remains below half of the 96 MiB byte budget. This leaves
headroom for thumbnail scrolling bursts and repeated navigation while still
letting eviction remove background thumbnails and previews before protected
presentation pages.

## Tradeoffs

The 64-entry limit protects long decks and aggressive thumbnail scrolling from
keeping every visited page. The 96 MiB byte limit protects image memory when
slides are taller than 16:9 or when future render widths increase.

Eviction is purpose-aware:

- thumbnails are evicted before previews and current slides
- next previews are evicted before current-slide renders
- current-slide renders within the presentation radius are protected first
- if protected pages alone exceed the byte budget, pages farther from the
  current slide are evicted before the visible current slide

The visible current slide may temporarily exceed the byte budget by itself. This
is intentional; during a presentation, showing the current slide is more
important than strictly satisfying the cache budget.

## v1.0.0 Decision

Keep the fixed default budget for v1.0.0. It is large enough for the current
fixed-width render policy and small enough to bound memory during rapid
navigation and thumbnail scrolling.

The display-aware implementation now changes current-slide widths and the budget
together, as specified in #271. The historical fixed-width rationale above is
preserved; it is not a claim that the original default was unsafe.

The proposed policy, code-review findings, numerical verification, and remaining
desktop checks are recorded in
[Display-aware rendering validation](validation/ISSUE271_DISPLAY_AWARE_RENDERING.md).

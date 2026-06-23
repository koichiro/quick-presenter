# Render Cache Budgets

Quick Presenter uses a fixed render cache budget for v1.0.0:

- `max_entries`: 64 rendered pages
- `max_estimated_bytes`: 96 MiB

The budget is intentionally fixed while slide rendering uses fixed target widths:

- current slide: 1600 px
- next preview: 600 px
- thumbnail: 180 px

Because these widths are not derived from the active display scale factor, the
cache footprint does not currently grow on high-DPI or external displays. A
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

Do not make the budget display-aware until slide render widths become
display-aware. That future work is tracked in issue #271 and should define how
monitor scale factor and slide window size feed into render width selection, and
then update the budget policy and eviction tests together.

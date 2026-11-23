# Homepage refresh review

Validated on 2026-11-23 against main `a6a38d1` (PRs #562 and #563 merged).

The English and Japanese homepages lead with Audience Live, the approved Hero copy, the existing Flowing recording, and a primary link to the localized Audience Live page. The existing AI, CLI, demo, and PDF preparation sections are preserved byte for byte. Legacy section and description IDs remain available. Presenter screenshots are reused and lazy-loaded.

## Screenshots

Responsive review used local fixed-width browser frames containing the built pages. Temporary review wrappers are not part of the build or committed source. Each page's measured viewport width equals its document scroll width; the measurements are in [layout.json](layout.json).

| Width | English | Japanese |
| --- | --- | --- |
| 1440px | [Desktop](en-1440.jpg) | [Desktop](ja-1440.jpg) |
| 1280px | [Laptop](en-1280.jpg) | [Laptop](ja-1280.jpg) |
| 768px | [Tablet](en-768.jpg) | [Tablet](ja-768.jpg) |
| 390px | [Mobile](en-390.jpg) | [Mobile](ja-390.jpg) |
| 320px | [Small mobile](en-320.jpg) | [Small mobile](ja-320.jpg) |

Desktop places the copy and demo side by side. Mobile places copy, CTAs, demo, then release badges and status text in both visual and document order. The primary CTA and the complete demo are visible within the captured 900px mobile viewport.

## Validation

- `npm test` in `website`: build and all 18 tests passed.
- `node --check` for the shared demo controller and language script: passed.
- `git diff --check`: passed.
- Internal navigation, fragments, assets, canonical URLs, hreflang, social cards, media identity, language routing, and legacy IDs are covered by the Web tests.
- Shared-controller tests cover reduced motion, offscreen and hidden panels, pause intent, shared fetches, late responses, failed requests, and failed image decoding. A new single-demo test exercises the homepage without comparison tabs.
- Browser checks: Enter pauses the homepage demo to its poster; Space resumes it. Primary Audience Live CTA, download CTA, menu, presenter anchor, and language switch work. All three existing videos advanced their playback times without media errors. The Audience Live comparison tabs and localized navigation work. No homepage console errors or warnings were observed.
- A browser iframe with scripts disabled displayed the static poster and hid the playback button. The no-JS fallback also retains both homepage CTAs and native navigation.
- No Rust, Slint, application, build configuration, hosting, or deployment changes; Cargo checks were not applicable.

## Performance and accessibility

The existing 960 × 510 poster is 52,165 bytes; the unchanged GIF is 4,538,342 bytes (about 4.33 MiB). The poster has high fetch priority and explicit dimensions. The demo preserves its ratio when the GIF replaces the poster, reserving its image space. There is one animated demo on each homepage. The unchanged shared controller fetches only visible, active demos and shares/reuses each asset request. Reduced motion prevents GIF fetching. Presenter screenshots and existing videos do not eagerly load their media.

Playback has localized button labels, associated image/caption IDs, keyboard controls, and focus styles. A skip link, semantic headings, and descriptive image text remain available. Desktop Japanese headline spans keep the approved text intact while allowing a break between its two clauses.

The throttled browser-native measurements below replace the previous unmeasured LCP/CLS limitation. The GIF remains a material mobile transfer when it becomes visible. No product dependencies or replacement/generated media were introduced.


## Throttled performance comparison (2026-11-23)

Measured the reviewed PR HEAD **`d5ed4d5fea9d62ef014dd0170b8d3d30c7310a61`** against **`a6a38d1acd14faaf60e34d8b71bf96bf0cab221e`**. The PR HEAD was unchanged when measurement started. Later commits in this task only add review evidence and the standalone capture script; the measured product HTML/CSS/JS is unchanged.

Both revisions were exported into separate temporary directories with `git archive`, built with their existing `node website/scripts/build.mjs`, and served with the unchanged `website/scripts/preview.mjs` on loopback HTTP servers with identical settings. There was no CDN, TLS, or response compression. Measurements use a single page at a time, alternating revision order between iterations. All 12 runs succeeded; none were discarded. No Lighthouse run or score is reported: the measurements come directly from Chromium's performance timeline and CDP Network events.

### Conditions and observation windows

- macOS 27.0.1 (26A434), Darwin arm64, Apple M6; headless Chrome for Testing **151.0.7922.34**, existing Playwright **1.62.1**, Node **26.13.1**.
- Mobile emulation: **390 × 844 CSS px**, device scale factor **1**, touch/mobile mode. Browser locale `en-US`, timezone `Asia/Tokyo`; a fixed Android mobile user agent with the measured Chrome version.
- CDP network: **1,600,000 bit/s down** (200,000 B/s), **750,000 bit/s up** (93,750 B/s), **150 ms latency**; CPU slowdown **4×**. This is emulation on a desktop CPU, not a physical handset.
- Fresh browser context for every run: empty cache/cookies/storage, HTTP cache disabled with CDP, Service Workers blocked and bypassed. Only `quick-presenter-language=en` or `ja` is seeded before site scripts. Actual `/` or `/ja/` pathname and document language are checked, without redirecting to another language.
- Normal runs use `prefers-reduced-motion: no-preference`; no clicks or scrolling occur during the 12 primary captures.
- **Initial window:** navigation until the `load` event plus 5 seconds. **Final window:** at least 20 seconds after navigation and, for the refreshed page, at least 5 seconds after the GIF Fetch has finished and its Blob image has decoded/displayed. The same final-window rule applies to all four groups; the old page has no Hero GIF to await. Exact cutoffs are listed below.
- `PerformanceObserver` records all LCP candidates and layout-shift entries from document start, including element, source URL, affected nodes, and rectangles. CLS uses the maximum session window, excluding `hadRecentInput` entries; raw entries are retained as well.

[Machine-readable conditions](performance/conditions.json) · [All primary runs and sanitized Network/performance timelines](performance/runs.json) · [Summary](performance/summary.json)

### Median comparison

Transfer totals below are the **final window's completed HTTP responses**, summed from CDP `Network.loadingFinished.encodedDataLength`: response headers plus encoded response body, excluding request headers, TCP overhead, and internal Blob reads. All responses finished by the final cutoff, all returned 200, and none were served from disk cache or a Service Worker. Counts include the same icon/favicon requests in both revisions. These are **browser Network transfer bytes**, not disk file sizes or decompressed resource sizes.

| Page | Revision | LCP median | LCP element | CLS median | Transferred bytes | Requests |
| --- | --- | --- | --- | --- | --- | --- |
| English | Before | 0.964 s | `h1.product-headline` | 0 | 1,397,743 | 10 |
| English | After | 1.780 s | `img#hero-demo` (static JPG) | 0 | 5,629,485 | 12 |
| Japanese | Before | 0.996 s | `h1.product-headline` | 0 | 1,401,546 | 10 |
| Japanese | After | 1.780 s | `img#hero-demo` (static JPG) | 0 | 5,633,849 | 12 |


### Individual runs and initial/final scope

| Page | Revision | Run | LCP (ms) | CLS | Initial cutoff (s) | Initial notified bytes* | Final cutoff (s) | Final bytes | Requests |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| en | before | 1 | 1216 | 0 | 11.947 | 1,397,743 | 20.002 | 1,397,743 | 10 |
| en | before | 2 | 964 | 0 | 11.944 | 1,397,743 | 20.001 | 1,397,743 | 10 |
| en | before | 3 | 944 | 0 | 11.947 | 1,397,743 | 20.003 | 1,397,743 | 10 |
| en | after | 1 | 1744 | 0 | 12.216 | 1,091,143 | 33.406 | 5,629,485 | 12 |
| en | after | 2 | 1780 | 0 | 12.244 | 1,091,143 | 33.410 | 5,629,485 | 12 |
| en | after | 3 | 1800 | 0 | 12.254 | 1,091,143 | 33.398 | 5,629,485 | 12 |
| ja | before | 1 | 1004 | 0 | 11.948 | 1,401,546 | 20.000 | 1,401,546 | 10 |
| ja | before | 2 | 996 | 0 | 11.947 | 1,401,546 | 20.001 | 1,401,546 | 10 |
| ja | before | 3 | 980 | 0 | 11.944 | 1,401,546 | 20.001 | 1,401,546 | 10 |
| ja | after | 1 | 1772 | 0 | 12.232 | 1,095,507 | 33.382 | 5,633,849 | 12 |
| ja | after | 2 | 1780 | 0 | 12.257 | 1,095,507 | 33.406 | 5,633,849 | 12 |
| ja | after | 3 | 1780 | 0 | 12.244 | 1,095,507 | 33.408 | 5,633,849 | 12 |


*The initial column is the number of response bytes **already notified by CDP** at the cutoff. On this Chrome version the in-flight `fetch().blob()` body is not reported incrementally, so the refreshed page's initial value is a **lower bound, not the complete wire transfer at that moment**. Its GIF request has already started. The final column uses the definitive completed-response counts and includes the entire GIF. The initial and final LCP and CLS entries were identical in every primary run. Do not substitute the initial lower bounds for total page transfer or present them as Lighthouse results.

### GIF transfer, timing, and resource sizes

In each of the six primary refreshed-page runs, the GIF was requested **exactly once**, with CDP type **`Fetch`**, not `Image`. Its URL was `/assets/audience-live/flowing.gif`; the displayed image subsequently used a local Blob URL. GIF requests started **1.232–1.394 s** after the document request and finished **28.368–28.385 s** after it. Final captures ended **5.013–5.026 s after GIF transfer completion**, after Blob decoding/display was confirmed. The file alone needs about **22.69 s** at 200,000 B/s before latency and competing resources; the measured request also shared bandwidth with the existing posters and page assets.

| Resource | Source file (B) | Encoded body (B) | Decoded body (B) | CDP Network transferred (B) | Request type |
| --- | --- | --- | --- | --- | --- |
| `flowing.jpg` | 52,165 | 52,165 | 52,165 | 52,339 | Image |
| `flowing.gif` | 4,538,342 | 4,538,342 | 4,538,342 | 4,538,517 | Fetch |

Source sizes are the existing files; encoded/decoded body sizes are corroborated by browser Resource Timing in [visibility.json](performance/visibility.json). The local server uses identity encoding, so encoded/decoded bodies equal source sizes here. The CDP transfer column additionally includes response headers. Resource Timing's separate `transferSize` values were 52,465 and 4,538,642 B; they are retained in that evidence but are **not mixed into the CDP totals**. The raw CDP `receivedBodyBytes` field for the buffered Fetch can be zero even after completion; it is not used as a decoded-resource-size measurement.

### Browser GIF-control evidence

[Control-state and Network logs](performance/controls.json) · [Strict offscreen-first loading evidence](performance/visibility.json)

| Scenario | English | Japanese |
| --- | --- | --- |
| Main 390 × 844 first view | GIF Fetch once while demo is visible | GIF Fetch once while demo is visible |
| Strictly offscreen first load, then scroll into view | 0 GIF requests offscreen → 1 in view | 0 GIF requests offscreen → 1 in view |
| Pause / Play, three complete cycles | Poster / Blob toggles; total GIF Fetch count stays 1 | Poster / Blob toggles; total GIF Fetch count stays 1 |
| Scroll offscreen and return | Poster offscreen, Blob after return; total GIF Fetch count stays 1 | Poster offscreen, Blob after return; total GIF Fetch count stays 1 |
| Fresh load with Reduced Motion | JPG; localized still-image label; 0 GIF requests | JPG; localized still-image label; 0 GIF requests |
| Fresh load with JavaScript disabled | JPG and both main CTAs visible; playback control hidden; 0 GIF requests | JPG and both main CTAs visible; playback control hidden; 0 GIF requests |

The strict offscreen-first check uses an additional **390 × 480** top-level viewport so that the demo is below the fold **before its first intersection observation**. This supplements, and does not enter, the 390 × 844 comparison medians. After `load + 5 s` with no GIF request, the demo is scrolled into view; the capture waits for its Blob display and a further 5 seconds. Reduced-motion and no-JS pages are observed for 20 seconds after `load` with fresh contexts.

Normal interaction traces cover approximately **33.4–44.8 s** after navigation in both languages: three Pause/Play cycles, 1.2 seconds offscreen, then at least 5 seconds after returning. The image rectangle remains **362 × 192.3125 CSS px** across poster/Blob transitions. Button text changes its width and left edge: **67.796875 ↔ 55.375 px** (English) and **82 ↔ 54 px** (Japanese). Each click produces a raw layout-shift value of **0.0001094584** or **0.0002975260**, respectively, tagged `hadRecentInput=true`; they do not contribute to CLS. The sum of *all* raw shift values, including the initial button reveal, is **0.0016064334 / 0.0026938761** in these two interaction traces (this sum is not the CLS metric). No additional shift entry appears for the offscreen/return transitions.

### Interpretation

- **English:** LCP median increased **816 ms** (0.964 → 1.780 s). **Japanese:** it increased **784 ms** (0.996 → 1.780 s). All 12 individual LCP values remained below the supplied 2.5 s reference, and all primary CLS values were **0**, below 0.1. Three repeats support this local comparison, not a field-performance guarantee.
- The old mobile LCP element was the product-name `h1`. In all six refreshed runs, the last LCP entry was **`img#hero-demo` with `/assets/audience-live/flowing.jpg`**. The performance timeline did not replace it with a GIF/Blob LCP candidate after the animation loaded. This identifies what the browser actually measured; it does not assume that GIF transfer time equals LCP.
- Final transfer rose by **4,231,742 B** for English (about **302.8%**) and **4,232,303 B** for Japanese (about **302.0%**), with **2 additional HTTP requests** in each locale. The GIF adds 4,538,517 Network transfer bytes; other changes, including lazy presenter images, offset about 307 kB of that increase. Existing below-the-fold video posters still contribute to both revisions' transfer totals.
- There is no material CLS regression or LCP threshold failure under these conditions. The refresh is acceptable for the intended preview with its known payload tradeoff, but the roughly 4.23 MB increase is material on mobile and should not be described as a lightweight transfer. The shared loader avoids repeated GIF downloads; it cannot reduce the first download's size. In this lab condition the animation itself is not ready until about 28.4 s, despite a good poster LCP. Prioritize a separate payload optimization before claiming fast animation on constrained mobile connections.
- No product fix, copy change, or design change was required by the observed thresholds; the small caption/button shifts are documented below rather than hidden by a zero CLS result. A separate optimization proposal could compare a substantially smaller animation encoding or explicit user-initiated animation loading. Those would change the required GIF/interaction policy and are intentionally not implemented here.

### Layout-shift details, including excluded entries

Standard CLS is **0** in all 12 primary runs, but that does **not** mean zero visual movement. All six refreshed-page runs recorded one small initial shift when the hidden playback button became visible: caption height grew by about **10.80 CSS px**, the caption label moved down about **5.39 px**, and `.home-details` moved down about **10.80 px**. The raw shift value was **0.0009496831 (English)** or **0.0009087203 (Japanese)**. Chromium marked these initial entries `hadRecentInput=true` even though the primary capture issued no clicks or scrolling. They are excluded by the standard CLS calculation; the browser flag is retained verbatim, not reclassified. Counting every raw entry conservatively gives those same values, still far below 0.1. The old page recorded no shifts.

The primary initial/final demo image rectangles are identical: the JPG-to-Blob/GIF replacement itself produced no additional layout-shift entry. Button interaction shifts and their observation intervals are recorded separately below. Reserving the caption/control row's height could remove the tiny initial movement in a separate refinement, but is not necessary for the measured threshold and is not changed in this measurement-only task.

### Limits

This is a **local lab measurement**, not the deployed CDN or a real mobile network. Loopback HTTP has no CDN cache, TLS, protocol multiplexing, or compression; OS file caching and host activity are not representative of a handset. CPU throttling is relative to this host. This is not a Lighthouse simulation, a Lighthouse score, or a field Core Web Vitals assessment. The initial in-flight-byte limitation is disclosed above; final transfer totals are complete. No raw HAR, cookies, authentication headers, request/response bodies, personal data, or machine-specific absolute paths are published in the JSON evidence.

### Direct-fragment edge case and probe accounting

An exploratory offscreen-first probe initially used `/#download` and stopped at its assertion that no GIF request should exist. That setup did **not** satisfy the intended precondition: the Hero was briefly visible before the initial smooth fragment scroll moved it offscreen. This was a supplemental probe, not one of the 12 primary measurements; no primary measurement was excluded.

[The follow-up visibility/Network diagnostic](performance/fragment-diagnostic.json) records the English demo as visible at **1.291 s**, the GIF Fetch starting at **1.294 s**, and the demo becoming offscreen at **1.500 s**. The image returns to its poster offscreen, but the already-started Fetch is not canceled. The strict offscreen-first tests were therefore rerun with the 390 × 480 viewport described above and passed in both languages.

This is a real remaining transfer-cost edge case for direct-anchor visitors: visibility gating does not cancel an asset whose download started while it was momentarily visible. A focused follow-up could delay animation activation until initial fragment scrolling settles, or cancel an in-flight fetch when all consumers become inactive, with shared-consumer/retry tests. No such product change is included in this measurement task.

### Reproduction and evidence handling

The capture uses the [standalone review script](performance/measure.mjs); it adds no package dependency, production code, build setting, or test framework. Use an existing Playwright module and Chrome for Testing executable matching [conditions.json](performance/conditions.json). Export the two recorded revisions into `<lab-root>/before` and `<lab-root>/after`, and run each revision's `website/scripts/build.mjs` before capture. The script starts/stops each unchanged preview server itself. Set these environment variables to your local paths, which are deliberately not published:

```sh
export PLAYWRIGHT_MODULE='<existing-playwright-core>/index.mjs'
export CHROME_PATH='<existing-chrome-for-testing-executable>'
export CHECKOUT_ROOT='<lab-root>'
export OUTPUT_DIR='<review-output-directory>'
node website/review/homepage-refresh/performance/measure.mjs
VISIBILITY_ONLY=1 node website/review/homepage-refresh/performance/measure.mjs
FRAGMENT_ONLY=1 node website/review/homepage-refresh/performance/measure.mjs
```

The first command captures all 12 primary runs and both locales' normal/reduced-motion/no-JS control checks; the second records the strict offscreen-first checks; the third records the optional direct-fragment diagnostic. The latter two do not alter `runs.json` or `controls.json`. The final script also exports Resource Timing body sizes when taking snapshots; those are present in the supplemental visibility evidence. Version/byte/geometry data and static URL pathnames were retained instead of exporting a raw HAR or full browser trace. All published JSON was checked for private URLs, local absolute paths, cookies, authentication data, and request/response bodies. Only the expected static page/asset pathnames are present.

Final checks for this review-only follow-up: `npm test` built the site and passed all 18 Web tests; `node --check performance/measure.mjs` and `git diff --check` passed. Product source, Hero copy, design, dependencies, build configuration, and deployment settings are unchanged.

## Public release state checked

- [GitHub Releases](https://github.com/koichiro/quick-presenter/releases): v2.5.0 is Latest/Stable; [v3.0.0 RC1](https://github.com/koichiro/quick-presenter/releases/tag/v3.0.0-rc.1) is a prerelease with macOS Apple Silicon DMG and Ubuntu amd64 DEB assets, without a Windows RC package.
- [Mac App Store](https://apps.apple.com/jp/app/quick-presenter-pdf-slides/id6820406883): Apple's public lookup reported version 2.5.0 and macOS 14.4 minimum.
- [Microsoft Store](https://apps.microsoft.com/detail/9N913S9NJ6D1): public product update notes identify version 2.5.0.
- [Official Relay Shop](https://shop.quickpresenter.com/): public browser view offered only a free ticket. The existing Open Beta positioning is retained without adding a direct Shop CTA to the homepage.
- [Relay license](https://github.com/koichiro/quick-presenter-relay/blob/main/LICENSE): Apache-2.0, separate from Quick Presenter's GPL-3.0-only license and the hosted Official Relay service.

## Release follow-up

When v3.0.0 becomes final, recheck the published release assets and each Store separately. Update the homepage preview eyebrow/status, Stable version and release notes, RC callout/link, and matching regression expectations. Update the existing Audience Live page's preview/download copy in the release follow-up as well. Change Official Relay Beta/free-ticket wording only when the public service state changes. No release date is assumed, and this PR performs no release or deployment.

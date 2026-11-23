# Homepage refresh review

Validated on 2026-11-23 against main `a6a38d1` (PRs #562 and #563 merged).

The English and Japanese homepages lead with Audience Live, the approved Hero copy, the existing Flowing recording, and a primary link to the localized Audience Live page. The existing AI, CLI, demo, and PDF preparation sections are preserved byte for byte. Legacy section and description IDs remain available. Presenter screenshots are reused and lazy-loaded.

## CodeRabbit follow-up — no-observer fallback

[The review finding](https://github.com/koichiro/quick-presenter/pull/564#discussion_r4664336564) is fixed in **`11a56d0304e8b014000a95845517565f4de15c61`**. Without IntersectionObserver, initial fragment settling could set a demo's visibility to false with no later viewport notification to update it. Hero therefore stayed on its poster after the user returned from `#connect`.

The fallback now refreshes demo geometry on scroll and resize, coalescing notifications into one animation frame. It also refreshes geometry when playback/preferences or comparison tabs trigger a render. Initial-fragment deferral, explicit pause intent, reduced motion, hidden-panel filtering and shared Fetch/cache ownership remain intact. Native IntersectionObserver pages do not install these fallback scroll/resize listeners. The fragment-settle check reuses the same geometry helper.

- Web build and **all 22 tests passed**. The new regression covers an offscreen `#connect` landing without IntersectionObserver, returning to Hero, pause retention, resize, hidden/visible comparison panels and unique asset reuse. Existing native-observer, slow-response, failed-fetch/decode and retry tests still pass.
- [Sanitized real-browser Network and state evidence](performance/no-observer-fix/browser-checks.json): Chrome for Testing 151.0.7922.34, fresh contexts, 390 × 844 mobile viewport / DPR 1 / 4× CPU, unthrottled local HTTP. IntersectionObserver was deleted before page scripts; every captured state verifies it is unavailable.
- Both English and Japanese Audience Live `#connect` cold visits issue **0 GIF requests** after landing. Returning to the actual Hero image starts one Flowing Fetch and renders its Blob image. Pause/Play works. Resizing through 390 × 200 (offscreen) and 390 × 2000 (visible), then restoring the normal viewport, updates poster/playback without another Flowing request. Visiting Flowing and switching Bubbles/back finishes with **one Fetch per unique GIF**.
- `node --check` for the controller/capture script and `git diff --check` passed. Published evidence contains static pathnames and media state only; no cookies, auth headers, response bodies or local absolute paths are exported.

No new LCP/CLS measurement is claimed in this fallback-only follow-up. The datasets below remain explicitly tied to their recorded product commits. Copy, CSS, assets, dependencies and build/deployment settings are unchanged.

To reproduce, archive/build `11a56d0` as `CHECKOUT_ROOT/after`, set the existing Playwright/Chrome paths as below, and use a fresh evidence directory:

```sh
ONLY_LANDING=1 NO_OBSERVER=1 AFTER_REVISION=11a56d0304e8b014000a95845517565f4de15c61 \
  node website/review/homepage-refresh/performance/check-fragments.mjs
```

## Initial-fragment fix and verification — 5d62371

The P2 direct-anchor issue is fixed in **`5d6237141d8534de9f778b072299f11a5fc1c843`**. The old controller could start a GIF Fetch during the brief initial Hero intersection before smooth scrolling reached `#download`.

For a valid initial fragment only, animation activation now waits for `load` and then compares actual scroll coordinates and target geometry over consecutive animation frames. Three unchanged frame-to-frame samples are required; continued scrolling resets that count. On settling, the controller reads each demo's current bounds before rendering, rather than replaying a stale early IntersectionObserver notification. There is no fixed long timeout, permanent hash-based playback restriction, or change to shared Fetch/cache/retry ownership. No-fragment, missing-target and malformed-fragment visits bypass the wait. Copy, design, assets, dependencies, build configuration, and deployment settings are unchanged.

### Post-fix normal-visit measurements

The measured product commit is `5d62371`; the evidence commit `30e7b1d` did not alter that controller. The later no-observer fallback fix `11a56d0` is documented separately above and is not represented by these historical numbers. A fresh archive of that commit was built/served with the same existing scripts and **the same Chrome 151.0.7922.34, macOS/host, 390 × 844 DPR 1 mobile emulation, 1.6 Mbps down / 750 Kbps up / 150 ms latency, 4× CPU slowdown, cold contexts, and observation windows** as the historical captures. English and Japanese normal no-fragment visits were each repeated three times; all six succeeded, none were excluded. These are a new measurement batch, not replacement values for `d5ed4d5` or a claim to isolate millisecond-level host noise.

| Page | d5ed4d5 LCP median | 5d62371 LCP median | CLS median old / fixed | Final transfer old / fixed (B) | Requests old / fixed |
| --- | --- | --- | --- | --- | --- |
| English | 1.780 s | 1.768 s | 0 / 0 | 5,629,485 / 5,631,006 | 12 / 12 |
| Japanese | 1.780 s | 1.784 s | 0 / 0 | 5,633,849 / 5,635,370 | 12 / 12 |

All six fixed-run LCP entries identify the static `flowing.jpg` Hero poster. The median changes are **−12 ms / +4 ms**, with no evidence of an added no-fragment startup delay. The GIF starts at **1.268–1.361 s** and completes at **28.378–28.427 s**; each page fetches it once. The total response-byte increase is exactly **1,521 B** per page, from the added shared JavaScript, not another GIF transfer. Poster/GIF dimensions and the small initial caption shift are unchanged. Standard CLS remains 0; the historical explanation of raw excluded shifts still applies.

| Page | Run | LCP (ms) | CLS | GIF Fetch start (s) | Final cutoff (s) | Final transferred bytes | Requests |
| --- | --- | --- | --- | --- | --- | --- | --- |
| en | 1 | 1808 | 0 | 1.361 | 33.454 | 5,631,006 | 12 |
| ja | 1 | 1784 | 0 | 1.278 | 33.396 | 5,635,370 | 12 |
| en | 2 | 1768 | 0 | 1.279 | 33.410 | 5,631,006 | 12 |
| ja | 2 | 1776 | 0 | 1.268 | 33.406 | 5,635,370 | 12 |
| en | 3 | 1768 | 0 | 1.312 | 33.413 | 5,631,006 | 12 |
| ja | 3 | 1784 | 0 | 1.278 | 33.410 | 5,635,370 | 12 |

The final window again includes every completed response and at least five seconds after GIF completion/Blob display. [Fixed conditions](performance/fragment-fix/conditions.json), [six full runs](performance/fragment-fix/runs.json), and [fixed summary](performance/fragment-fix/summary.json) are separate from the historical files below. Local-lab/CDN, buffering, and desktop CPU-emulation limitations remain the same. The 4.33 MiB GIF and its roughly 28.4 s readiness on this constrained connection remain a payload optimization opportunity; this fix prevents unnecessary *starts* during initial anchor navigation rather than shrinking that asset.

### Post-fix browser regression evidence

[Browser cases and sanitized Network/intersection timelines](performance/fragment-fix/browser-checks.json) record 26 successful fresh-context cases. Direct download screenshots: [English](performance/fragment-fix/download-en.jpg) / [Japanese](performance/fragment-fix/download-ja.jpg); Hero after return: [English](performance/fragment-fix/returned-en.jpg) / [Japanese](performance/fragment-fix/returned-ja.jpg). The `#download` cases use the same slow network/4× CPU/mobile viewport as the measurements; other functional cases use the same browser/viewport/CPU with unthrottled networking to check behavior, not to enter performance medians.

- English `/#download` and Japanese `/ja/#download`: the actual first-view intersection can occur, but no GIF request starts before/during the initial fragment scroll or within `load + 5 s` after landing. The displayed source remains the static poster. Returning to Hero starts one GIF Fetch and displays the Blob image; after three Pause/Play cycles and another offscreen/return cycle, the GIF request count stays one.
- Both locales also check `#features`, `#automation`, `#authoring`, `#scope`, and `#demo`: zero GIF starts after landing, then exactly one on returning to Hero.
- Normal no-fragment, missing-target, malformed-fragment and `#top` visits all play a visible Hero; no hash remains a permanent playback lock.
- Fresh Reduced Motion and JavaScript-disabled pages retain the poster and issue no GIF request. The disabled-JavaScript pages retain both primary CTAs and hide the playback control.
- English and Japanese Audience Live `#connect` visits issue no initial GIF request. Returning to Hero, visiting the Flowing comparison, switching to Bubbles and back preserves shared fetching: one `flowing.gif` request and one `bubbles.gif` request total.
- The 21 Web tests include new coverage for post-load scrolling, stale intersection notifications, fresh geometry, valid/missing/malformed fragments, pause/reduced-motion intent and shared/hidden panels. Existing late-response, request-failure, decode-failure, retry and shared-cache tests remain passing.

An earlier Japanese landing-page harness attempt timed out because scrolling to document top did not actually put its lower Hero image in the mobile viewport. The harness was corrected to scroll each demo image explicitly into view, and both landing-page cases were rerun. No primary performance run was excluded or changed for this harness correction.

The original `performance/fragment-diagnostic.json` is intentionally retained as **pre-fix** evidence; current results live under `performance/fragment-fix/`. No raw HAR, headers, cookies, tokens, response bodies, or unnecessary local absolute paths are published.

Reproduce by preparing `CHECKOUT_ROOT/before` from `a6a38d1` and `CHECKOUT_ROOT/after` from `5d62371`, building both with their existing scripts, and setting the existing `PLAYWRIGHT_MODULE`, `CHROME_PATH`, and a separate `OUTPUT_DIR` as in the historical reproduction instructions. Then run:

```sh
export AFTER_REVISION=5d6237141d8534de9f778b072299f11a5fc1c843
AFTER_ONLY=1 SKIP_CONTROLS=1 node website/review/homepage-refresh/performance/measure.mjs
node website/review/homepage-refresh/performance/check-fragments.mjs
```

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

- `npm test` in `website`: build and all 22 tests passed after the no-observer fallback fix (18 in the original refresh, 21 after the initial-fragment fix).
- `node --check` for the shared demo controller and language script: passed.
- `git diff --check`: passed.
- Internal navigation, fragments, assets, canonical URLs, hreflang, social cards, media identity, language routing, and legacy IDs are covered by the Web tests.
- Shared-controller tests cover reduced motion, offscreen and hidden panels, pause intent, shared fetches, late responses, failed requests, and failed image decoding. A new single-demo test exercises the homepage without comparison tabs.
- Browser checks: Enter pauses the homepage demo to its poster; Space resumes it. Primary Audience Live CTA, download CTA, menu, presenter anchor, and language switch work. All three existing videos advanced their playback times without media errors. The Audience Live comparison tabs and localized navigation work. No homepage console errors or warnings were observed.
- A browser iframe with scripts disabled displayed the static poster and hid the playback button. The no-JS fallback also retains both homepage CTAs and native navigation.
- No Rust, Slint, application, build configuration, hosting, or deployment changes; Cargo checks were not applicable.

## Performance and accessibility

The existing 960 × 510 poster is 52,165 bytes; the unchanged GIF is 4,538,342 bytes (about 4.33 MiB). The poster has high fetch priority and explicit dimensions. The demo preserves its ratio when the GIF replaces the poster, reserving its image space. There is one animated demo on each homepage. The shared controller fetches only visible, active demos and shares/reuses each asset request. Reduced motion prevents GIF fetching. Presenter screenshots and existing videos do not eagerly load their media.

Playback has localized button labels, associated image/caption IDs, keyboard controls, and focus styles. A skip link, semantic headings, and descriptive image text remain available. Desktop Japanese headline spans keep the approved text intact while allowing a break between its two clauses.

The throttled browser-native measurements below replace the previous unmeasured LCP/CLS limitation. The GIF remains a material mobile transfer when it becomes visible. No product dependencies or replacement/generated media were introduced.


## Historical throttled performance comparison — d5ed4d5 (2026-11-23)

Measured the reviewed PR HEAD **`d5ed4d5fea9d62ef014dd0170b8d3d30c7310a61`** against **`a6a38d1acd14faaf60e34d8b71bf96bf0cab221e`**. The PR HEAD was unchanged when that measurement started. The measurement-only follow-up `d2fdbb7` preserved that product revision. The subsequent initial-fragment fix changes the shared JavaScript controller; its separate measurements and evidence are documented above. The historical numbers below remain unchanged.

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

### Historical direct-fragment edge case and probe accounting

An exploratory offscreen-first probe initially used `/#download` and stopped at its assertion that no GIF request should exist. That setup did **not** satisfy the intended precondition: the Hero was briefly visible before the initial smooth fragment scroll moved it offscreen. This was a supplemental probe, not one of the 12 primary measurements; no primary measurement was excluded.

[The follow-up visibility/Network diagnostic](performance/fragment-diagnostic.json) records the English demo as visible at **1.291 s**, the GIF Fetch starting at **1.294 s**, and the demo becoming offscreen at **1.500 s**. The image returns to its poster offscreen, but the already-started Fetch is not canceled. The strict offscreen-first tests were therefore rerun with the 390 × 480 viewport described above and passed in both languages.

This historical defect was fixed in `5d62371`: initial valid-fragment navigation now defers animation activation until scrolling settles, then rechecks actual geometry. The post-fix direct-anchor Network checks above supersede this old diagnostic; the JSON is retained as the original failure evidence. No shared-fetch cancellation was introduced.

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

Historical checks for the measurement-only follow-up `d2fdbb7`: `npm test` built the site and passed all 18 Web tests; `node --check performance/measure.mjs` and `git diff --check` passed. Product source, Hero copy, design, dependencies, build configuration, and deployment settings are unchanged.

## Public release state checked

- [GitHub Releases](https://github.com/koichiro/quick-presenter/releases): v2.5.0 is Latest/Stable; [v3.0.0 RC1](https://github.com/koichiro/quick-presenter/releases/tag/v3.0.0-rc.1) is a prerelease with macOS Apple Silicon DMG and Ubuntu amd64 DEB assets, without a Windows RC package.
- [Mac App Store](https://apps.apple.com/jp/app/quick-presenter-pdf-slides/id6820406883): Apple's public lookup reported version 2.5.0 and macOS 14.4 minimum.
- [Microsoft Store](https://apps.microsoft.com/detail/9N913S9NJ6D1): public product update notes identify version 2.5.0.
- [Official Relay Shop](https://shop.quickpresenter.com/): public browser view offered only a free ticket. The existing Open Beta positioning is retained without adding a direct Shop CTA to the homepage.
- [Relay license](https://github.com/koichiro/quick-presenter-relay/blob/main/LICENSE): Apache-2.0, separate from Quick Presenter's GPL-3.0-only license and the hosted Official Relay service.

## Release follow-up

When v3.0.0 becomes final, recheck the published release assets and each Store separately. Update the homepage preview eyebrow/status, Stable version and release notes, RC callout/link, and matching regression expectations. Update the existing Audience Live page's preview/download copy in the release follow-up as well. Change Official Relay Beta/free-ticket wording only when the public service state changes. No release date is assumed, and this PR performs no release or deployment.

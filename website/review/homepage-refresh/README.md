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

No throttled Lighthouse/Web Vitals trace or numerical LCP/CLS benchmark was collected. The GIF remains a material mobile transfer when it becomes visible. No new libraries or replacement/generated media were introduced.

## Public release state checked

- [GitHub Releases](https://github.com/koichiro/quick-presenter/releases): v2.5.0 is Latest/Stable; [v3.0.0 RC1](https://github.com/koichiro/quick-presenter/releases/tag/v3.0.0-rc.1) is a prerelease with macOS Apple Silicon DMG and Ubuntu amd64 DEB assets, without a Windows RC package.
- [Mac App Store](https://apps.apple.com/jp/app/quick-presenter-pdf-slides/id6820406883): Apple's public lookup reported version 2.5.0 and macOS 14.4 minimum.
- [Microsoft Store](https://apps.microsoft.com/detail/9N913S9NJ6D1): public product update notes identify version 2.5.0.
- [Official Relay Shop](https://shop.quickpresenter.com/): public browser view offered only a free ticket. The existing Open Beta positioning is retained without adding a direct Shop CTA to the homepage.
- [Relay license](https://github.com/koichiro/quick-presenter-relay/blob/main/LICENSE): Apache-2.0, separate from Quick Presenter's GPL-3.0-only license and the hosted Official Relay service.

## Release follow-up

When v3.0.0 becomes final, recheck the published release assets and each Store separately. Update the homepage preview eyebrow/status, Stable version and release notes, RC callout/link, and matching regression expectations. Update the existing Audience Live page's preview/download copy in the release follow-up as well. Change Official Relay Beta/free-ticket wording only when the public service state changes. No release date is assumed, and this PR performs no release or deployment.

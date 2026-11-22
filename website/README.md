# Website deployment and custom domain

The shared [color theme](COLOR_THEME.md) defines the v1.0.0 brand palette
and its application to the v2.5.0 website.

`npm ci` installs the dependency-free Node project. `npm run build` copies the
HTML, CSS, and selected repository assets into `dist`; `npm test` rebuilds and
validates the output with Node's built-in test runner. `npm run preview` serves
the build locally. Relative navigation and asset paths work at the domain root.

The Website workflow validates pull requests and deploys `website/dist` on
matching pushes to `main` using GitHub Actions and GitHub Pages. Cloudflare
provides DNS only. The canonical URL is https://quickpresenter.com/.
The previous URL, https://koichiro.github.io/quick-presenter/, is retained here
only for migration history, redirect checks, and rollback.

## Manual migration checklist (after merge)

These operations are not performed by the pull request. Record the previous
Pages custom-domain setting and affected DNS records before changing them.

### Domain ownership

- [ ] In the **koichiro account's Settings → Pages** (not repository settings),
  choose **Add a domain**, enter `quickpresenter.com`, and choose **Add domain**.
- [ ] Copy the name and generated value from **Add a DNS TXT record** into
  Cloudflare DNS. The name is `_github-pages-challenge-koichiro` within the
  `quickpresenter.com` zone; use exactly the name GitHub displays. Set TTL to
  Auto. The value must be obtained from GitHub; do not commit it or credentials.
- [ ] Check `dig _github-pages-challenge-koichiro.quickpresenter.com TXT +short`,
  then return to account Pages settings, choose **Continue verifying** if needed,
  and click **Verify**. Keep this TXT record to maintain verification.

This proves account ownership; the repository Pages DNS check separately checks
whether DNS routes website traffic correctly. See GitHub's
[domain verification procedure](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/verifying-your-custom-domain-for-github-pages).

### GitHub Pages and Cloudflare DNS

- [ ] Open repository **Settings → Pages** and confirm **GitHub Actions** is the
  deployment source.
- [ ] Set **Custom domain** to `quickpresenter.com` and **Save**, before changing
  the traffic-routing DNS records below.
- [ ] In Cloudflare, check for conflicting records at `@` and `www`. Correct only
  conflicts with this website routing; preserve unrelated MX, TXT, and other
  records. Do not add wildcard records.
- [ ] Configure all records below with **Proxy status: DNS only** and **TTL: Auto**.

| Type | Name | Content |
| --- | --- | --- |
| A | @ | 185.199.108.153 |
| A | @ | 185.199.109.153 |
| A | @ | 185.199.110.153 |
| A | @ | 185.199.111.153 |
| AAAA | @ | 2606:50c0:8000::153 |
| AAAA | @ | 2606:50c0:8001::153 |
| AAAA | @ | 2606:50c0:8002::153 |
| AAAA | @ | 2606:50c0:8003::153 |
| CNAME | www | koichiro.github.io |

- [ ] Return to repository Pages settings and confirm the DNS check succeeds.
- [ ] Wait for GitHub Pages to provision HTTPS, then enable **Enforce HTTPS**.
  Leave Cloudflare Proxy disabled during the initial migration.
- [ ] Update repository **About → Website** to `https://quickpresenter.com/`.

GitHub's [custom-domain guidance](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site)
documents these records and the automatic `www` redirect to the configured apex
domain. Custom Actions deployments do not require a repository `CNAME` file.
DNS propagation and HTTPS availability can each take up to 24 hours.
Hosting stays on GitHub Pages; relay/API services are outside this migration.

## Post-deployment verification

Run manually after DNS and certificate provisioning:

```sh
dig quickpresenter.com A +short
dig quickpresenter.com AAAA +short
dig www.quickpresenter.com CNAME +short
curl -I https://quickpresenter.com/
curl -I https://quickpresenter.com/privacy/
curl -I https://www.quickpresenter.com/
curl -I https://koichiro.github.io/quick-presenter/
```

- [ ] A/AAAA answers match the table and `www` points to `koichiro.github.io`.
- [ ] Homepage and privacy policy respond successfully over HTTPS.
- [ ] `www` and the old project URL redirect to `https://quickpresenter.com/`.
  Inspect `Location` headers and follow with `curl -IL --max-redirs 5` to detect
  loops. Also check the old `/privacy/` URL reaches the new privacy policy.
- [ ] In a browser, verify navigation in both directions, CSS, all screenshots,
  the demo GIF, icon, and store badges; inspect canonical and Open Graph URLs.

These live checks remain pending until the manual migration is performed.

## Rollback (manual only)

1. **Repository:** Revert the migration commit through a reviewed PR and allow
   the Website workflow to redeploy the prior metadata and documentation. Remove
   or revert the new URL assertions together with that change.
2. **Cloudflare:** Restore the recorded routing records or remove only records
   introduced for this migration. Correct erroneous values if retrying instead.
   Preserve unrelated records and the ownership TXT record. Remove routing to
   Pages before releasing its custom-domain association.
3. **GitHub Pages:** Restore the recorded custom-domain configuration. If it was
   empty, remove `quickpresenter.com` in repository Settings → Pages to return to
   the project URL; retain GitHub Actions as the source. Restore About → Website.
4. **Propagation:** Allow DNS caches and any HTTPS certificate changes to settle;
   neither a repository revert nor a settings change flushes DNS. Repeat DNS,
   HTTPS, assets, and navigation checks against the restored destination. Confirm
   the old project URL serves the site without a stale custom-domain redirect.

Do not execute rollback automatically.

## Version 2.5.0 local release preview

The homepage introduces the stable `qp` interface alongside PDF playback:
source context, explicit presentation control, and NDJSON change observation.
The optional external narration runner is linked from the `v2.5.0` examples.
It is separate from the player; Quick Presenter does not bundle an AI model or
Kokoro. macOS automation is available in both the Mac App Store and Developer
ID / DMG distributions in v2.5.0. Public Store delivery was confirmed on
2026-10-25 JST; see the release checklist below.

The checked-in `src/assets/kokoro-demo.mp4` is an actual macOS presenter-window
recording of the four-slide Marp notes demo from #445, with the cursor hidden.
Its audio uses the exact cached Kokoro WAV files used in that run, synchronized
with recorded playback launch times because system-audio capture was silent.
The 1024×720 H.264/AAC MP4 has faststart metadata and is approximately 1.8 MB.
This recording is presented as **Autopilot** at `#autopilot`.
The homepage provides native playback controls, a poster, and the full text
transcript; it does not autoplay or download the video before playback.

Run `npm test` to rebuild and validate the output, links, assets, and local
video range requests. Run `npm run preview` for a local browser preview.
The preview server serves MP4/JPEG content types and byte ranges so video
seeking works locally. A local preview does not publish a release or deploy
GitHub Pages; confirm package availability and release copy before publishing.

## English and Japanese pages

The English homepage stays at `/`, with Japanese at `/ja/`. Privacy pages are
`/privacy/` and `/ja/privacy/`. Each page provides explicit language navigation,
a self-referencing canonical URL, and reciprocal `hreflang` alternatives.
On the first visit to an English URL, `language.js` uses the browser's first
preferred language (`navigator.languages`, with `navigator.language` as a
fallback) to select Japanese or English. A direct Japanese URL is respected.
The initial choice and subsequent manual language selections are saved in
`localStorage` and take precedence on later visits. Redirects preserve the page,
query string, and fragment. This is a browser-side redirect, not HTTP
`Accept-Language` negotiation by GitHub Pages. Without JavaScript, both static
pages and their language links remain usable. When storage is blocked, manual
links carry a `lang` query parameter to honor the choice on the destination;
the preference cannot persist across unrelated visits.
Both languages share the same media files; the English narration has an original
transcript and Japanese translation on the Japanese homepage.

Edit the static pages under `src/` and `src/ja/` together when changing product
copy. Keep command names and API options unchanged in translated examples.
The output tests cover both locales and language navigation.


## Interactive AI Presenter recording and guide

The display name is **Interactive AI Presenter**. The legacy
`#agentic-presentation` anchor and full-recording filenames remain valid.
The homepage presents this example before Autopilot, and keeps both as examples
of a broader interface rather than the only supported workflows.

The main player uses `src/assets/interactive-ai-short.mp4` and the matching
short preview supplied from the recording's `short/output/` folder. It is
28.7 seconds, 1920×1080 H.264, 2,448,799 bytes, faststart, and has no audio.
Processing waits were edited; it is not a latency benchmark. Codex is the
recorded external agent. The question, related-slide selection, answer,
return request, and restored page remain in their original order.

`src/assets/agentic-presentation-demo.mp4` and its poster retain the supplied
formal recording. It is 75.5 seconds, 1920×1080 H.264, approximately 5.3 MB,
and has no audio. It appears in a collapsed full-demo player; neither player
autoplays or preloads video data. The hero and Open Graph use the actual v2 presenter-window screenshot.
The hero introduces PDF playback first; the following section introduces AI
integration and retains both interactive and narration demos.

The localized guides are `/guides/interactive-ai/` and
`/ja/guides/interactive-ai/`, with the repository guide at
`docs/INTERACTIVE_AI_PRESENTER.md`. The copies of
`docs/samples/quick-presenter-demo.pdf` (12 pages) and
`examples/autopresent/demo.pdf` (4 pages) in `src/assets/` come from v2.0.0
commit `926882b01befc0f9937ec93c7a182fc324975010`. Update their provenance when
replacing the samples. Keep source notes in Marp comments and export with
`--pdf-notes`; ordinary PDF export does not guarantee extractable notes.

The site documents `qp export --json`, `goto`, `status`, and `watch` from the
v2.5.0 CLI contract (Control Protocol v1, introduced in v2.0.0). Export provides
text and notes, not images or OCR.
The external agent interprets data and chooses actions; restoration requires
a user request. The guide prompt checks document and session identity before
restoring a saved page.

## Release and local verification checklist

The existing homepages target v2.5.0. Public release and actual Store delivery were
confirmed for both Mac App Store and Microsoft Store on 2026-10-25 JST.
The completed [Store qualification tracker #520](https://github.com/koichiro/quick-presenter/issues/520#issuecomment-6391442187)
records the results and remaining validation limits.

Both localized homepages and interactive guides describe matching v2.5.0
GUI/CLI packages. Mac App Store now supports CLI control via the bundled
`Contents/Helpers/ControlCLI.app/Contents/MacOS/qp`; Developer ID / DMG uses
`Contents/MacOS/qp`. The guide explains the native PDF picker, exact-file
selection within 120 seconds, cancellation behavior, and saved read-only access.
Store product IDs are unchanged. Direct macOS and Debian downloads, CLI and
compatibility references, and examples link to v2.5.0. Existing recordings,
screenshots, and sample PDFs retain their original provenance above and below.

Run `npm test` before publishing. Review both localized homepages and guides,
including download cards at desktop and mobile widths. No Store listing,
release record, or deployment setting is changed by this website update.

The presenter feature uses a static screenshot rather than an unpausable GIF.
All videos use native controls. Focus outlines, a native details-based compact
navigation menu, and reduced-motion rules are supplied. The `#scope` alias is
kept within PDF preparation; `#support` now includes open source and support.

Before publishing, manually inspect English and Japanese at desktop, tablet,
and phone widths, keyboard focus, menu behavior, media playback, and language
switching. Automated tests cover eight generated pages, reciprocal locale links,
internal links/assets, and seeking for all three videos. Browser rendering,
responsive geometry, playback interaction, and screenshots remain unverified
in this session because the in-app browser's connection-error document is
blocked by its URL policy. Do not treat source-level checks as visual QA.


## Presenter screenshot

`src/assets/presenter-window-v2-macos.png` replaces the older presenter UI in
both homepage locales and the README's macOS presenter example. It is an
889×519 crop of the actual v2.0.0 presenter window at 1 second in the original
2026-10-12 12-52-19 interactive demo recording, showing page 1 of the deck.
It includes the
first/previous/next/last icon controls and the 12-slide deck's speaker notes.
The screenshot retains the source recording's visible window area and pixel
geometry; no UI was recreated. The original recording's SHA-256 is
`b6726cf89d255da4bcb07bff7491d2e3bc47d2030404902badbf286eaf474e4e`.

## Audience Live v3.0.0 landing pages (PR1)

The English page is `/audience-live/` and Japanese is `/ja/audience-live/`.
Both homepages add a navigation-menu link only; their Hero, existing anchors,
AI/CLI sections, and v2.5.0 downloads are unchanged. This is a source-only PR for
the existing dependency-free static build and GitHub Pages workflow; no new
hosting, framework, runtime dependency, or deployment is introduced.

The page follows Hero, How It Works, Live Comments & Reactions, Choose Your
Connection, Getting Started, FAQ, and final CTA. English uses the adopted
"Turn your presentation into a conversation." copy. All download CTAs target
v3.0.0 RC1 and explicitly identify it as a testing prerelease with macOS Apple
Silicon DMG and Ubuntu amd64 DEB, not the final release or Store update.
Review those labels and destinations in both languages when final delivery is
confirmed. Official Relay is described as an Open Beta with free tickets only,
per the approved brief; no future pricing or service features are advertised.
The Shop remains authoritative for ticket conditions.

### Media and interaction

Unmodified GIFs and posters from PR #561 are independently vendored in
`src/assets/audience-live/`. See its [media notes](src/assets/audience-live/README.md)
for pinned provenance, GPL-3.0-only licensing, sizes, hashes, and recording limits.
There is a content/provenance relationship to #561, but no merge-order or build
dependency. Do not cherry-pick its README/CHANGELOG commits into this PR.

The Hero and comparison begin with still posters. `audience-live.js` loads GIFs
only on explicit Play, stops after 27 seconds, and restores posters on Stop,
tab changes, page hiding, and changes to the OS reduced-motion preference. There
is never autoplay, including for reduced-motion users. Hidden/inactive demos do
not keep animating. The comparison supports Left/Right/Home/End with a single
active tab stop, ARIA tab/panel relationships, and visible focus. Without JS,
both posters, original GIF links, native FAQ disclosures, and static content
remain accessible. Language selection reuses the existing `language.js`.

### Specification and link review

Reviewed on 2026-11-22 against the app's `docs/AUDIENCE_LOCAL.md`,
`docs/AUDIENCE_COMMENTS.md`, `docs/AUDIENCE_RELAY.md`, and `docs/PRIVACY.md`, plus:

- [Relay README](https://github.com/koichiro/quick-presenter-relay) and
  [self-hosting guide](https://github.com/koichiro/quick-presenter-relay/blob/main/docs/SELF_HOSTED.md):
  independent infrastructure, HTTPS for remote clients, TOML settings export.
- [Official Shop](https://shop.quickpresenter.com/): GET returned 200; confirms
  free tickets, presenter account/ticket flow, internet access, and no attendee
  account. HEAD returns 404, so use GET for this service's link checks.
- [npm registry](https://registry.npmjs.org/quick-presenter-relay/latest): confirmed
  published package `quick-presenter-relay` and Apache-2.0 licensing. npm's human
  package page returned 403 to automated requests; manual access remains a check.
- [RC1 release](https://github.com/koichiro/quick-presenter/releases/tag/v3.0.0-rc.1):
  verified `isPrerelease: true` and the DMG/DEB assets. GitHub CTA, documentation,
  repository, and issue links returned 200.

### Validation

`npm test` validates eight pages, reciprocal locales, metadata, links/fragments,
asset byte identity, initial still posters, and existing video range handling.
Run `node --check src/audience-live.js` for script syntax. No Rust/Slint/build
configuration for the native app is changed, so Cargo checks are outside PR1.

Local browser review covered English/Japanese at 1280px and 390px, with no mobile
horizontal overflow, loaded posters and GIFs, Play/Stop, stopping on tab change,
arrow-key tab selection, language switching, menu operation, and native FAQ.
Screenshots are saved locally under `artifacts/audience-live-pr1/` (ignored);
full pages in both locales plus Japanese desktop/mobile Hero captures are
available. Review copies of the Japanese Hero are tracked in
`review/audience-live/{desktop-ja,mobile-ja}.jpg` and are not deployed. Review screenshots and the page before publishing. Cross-browser
coverage, a real phone, OS reduced-motion toggle, and JS-disabled browser testing
remain manual release checks; static fallback/no-autoplay contracts are covered
by source checks and the automatic tests.

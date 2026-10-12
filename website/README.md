# Website deployment and custom domain

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

## Version 2.0 local release preview

The homepage introduces the stable `qp` interface alongside PDF playback:
source context, explicit presentation control, and NDJSON change observation.
The optional external narration runner is linked from the `v2.0.0` examples.
It is separate from the player; Quick Presenter does not bundle an AI model or
Kokoro. macOS automation requires the Developer ID / DMG distribution in v2.0.0.

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
pages and their language links remain usable; when storage is blocked, manual
choices cannot persist across visits.
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
autoplays or preloads video data. The hero and Open Graph use the actual
interactive demo preview instead of a generated UI.

The localized guides are `/guides/interactive-ai/` and
`/ja/guides/interactive-ai/`, with the repository guide at
`docs/INTERACTIVE_AI_PRESENTER.md`. The copies of
`docs/samples/quick-presenter-demo.pdf` (12 pages) and
`examples/autopresent/demo.pdf` (4 pages) in `src/assets/` come from v2.0.0
commit `926882b01befc0f9937ec93c7a182fc324975010`. Update their provenance when
replacing the samples. Keep source notes in Marp comments and export with
`--pdf-notes`; ordinary PDF export does not guarantee extractable notes.

The site documents `qp export --json`, `goto`, `status`, and `watch` from the
verified v2.0.0 CLI contract. Export provides text and notes, not images or OCR.
The external agent interprets data and chooses actions; restoration requires
a user request. The guide prompt checks document and session identity before
restoring a saved page.

## Release and local verification checklist

This website is intended for publication after the v2.0.0 release. Public copy
assumes v2.0.0 packages are available and guides users to matching GUI/CLI
binaries. Confirm the download destinations and Store package version when
publishing this prepared site.
GitHub Releases lists macOS DMG and Linux Debian packages; the Windows link
retains the existing Microsoft Store product ID. Store version availability
has not been independently verified. No Mac App Store review status or planned
price is asserted. Mac App Store CLI control is excluded in the v2.0.0 contract.

The presenter feature uses a static screenshot rather than an unpausable GIF.
All videos use native controls. Focus outlines, a native details-based compact
navigation menu, and reduced-motion rules are supplied. The `#scope` alias is
kept within PDF preparation; `#support` now includes open source and support.

Before publishing, manually inspect English and Japanese at desktop, tablet,
and phone widths, keyboard focus, menu behavior, media playback, and language
switching. Automated tests cover six generated pages, reciprocal locale links,
internal links/assets, and seeking for all three videos. Browser rendering,
responsive geometry, playback interaction, and screenshots remain unverified
in this session because the in-app browser's connection-error document is
blocked by its URL policy. Do not treat source-level checks as visual QA.

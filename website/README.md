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

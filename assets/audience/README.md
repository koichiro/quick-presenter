# Embedded shared audience web assets

`index.html` and `audience.js` are the unmodified `quick-presenter-audience-web`
0.0.1 assets. LICENSE and NOTICE apply to these Apache-2.0 assets; the application
retains its own GPL-3.0-only license. All four files are embedded in the executable.
`SOURCE.json` records their content fingerprints. No npm install or runtime CDN
is required. The browser regression test verifies those fingerprints.

Update HTML, module, license and notice together from a reviewed shared package,
review the source and regenerate the hashes. Do not patch a local browser fork;
changes belong in the shared module first. The local server implements the same
Audience role contract, while native session creation and event admission stay
in Rust. No relay server implementation or service configuration is bundled here.

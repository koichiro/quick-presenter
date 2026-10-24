# v3.0.0 post-merge macOS GUI smoke — 2026-10-24

Baseline: merge `baf643b`, incorporating the `v2.5.0` release into `v3.0.0`.
This record covers the accompanying notes-resize fixture correction and local
validation on macOS 27.0.1 (26A434), arm64. It is not release qualification for
other platforms or a notarized/distributed v3.0.0 package.

## Failure analysis

The unbundled debug executable reproduced the reported failure: rapid entry
cancellation received `will-enter` but no completion notifications, timing out
with counts `[1, 0, 0, 0]`. The same executable staged as an ad-hoc `.app` passed
all five native reversal cases without changing fullscreen implementation or
assertions. This matches the unbundled-run limitation previously recorded in
[the #491 investigation](ISSUE491_MACOS_FULLSCREEN_INPUT_20261021.md).

That bundled comparison exposed a second failure in the merged notes fixture.
The portrait/inline-menu resize still requested 800x560, omitting the 282px
Audience Live footer added by v3.0.0. Its applied notes viewport was -20x100,
with a zero-width slide surface. The resize waiter correctly rejected it.
Other notes fixtures already included the footer. This correction includes it
in the portrait request too, retaining the existing native-layout, fitted-font,
minimum-viewport, presentation-state and timer assertions.

The [GUI smoke instructions](../GUI_SMOKE_CHECKLIST.md#semi-automated-path) now
show a signed local `.app` procedure. No native fullscreen checks are skipped,
notification counts relaxed, or production fullscreen behavior changed.

## Validation

- `cargo fmt --check` and `cargo check`: passed. The existing `fetch_update`
  deprecation warning in `macos_renderer.rs` remains.
- Complete `cargo test`: passed in the normal host environment. The sandboxed
  attempt could not bind the loopback HTTP/IPC endpoints and is not a pass.
- Developer ID-signed debug `.app`, timestamp disabled, PDFium 8076 inherited
  from the local checkout: **256 GUI checks passed, zero failed**, including
  both portrait notes fixtures, native reversals, local Audience Live sessions,
  reactions/comments, Settings, PDF rendering and subsequent window checks.
- The final local bundle uses PDFium 8111, downloaded and SHA-256 checked with
  the merged `scripts/pdfium_manifest.json`. Both renderer and application
  copies include its metadata and licenses. Signing verification and the signed
  XPC render/notes plus private-file/network/child denial gate passed. The full
  GUI rerun also passed **256 checks, zero failed** with PDFium 8111. The signed
  GUI executable SHA-256 was
  `f3be1f685fddc6ea451f63e8d3a4d6214ec663c3bff6816a2cc8f21981926d01`.

Raw reports and temporary bundles remain outside the repository. This does not
claim physical-key, Store, relay-service, Windows or Linux runtime qualification.

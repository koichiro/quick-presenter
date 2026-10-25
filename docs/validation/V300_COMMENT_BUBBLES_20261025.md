# v3.0.0 comment bubbles — 2026-10-25

Base: `a6eab96` on `v3.0.0`. Local macOS arm64 debug validation.

## Passed checks

- `cargo fmt --check`, `cargo check`, `cargo build --bins`, and the final
  complete `cargo test` pass. Two existing tests remain ignored; the existing
  `macos_renderer.rs` `fetch_update` deprecation warning remains.
- Unit tests cover every flowing/bubble combination, admission/generation and
  duplicate suppression, anonymous names, bounded bursts, upward motion,
  idle timer termination, pause/reset, and preference persistence across relay
  import/reset.
- Developer ID-signed local debug `.app` passes signature verification and the
  XPC PDF render/notes plus private-file/network/child denial gate.
- Normal-launch native UI checks confirm translucent rounded gray cards,
  Japanese wrapping, small author labels, anonymous label omission,
  simultaneous flowing/bubble output, independent settings, and saved
  bubble-only preferences restored after restarting the app.
- The final native app preserves three bubbles while moving from page 0 to the
  PDF. Posting a fourth and fifth comment leaves the newest four in order,
  with the oldest removed and the newest at the bottom. The slide title remains
  visible through the cards. Test sessions were stopped and apps exited.

## Limits and unsuccessful attempts

The sandboxed test attempt could not bind local sockets. Host tests passed.
An intermediate full rerun timed out in the existing
`restart_is_bounded_malformed_responses_are_not_retried_and_later_pdf_recovers`
renderer integration test while GUI validation was also running. The final
complete rerun passed.

Full macOS GUI smoke, run both directly from the signed bundle and through
Launch Services, stops before PDF/comment checks at rapid fullscreen-entry
cancellation: `notifications=[1, 0, 0, 0]` without completion. This is the same
failure shape documented in the earlier fullscreen investigations; no native
notification assertions were relaxed. The new GUI comment assertions compile
but were not reached in these runs. Normal-launch UI checks above were
performed separately and do not establish a full GUI smoke pass.

Temporary bundles and raw reports remain under `/tmp/quick-presenter-bubbles*`.
This is not Windows/Linux runtime, physical mobile-client, Store, notarized
release, or full cross-platform qualification.

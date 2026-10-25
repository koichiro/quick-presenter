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

Before desktop unlock, full macOS GUI smoke stopped at rapid fullscreen-entry
cancellation: `notifications=[1, 0, 0, 0]` without completion. No native
notification assertions were relaxed.

## Unlocked desktop rerun and screenshots

After desktop unlock, the full signed-app GUI suite reached **229 passes**,
including all comment assertions on both an empty deck and a loaded PDF. It
then stopped in the later hidden-window fullscreen check:
`hidden-window fullscreen=true did not complete`, with notification counts
`[5, 5, 5, 5] -> [6, 5, 5, 5]`. Full GUI qualification remains incomplete.
Subsequent full-suite retries also reproduced the early native fullscreen
notification timeout.

A separately identified focused run against `docs/samples/quick-presenter-demo.pdf`
passed **117 checks, zero failures**. It covers Audience Live start/stop, real
loopback WebSocket comments/reactions, both comment styles together, bubble-only
mode, author separation, pause/resume and generation handling, both styles OFF,
page-0 continuity, four-card burst bounds/ordering, panel layout, and Settings.
It does not run or claim to pass native fullscreen checks.

The optional `QP_GUI_SMOKE_SCOPE=comments` selects this focused scope; leaving
it unset retains the full suite. `QP_GUI_SMOKE_BUBBLES_SNAPSHOT=/tmp/comments.ppm`
saves separate PPM snapshots for combined/two-bubble/four-bubble stages, with
`pdf` or `no-deck` in each filename. The existing
`QP_GUI_SMOKE_SETTINGS_SNAPSHOT` saves the Settings view.

These PNGs were converted from actual Slint window snapshots during the focused
run, without compositing or modifying their content:

| State | Screenshot |
| --- | --- |
| Flowing comments entering while a bubble is visible | [Combined styles](../assets/comment-bubbles/combined.png) |
| Two bubbles; anonymous and named author | [Two bubbles](../assets/comment-bubbles/two-bubbles.png) |
| Latest four after five posts; oldest removed | [Four bubbles](../assets/comment-bubbles/four-bubbles.png) |
| Independent display toggles | [Settings](../assets/comment-bubbles/settings.png) |

![Combined styles](../assets/comment-bubbles/combined.png)

![Two bubbles](../assets/comment-bubbles/two-bubbles.png)

![Four bubbles](../assets/comment-bubbles/four-bubbles.png)

![Settings](../assets/comment-bubbles/settings.png)

Temporary bundles and raw reports remain under `/tmp/quick-presenter-bubbles*`.
This is not Windows/Linux runtime, physical mobile-client, Store, notarized
release, or full cross-platform qualification.

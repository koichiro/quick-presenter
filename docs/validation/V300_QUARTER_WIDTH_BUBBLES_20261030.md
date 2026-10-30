# v3.0.0 quarter-width comment bubbles — 2026-10-30

Base: `7afdc2c` on `v3.0.0`. Local macOS arm64 debug validation.

Bubble cards now use one quarter of the Audience window width instead of one
third. The right margin, card height, five-card stack, and opacity are unchanged.

- `cargo fmt --check`, `cargo check`, `cargo test`, and `cargo build --bins` pass.
  Two existing tests remain ignored.
- A locally Developer ID-signed debug app passes the XPC PDF render/notes and
  private-file/network/child denial gate.
- The focused Audience Live/comments GUI run passes **123 checks, zero failures**,
  using real loopback WebSocket events and the repository demo PDF.
- The screenshot below captures five settled bubbles with flowing comments also
  enabled. It is an actual Slint Audience window snapshot converted from PPM to
  PNG without changing its content.

![Five quarter-width bubbles](../assets/comment-bubbles/quarter-width-five-bubbles.png)

Raw reports and the temporary app remain under `/tmp/quick-presenter-quarter-bubbles`.
This focused run does not qualify native fullscreen, Windows/Linux, or release
packages.

## Longer comments

Body text is reduced from 22 to 14 logical pixels, author labels from 14 to 11,
and inner padding from 12 to 10. Width, right margin, card height, and stack
spacing remain unchanged. Explicit author/body bounds replace the unconstrained
vertical layout; each card also clips its contents. This prevents a 200-character
body from extending over the following card.

The focused GUI run now sends five additional real comments: named Japanese
bodies of 69 and 95 characters, an anonymous 95-character body, a named
200-character body, and an English question. Both 95-character Japanese bodies
are fully visible in the demo-PDF snapshot; the 200-character body is elided
within its card. Full bodies and separate authors remain in the bubble model,
and the full 200-character body remains in presenter history.

The final focused run passes **125 checks, zero failures**. Formatting, compile,
binary build, and the complete host Rust tests pass; two existing tests remain
ignored. The sandboxed test attempt could not create local sockets; the host
rerun passes. The final signed app also passes the XPC render/notes and denial
gate. The same scope limits above apply. Final GUI reports and snapshots remain
under `/tmp/quick-presenter-readable-bubbles-final`; the host Rust test log is
`/tmp/quick-presenter-readable-bubbles/cargo-test-final.log`.

![Five longer quarter-width comments](../assets/comment-bubbles/quarter-width-long-comments.png)

## Increased line spacing

Body text now uses `line-height-factor: 1.4` to separate Japanese lines clearly.
Font size, card dimensions, and stack positions are unchanged. Compared with the
initial compact-text snapshot above, fewer lines fit: the named 69-character
Japanese comment and anonymous 95-character comment remain fully visible, while
the named 95-character and 200-character bodies are elided within their cards.
Full bodies remain available in presenter history.

Formatting, compile, binary build, and the complete host Rust tests pass, with
two existing ignored tests. The locally signed app passes the XPC render/notes
and denial gate and **125 focused GUI checks, zero failures**. The final snapshot
below was visually checked for Japanese line separation and bounded output.
The same scope limits above apply. Raw GUI artifacts remain under
`/tmp/quick-presenter-bubble-line-spacing`; the host Rust test log is
`/tmp/quick-presenter-bubble-line-spacing-tests.log`.

![Long comments with increased line spacing](../assets/comment-bubbles/quarter-width-long-comments-spaced.png)

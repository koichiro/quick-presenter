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

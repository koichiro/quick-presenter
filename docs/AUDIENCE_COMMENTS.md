# A7 Live Comments (initial implementation)

Audience Live ON accepts comments on PDF pages and page 0. Browsers show a handle
(default `Anonymous`) and body input. Presenter View shows the latest 20 accepted
comments, oldest first, in a scrollable area below notes. The audience window
shows the enabled comment overlays above the PDF/join guide. Comments and reactions
share a session but have independent display state and rate budgets.

The button immediately to the right of Audience Live is **Pause** while displaying
comments and **Resume** while paused. Pause clears active/pending comments instantly;
reception, history, comment logs, and reactions continue. Resume displays only
newly admitted events; queued old-generation deliveries still enter history.
Session start defaults to unpaused. Stop clears history and overlay state while
retaining rotated log files. Display preferences survive session restarts.

## Comment display settings (v3.0.0)

Open **Settings > Comment display** to toggle **Flowing comments** and
**Bubble comments** independently. Both may be ON together, or OFF together;
reception, presenter history, and logs continue with both OFF. Settings apply
immediately and persist in the existing configuration file:

```toml
[comment_display]
flowing = true
bubbles = false
```

The defaults preserve the existing flowing overlay. Disabling a style clears
its visible/pending items; enabling it waits for new comments without replaying
history. Relay import/reset preserves these preferences.

Bubbles are translucent gray rounded rectangles anchored to the lower right of
the Audience window. Each card occupies one third of the window width, with a
scaled right/bottom margin and 50% background opacity. Cards are 120 logical
pixels tall; the author and comment text remain opaque for readability. A smaller
author line appears above the wrapped comment;
empty/whitespace-only handles and case-insensitive `Anonymous` omit that line.
Each new bubble enters from below over 250 ms, moving older bubbles upward by
one slot. Four settled bubbles fit in the stack; the oldest leaves through its
clipped top as the next arrives. Bubbles stay in place until another comment
arrives, a style is disabled, comments are paused, or the session ends. Long
text is wrapped and elided within its fixed card, preserving the stack bounds.
The idle stack does not keep an animation timer running. Blackout covers both
styles; resizing preserves their logical geometry.

## Display engine

`audience_comments` owns a 1280 × 720 logical canvas independent of PDF geometry.
The flowing style uses six lanes with top coordinates `48 + lane * 64`, with 32 px bold text. Slint measures
the entire display string at its actual logical display font and weight before
admission. All comments enter at x=1280 and move left at 160 logical px/s. They
expire when `x + measured_width <= 0`. The next lane is selected cyclically after
the last used lane, allowing entry only when the previous comment's right edge
is at most 1248. Equal speed maintains at least a 32 px gap.

A FIFO queue holds at most 32 pending comments, expiring only after more than two
seconds since server validation/admission; active comments are capped at 24.
Queue/active overflow and expired comments still appear in presenter history and
logs. Animation uses monotonic elapsed time, including while hidden/minimized or
blacked out. Blackout covers the overlay without resetting its time. PDF/page-0
transitions do not clear, restart, or delay comments. Resizing uniformly scales
and centers a clipped canvas; no logical widths, lanes, start times, deadlines,
or speed change. No replay occurs when returning to a previous page.

`audience_ui` adapts bounded event batches and owns separate Slint models for
reactions, flowing comments, and bubbles. Network tasks never access UI/PDF state. A dedicated
bounded writer records JSON Lines with the existing secure size-rotation writer;
it stays idle after a session stops and drains at orderly app exit. Failures are
reported in the Audience status and never stop PDF playback.

## Initial policy choices

The A7 source specification leaves string limits, rate limits, anonymous matching,
and retention open. This implementation chooses the values documented in
[Audience Protocol](AUDIENCE_PROTOCOL.md) and [Privacy](PRIVACY.md). File logging
with rotation was explicitly requested for this implementation. There is no
cloud relay implementation, PDF distribution, content synchronization, or
comment-to-page association. This document describes behavior, not a patent or
legal assessment.

## Validation

Unit tests exercise normalization, validation, history bounds, duplicate event
suppression, constant motion/lifetime, lane spacing, queue expiration, pause
generations, admission limits, log rotation/permissions, and logging failure.
Loopback WebSocket tests cover authenticated comment reception, independent
rate budgets, and invalid input. GUI smoke checks actual font measurement,
resize/page-0 continuity, pause history, stale-generation delivery, and resume.
The v3.0.0 bubble checks additionally cover all four style combinations,
configuration persistence, anonymous names, bounded bursts, upward movement,
and pause clearing. GUI smoke uses an isolated temporary configuration file
and exercises both styles together, bubble-only mode, and both disabled.

Local macOS validation: Cargo formatting/checks passed; tests passed (510 passed,
1 existing ignored test). All 14 added A7 GUI checks passed. The final complete
GUI run had 234 passes and 12 macOS startup-restoration/edge-pixel failures;
a clean archived `origin/v3.0.0` baseline reproduced the identical 12 failures
with 220 passes. An earlier A7 run passed 244 checks before adding scroll-history
checks. These observations do not replace signed/Store, Windows/Linux GUI, or
physical mobile-client validation.

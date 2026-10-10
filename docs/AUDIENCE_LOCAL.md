# Local Audience Sessions

Audience Live targets v3.0.0 and is developed on the `v3.0.0` branch.

The initial Audience Live implementation lets audience browsers join a local
session hosted by Quick Presenter. It supports session start/stop, a join QR
code and URL, and an authenticated live connection count. Five reactions can be sent from the
mobile page and appear in Presenter View. Comments and presentation overlays
remain outside the A4 scope.

## Use

1. Connect the presenter computer and audience devices to the same LAN.
2. Select an IPv4 address with **Next address**, or rediscover interfaces with
   **Refresh** in the compact connection area below the notes.
3. Click **Audience Live ON** using the button between
   the notes and Audience Live area. This starts the server and shows the page 0
   QR code, session code, and join URL in the presentation window.
4. Scan the QR code with a phone. All five reactions are available as soon as
   the browser joins; no separate presenter reaction switch exists.
5. Advance once to restore the current PDF page. Reception continues.
   Page 0 appears when a session starts; there is no separate join-screen toggle.
   Page 0 does not modify PDF pagination,
   rendering state, or the timer. Blackout covers the join screen as well.
6. Click **Audience Live OFF** to invalidate the URL,
   clear the feed/join screen, and close all connections.

The thumbnail column extends to the bottom of the main content, alongside the
Audience area, and is clipped within its own viewport. The Audience area spans
only the notes/keys columns. Its compact connection controls take at most 280
logical pixels (35% at narrower sizes); the reactions feed receives the remaining
width and displays up to 36 recent reactions. The total reserved footer height
is 298 logical pixels, including the toggle row; preferred/minimum window height
increases by that amount to preserve the notes area. The full join URL appears below the panel in a read-only, selectable text area.
**Copy** copies the complete URL, and **Open URL** opens it in the browser without
interfering with text selection. Button labels describe the action to perform. Starting/stopping temporarily disables the toggle
until the asynchronous operation finishes. A new session resets the delivery
count, uses a new random secret, and may have a different OS-assigned port.
Sessions do not start automatically.

The connection count represents authenticated WebSocket connections, not unique
people: two browser tabs count twice. Backgrounded mobile browsers may be
disconnected by heartbeat timeout. They can reconnect from the page while the
session remains active, or scan the new QR after a restart.

## Network and security boundaries

- The listener binds only the selected IPv4 address. IPv6 is not supported yet.
- HTTP/WebSocket traffic is unencrypted. Use a trusted LAN; a session secret does
  not protect against someone observing network traffic.
- No router, firewall, or port-forwarding settings are changed automatically.
  Client isolation on venue Wi-Fi can prevent joining even on the same SSID.
- The server serves only its embedded audience page, not PDFs, notes, or files.
- The 256-bit secret travels in the URL fragment and then the first WebSocket
  message. It is not rendered into the HTTP page or written to settings/logs.
- HTTP Host and WebSocket Origin must match the selected listener authority.
- Authentication has a five-second deadline and at most 32 concurrent pending
  WebSocket authentications. WebSocket frames/messages are limited to 4 KiB.
- A session-wide handshake budget permits 20 attempts per second, including
  reconnects. This is a resource protection measure, not a participant limit.
- After authentication, only version 1 reaction messages with five known kinds
  are accepted. Invalid application messages close the connection. Heartbeats
  detect stale connections.
- Per connection: a burst of five reactions, refilling at two per second. Across
  the session: a burst of 120, refilling at 120 per second. The event queue holds
  at most 128 events, expires them after two seconds, and drains at most 16 per
  UI poll. Resource limits do not impose a participant-count product limit.
- See [Audience Protocol](AUDIENCE_PROTOCOL.md) for messages and response semantics.

These bounds cover the WebSocket/session layer. This initial implementation
does not claim protection against hostile network floods or unlimited slow HTTP
connections. Validate venue-scale load before relying on it in production.

## Runtime boundary

`audience.rs` owns a dedicated thread and single-thread Tokio runtime. It exposes
a latest-state snapshot and an independent stop signal; it never references
Slint, PDF state, or the renderer mailbox. `audience_ui.rs` owns UI callbacks and
a 100 ms polling timer that runs only during session startup/running/stopping.
Address discovery occurs when the stopped panel is opened. There is no listener,
network runtime, or audience polling timer while the feature is off.

Stop cancels the listener and drops the runtime, cancelling upgraded sockets as
well as pending authentication. The UI waits asynchronously for thread completion.
After the application event loop exits, explicit shutdown joins the worker.
Errors remain in the audience panel and do not replace PDF rendering status.

macOS bundles declare local-network usage. The sandboxed Store UI additionally
requests incoming-network permission; renderer and proxy entitlements are not
changed. Signed macOS builds, Windows firewall/MSIX behavior, Linux, and actual
iOS/Android LAN connections require separate release validation.

## Validation

Run `cargo fmt --check`, `cargo check`, and `cargo test`. Audience tests exercise
real loopback HTTP/WebSocket connections, secret validation, connection count,
disconnect/stop cleanup, listener release, restart secret rotation, Host/Origin
validation, and the handshake budget. Manual release checks should include
multiple network interfaces, denied network permissions, rapid start/stop,
mobile background/resume, and presentation navigation during connection churn.

The debug Unix GUI smoke also runs the existing presentation-control checks
while a loopback audience server is active. It verifies join-page delivery and
that stopping Audience leaves presentation control available. The production
control protocol remains unchanged and has no Audience administration commands.

### CLI baseline integration validation

Integrated the `v1.9.0` snapshot `21d729b` on 2026-10-09, retaining the
existing `qp` binary, Control Protocol v1, presentation control runtime, and
Store bookmark entitlement. The Audience development package is 3.0.0.
The CLI endpoint and audience listener remain separate; the LAN server never
exposes presentation-control commands.

The Cargo checks passed with 483 tests, and the staged CLI contract check passed
all 10 checks. The macOS debug GUI smoke passed all 80 checks, including the
combined local audience and presentation-control path. The additional checks
cover real automatic session start, QR/URL publication and stop cleanup,
page 0 metadata, PDF/timer preservation, return navigation, and rendered
blackout before and after opening a PDF. The GUI smoke layout
synchronization fix from #430 resolves the five notes-sizing failures observed
in the earlier validation. This does not qualify signed-package GUI behavior
or mobile/Windows/Linux interoperability.

A4 presenter-layout follow-up validation: 489 Rust tests, 96 GUI smoke checks,
and 10 CLI contract checks passed. GUI checks cover footer/thumbnail separation,
toggle placement and expanded reaction width at three sizes, as well as stopping,
restarting with rotated credentials, and immediate reception without a reaction
switch. One run failed the existing fullscreen-on check; the same GUI suite
passed on retry with unchanged assertions.

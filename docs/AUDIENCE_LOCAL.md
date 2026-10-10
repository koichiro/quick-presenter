# Local Audience Sessions

Audience Live targets v3.0.0 and is developed on the `v3.0.0` branch.

The initial Audience Live implementation lets audience browsers join a local
session hosted by Quick Presenter. It supports session start/stop, a join QR
code and URL, and an authenticated live connection count. Five reactions can be sent from the
mobile page, appear in Presenter View (A4), and animate over the presentation
window (A5). Comments remain outside this scope.

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
   Page 0 appears when a session starts; Previous from PDF page 1 returns to it
   while the session is ON. Previous at page 0 stays there; Next restores PDF
   page 1. There is no separate join-screen toggle.
   Page 0 does not modify PDF pagination,
   rendering state, or the timer. Blackout covers the join screen as well.
6. Click **Audience Live OFF** to invalidate the URL,
   clear the feed/join screen, and close all connections.

The thumbnail column extends to the bottom of the main content, alongside the
Audience area, and is clipped within its own viewport. The Audience area spans
only the notes/keys columns. Its compact connection controls take at most 300
logical pixels (40% at narrower sizes); the reactions feed receives the remaining
width and displays up to 36 recent reactions. The total reserved footer height
is 282 logical pixels, including the toggle row; preferred/minimum window height
increases by that amount to preserve the notes area. The full join URL appears alone below the panel as a clickable link that opens
the browser. The thumbnail viewport ends at the same bottom edge, with a visible
background and border even when the deck has few pages. Button labels describe the action to perform. Starting/stopping temporarily disables the toggle
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
connections. Real-LAN multi-participant load testing is outside A6 scope;
repeatable load validation uses local loopback connections.

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

## Reaction overlay (A5)

The presentation window draws reactions in a Slint layer above the PDF image
or the page 0 join information.
Original, bundled SVG icons keep the display independent of emoji font support;
the mobile page and presenter feed still use native emoji. Overlay updates do
not submit PDF render requests or modify pages, notes, or the timer.

At most 24 reactions are visible for 1.8 seconds from their server receive time.
Positions are normalized to the current slide window and recomputed by Slint on
resize, fullscreen, or display changes. Excess events are omitted, never queued
for later overlay replay. A 16 ms animation timer runs only while items remain;
the existing session poll remains 100 ms. Audience Live OFF, blackout, and
hide/close discard the display model immediately. Switching between page 0
and the PDF clears existing animations; new reactions appear on either screen. Restoring visibility also
establishes a receive-time cutoff so late delivery cannot replay suppressed
inputs. Presenter reception continues during blackout or slide hiding.

`audience_overlay` manages lifetime and density without Slint or PDF state.
`audience_ui` owns the Slint model and stops the animation timer when empty or
suppressed. Start/stop and application shutdown clear both timers and models.
The overlay takes no mouse or keyboard input. Physical multi-monitor changes,
platform packaging and mobile checks are tracked in
[A6 release validation](AUDIENCE_RELEASE_CHECKLIST.md).

A5 validation on macOS: `cargo fmt --check`, `cargo check --locked --offline`,
`cargo test --locked --offline` (492 tests), and the binaries build passed.
GUI smoke passed 118 checks, including actual SVG pixels, motion on resize,
immediate session-stop clearing with live overlays, blackout during incoming events,
hidden-window suppression, resumed input, expiry, page 0 before/after PDF open,
and the updated layout/session toggle checks.
The CLI contract check passed all 10 checks for application 3.0.0 / protocol 1.
These checks do not qualify physical display swapping or signed packages.

Presenter action/URL follow-up validation: A4 passed 489 Rust tests, 98 GUI
checks, and 10 CLI contract checks. A5 passed 492 Rust tests, 118 GUI checks,
and 10 CLI contract checks. GUI checks verify inactive/active action labels,
URL publication as a browser link, placement below the panel, aligned thumbnail
viewport bottoms, automatic
page 0 on session start, and PDF navigation without a join-screen toggle.

Address selection and refresh use compact buttons beside the IPv4 address.
The URL strip is 36 logical pixels high. Stopping disables page 0 navigation
immediately, even while the asynchronous server shutdown is pending.

Page 0 return navigation and compact controls validation: 489 Rust tests,
101 GUI checks, and 10 CLI contract checks passed on macOS. The GUI covers
returning from PDF page 1, the lower page 0 boundary, and forward navigation
without skipping, as well as compact inline address controls at three sizes.

A5 integration of page 0 return navigation passed 492 Rust tests, 121 GUI
checks, and 10 CLI contract checks on macOS, including
the original reaction overlay suppression on page 0 and resumption on PDF navigation.
Page 0 now accepts new overlays, verified by a live WebSocket reaction and
rendered SVG pixels in GUI smoke before and after PDF loading.

Page 0 overlay validation on macOS: GUI smoke passed 123 checks, including
live WebSocket reception and visible reaction pixels above the join screen
both before and after PDF loading. Blackout and hidden-window suppression
continue to pass.

The Audience Live toggle includes a thumbs-up icon for both actions. The adjacent
status lamp is green while the session is active and gray while stopped. The
button text names the action; the lamp indicates the current session state.

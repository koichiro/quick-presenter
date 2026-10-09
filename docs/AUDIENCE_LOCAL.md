# Local Audience Sessions

The initial Audience Live implementation lets audience browsers join a local
session hosted by Quick Presenter. It supports session start/stop, a join QR
code and URL, and an authenticated live connection count. Reactions, comments,
and animated audience-event overlays remain outside this scope.

## Use

1. Connect the presenter computer and audience devices to the same LAN.
2. Use the **Audience Live** controls permanently below the notes area. Select
   an IPv4 address with **Next address** or rediscover interfaces with
   **Refresh addresses**, then select **Start Session**.
3. Press **Audience** to start a session if needed and show the page 0 join screen in the presentation window.
   It contains the audience QR code, session code, and join URL, without host
   controls or connection diagnostics.
4. Scan the QR code with a phone. Keep the browser page open to stay connected.
5. Press **Return to PDF**, or advance once, to restore the current PDF page.
   The join screen is separate from PDF pagination; it does not modify the PDF,
   render generation, or timer. Blackout covers the join screen as well.
6. Select **Stop Session** to invalidate the link, clear the join screen, and
   close all connections.

The presenter window reserves 200 logical pixels below the notes and grows its
preferred/minimum height by the same amount, preserving the notes area. Start,
stop, address selection, session status, and connection count stay in this area.
Audience starts a session when none is running; dismissing page 0 keeps it active.
Click the presenter join URL to open the same audience page in your browser.
A separate reactions area is reserved for a future update; no reaction events
are implemented yet. A restarted
session uses a new random secret and may use a different OS-assigned port.
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
- Only authentication is accepted in this version. Other application messages
  close the connection. Heartbeats detect stale connections.

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
Store bookmark entitlement. The Audience development package remains 2.9.0.
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

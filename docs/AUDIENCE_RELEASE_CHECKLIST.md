# Audience Live release validation (A6)

Audience Live targets v3.0.0. A6 provides repeatable transport checks and a
release evidence checklist. Passing CI does not certify mobile LAN access,
physical display switching, or signed/Store-installed packages.

## Automated checks

Run `cargo fmt --check`, `cargo check`, and `cargo test`. For a focused transport
and overlay run, use `cargo test --bin quick-presenter audience`.
Build Binaries runs this suite on macOS, Windows, and Ubuntu, and is triggered
by audience modules, overlay assets, and Slint changes. Existing installed
Ubuntu GUI smoke also exercises Audience Live; macOS and Windows desktop
checks remain manual.

The multi-client test keeps 48 authenticated WebSockets open, sends six
reactions per client concurrently, verifies acknowledged request IDs and
bounded UI delivery, then stops with clients still connected. It checks
connection cleanup, event disposal, and immediate listener rebinding.
Joins are paced to respect the handshake budget. This is a reproducible
loopback stress case, not a supported audience-size limit or a venue benchmark.
A separate test stops with eight clients still waiting for authentication.

For desktop GUI smoke, build the binaries and run:

```sh
quick-presenter --gui-smoke docs/samples/quick-presenter-demo.pdf \
  --gui-smoke-report audience-gui-smoke.txt
```

Use the final package's executable when validating packaging. Keep PDFium
overrides unset for packaged checks. Never record a join URL with its secret
in shared reports, screenshots, or CI logs.

## Evidence record

For each supported platform, record the app version and commit, OS version,
package source/signing status, Build Binaries run URL, device/browser versions,
network type, result, and relevant redacted evidence. Mark unexecuted checks
**pending**, never passed. A failure must include reproduction and recovery.

| Gate | macOS | Windows | Ubuntu |
| --- | --- | --- | --- |
| Focused audience tests in Build Binaries | Pending CI | Pending CI | Pending CI |
| Final installed package starts/stops LAN session | Pending | Pending | Pending |
| Mobile join and all five reactions | Pending | Pending | Pending |
| Page 0, PDF, blackout, hide/show, fullscreen | Pending | Pending | Pending |
| Physical displays: swap, detach, reconnect | Pending | Pending | Pending |
| Network loss/recovery and multiple addresses | Pending | Pending | Pending |
| Measured venue load and presentation responsiveness | Pending | Pending | Pending |
| Application exit releases sockets; restart rotates secret | Pending | Pending | Pending |

Developer-build macOS/iPhone reception and macOS GUI smoke have been checked
during A4/A5. They do not replace the final-package gates above.

## Installed package and LAN checks

1. Install the final signed/notarized macOS app, Microsoft Store Windows app,
   or Ubuntu `.deb`. Open the sample PDF, navigate, and verify notes/timer.
2. Turn Audience Live ON. Verify page 0 shows QR, URL, and session code. Check
   the presenter URL opens the join page and the thumbnail area remains usable.
3. From iPhone Safari and Android Chrome on the same LAN, scan the QR and send
   each reaction. Confirm connected count, presenter feed, and overlays on
   page 0 and PDF slides. Use a second desktop browser as another participant.
4. Confirm Next returns from page 0 without skipping the PDF, and Previous
   from PDF page 1 returns to page 0. Test blackout and hide/show during incoming
   reactions: suppressed inputs must not reappear later.
5. Stop with browsers connected. Confirm overlays/feed/credentials clear and
   browsers disconnect. Restart: use the new QR; the old URL must not rejoin.
6. Exit while clients are connected. Confirm the previous listener no longer
   accepts connections and a new process can start a fresh session.

On macOS, verify local-network permission and incoming firewall behavior for
the actual signed distribution. On Windows, check the installed package's
network permissions and Windows Firewall on private and public profiles.
On Linux, check firewall and desktop-session behavior. Record denied access
and recovery without requiring users to disable their firewall globally.

## Network and physical display checks

- With Wi-Fi and Ethernet, verify Next Address switches candidates when off
  and keeps the same selection with only one address. Refresh must update the candidate
  list. During an active session these controls are disabled; stop before
  changing the listener address.
- Disconnect the selected network during a session. PDF navigation, notes,
  and timer must remain usable. Reconnect, stop/start if necessary, and verify
  a fresh QR works. Test sleep/wake and browser background/foreground too.
- On isolated guest Wi-Fi, record that direct LAN join may be unavailable;
  presentation playback must continue. A successful loopback join does not
  establish mobile reachability.
- With two physical displays, test fullscreen, display swapping, disconnect,
  and reconnect while reactions arrive. Check QR readability, overlay bounds,
  keyboard controls, and continued PDF rendering. Capture which screen receives
  the presentation window after each transition.

## Venue load checks

Use a controlled LAN and anonymous synthetic events. Record participant count,
join ramp rate, reaction rate, test duration, machine specification, CPU/memory,
and observed presentation/control latency. Start with 20 concurrent clients;
then test 100 and 300 where resources permit. These are test scenarios, not
Local/Self-hosted product caps. Pace connections to respect admission budgets.

Confirm bounded overlays (24 items), event expiry, acknowledged rate limiting,
and no stale replay after a burst. Navigate PDFs and toggle blackout throughout.
Verify memory stabilizes after disconnects and stopping, then repeat sessions.
Treat busy/rate-limited replies as expected load shedding, not guaranteed
on-screen delivery. Do not claim a supported capacity from loopback tests.

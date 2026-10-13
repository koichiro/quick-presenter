# Experimental Audience Live relay client

This v3.0.0 milestone adds the qp side of Relay Protocol `1-draft.1`, plus a
loopback fixture for testing. The relay project's initial artifacts are contract
schemas and fixtures; this milestone does not claim a production self-hosted
server exists or that the draft protocol is stable.

## Run a local interoperability test

From the repository root, start the fixture in one terminal. The configuration
file must not already exist; choose a new path for each fixture process.

```sh
mkdir -p artifacts
cargo run --example audience-relay-fixture -- artifacts/relay-config.json
```

In another terminal, start the GUI with the generated configuration:

```sh
QUICK_PRESENTER_AUDIENCE_RELAY_CONFIG="$PWD/artifacts/relay-config.json" \
  cargo run --bin quick-presenter -- examples/autopresent/demo.pdf
```

Use an existing presentation PDF instead of `examples/autopresent/demo.pdf` if that file is
not available. On PowerShell, set the environment variable before launching:

```powershell
$env:QUICK_PRESENTER_AUDIENCE_RELAY_CONFIG = (Resolve-Path artifacts/relay-config.json).Path
cargo run --bin quick-presenter -- path/to/presentation.pdf
```

1. Click Audience Live ON. The presenter address label reads `Relay · draft protocol`.
2. Open the join URL from qp in a browser on the same computer. Loopback URLs in
   this fixture cannot be accessed from a phone or another computer.
3. Send reactions and comments. `accepted` means qp admitted the event into its
   bounded queue/log pipeline, not that animation or durable disk writing finished.
4. Check the connected count, reaction animation, original comment handle and
   twenty-entry comment history. Pause removes active/pending comment overlays;
   incoming comments still enter history and the existing comment log.
5. Resume. Comments offered under an earlier display epoch must not reappear.
6. Turn Audience Live OFF, then ON. The same room credentials resume with a new
   presenter epoch; old events are not replayed. Stop the fixture to check that
   Audience Live reports a disconnect while PDF playback remains usable.

Unset the environment variable and restart qp to use Local Network again.
The GUI smoke harness always uses the local transport.

## Credentials and transport

The file is the draft create-response object with exactly `room_id`, `code`,
`join_url`, `presenter_url` and `presenter_token`. It is read and validated on the
network worker when a session starts. A missing/invalid file fails that session;
there is no silent fallback to a local server. The GUI never displays the
presenter token. The audience secret is carried by the join URL fragment.

The fixture creates fresh random role-separated tokens and writes a new file
with owner-only permissions on Unix. qp rejects group/world-readable files on
Unix. On Windows place the file in a user-controlled directory with suitable ACLs.
The ignored `artifacts/` directory avoids committing test credentials. Never paste
real configuration files into PRs or logs. Error messages omit tokens and paths.

HTTPS/WSS is required except for loopback HTTP/WS test endpoints. TLS uses WebPKI
roots with normal certificate verification; no insecure certificate bypass is
provided. Join and presenter URLs must share a host/port, with no userinfo or
query tokens. Creation/end HTTP APIs, credential persistence, automatic reconnect
and custom trust roots are outside this milestone. OFF disconnects qp; it does
not permanently end a server-side room. Use ON to explicitly resume the existing
room, or restart the fixture with fresh credentials for a new test room.

## Integration and limits

`AudienceSession` delegates the existing session handle operations to local or
remote implementations. The remote worker owns its Tokio runtime and WebSocket;
Slint/PDF state never enters it. Queue/log admission and overlay engines are shared.
No network runtime starts until Audience Live ON.

Incoming JSON is bounded to 4096 bytes, rejects duplicate keys at every depth,
unknown fields and unsupported versions. Identifiers, canonical decimal epochs,
comment scalar/UTF-8/control limits and receiver-relative offer TTL are checked.
The dedup cache retains at most 256 event results for 30 seconds; saturation
returns busy without evicting a live cache entry. Log participant IDs are local
numeric hashes of opaque relay connection IDs, not persistent audience identities.

qp seeds its display epoch from the presenter welcome, publishes a newer display
snapshot, and suppresses comment overlay until the relay confirms it. State
revisions never move backward. The local generation fences queued comments
immediately on Pause/Resume. Receiver-relative expiry follows the original offer
through UI and lane waiting; it is not renewed when the UI drains the queue.
This is not a guarantee of browser-to-screen age under arbitrary network delay.

The worker bounds connect/auth/send waits and heartbeat inactivity. Cancellation
interrupts pending network work. Disconnect clears queued events and join metadata;
manual restart authenticates again, without replaying failed submissions.

The fixture binds only `127.0.0.1`, supports one transient room and bounded queues,
and implements only enough behavior for these tests. It has no room lifecycle API,
persistence, hardened input/parser handling or production reliability guarantees.
It is a qp test aid, not a replacement for the relay runtime.

## Automated validation

```sh
cargo fmt --check
cargo check --all-targets
cargo test
```

The relay tests include pinned Apache-2.0 OSS wire fixtures, malformed/duplicate
JSON, receipt admission, duplicate delivery, expiry, stale state/display epochs,
Pause/Resume fencing, worker cleanup and actual browser-socket → fixture → qp →
receipt round trips. GUI animation still needs the manual test above.

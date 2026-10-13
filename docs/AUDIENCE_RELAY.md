# Experimental Audience Live relay client

This v3.0.0 milestone adds only the qp client for Relay Protocol `1-draft.1`.
qp remains GPL-3.0-only. Relay server code, browser assets, shared wire fixtures and
their licensing belong to the separate `quick-presenter-relay` project.

The proposed replacement for the manual room-file setup is documented in
[Audience relay configuration and room creation](AUDIENCE_RELAY_ROOM_CREATION.md).
That proposal is not implemented by the current client.

## Run a local interoperability test

In `quick-presenter-relay`, start its development fixture. The configuration path
must be new; use an absolute path accessible to qp.

```sh
npm ci
npm run fixture -- /absolute/path/to/relay-config.json
```

Then, from the qp repository, launch the GUI:

```sh
QUICK_PRESENTER_AUDIENCE_RELAY_CONFIG="/absolute/path/to/relay-config.json" \
  cargo run --bin quick-presenter -- examples/autopresent/demo.pdf
```

On PowerShell, set the variable before launching:

```powershell
$env:QUICK_PRESENTER_AUDIENCE_RELAY_CONFIG = "C:\path\to\relay-config.json"
cargo run --bin quick-presenter -- path/to/presentation.pdf
```

1. Click Audience Live ON. The address label reads `Relay · draft protocol`.
2. Open the join URL on the same computer. The fixture is loopback-only.
3. Send reactions/comments and check animation, original handles, history and count.
4. Pause removes current/pending comment overlay; reception/history/logging continue.
   Resume must not display comments offered under an old epoch.
5. OFF disconnects qp. ON resumes with the same credentials and a new presenter
   epoch, without replay. Stopping the external fixture must leave PDF playback usable.

Unset the environment variable and restart qp for Local Network. The GUI smoke
harness always uses the local transport. For automated interoperability, use a
fresh external fixture with no other presenter or audience connections:

```sh
QUICK_PRESENTER_AUDIENCE_RELAY_CONFIG="/absolute/path/to/relay-config.json" \
  cargo test --bin quick-presenter \
  audience_relay::fixture_tests::external_relay_round_trip_and_presenter_resume -- --ignored
```

This opt-in test uses real audience and presenter sockets and checks native overlay
engine input and presenter resume. It does not start or embed a relay in qp.

## Credentials and transport

The file is the draft create-response object with exactly `room_id`, `code`,
`join_url`, `presenter_url` and `presenter_token`. It is read and validated on the
network worker when a session starts. A missing/invalid file fails that session;
there is no silent fallback to a local server. The GUI never displays the
presenter token. The audience secret is carried by the join URL fragment.

The external fixture supplies separate audience and presenter credentials. qp
rejects group/world-readable files on Unix. On Windows place the file in a user-controlled directory with suitable ACLs.
Keep credential files outside tracked source. Never paste real configuration
files into PRs or logs. Error messages omit tokens and paths.

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

## Automated validation

```sh
cargo fmt --check
cargo check --all-targets
cargo test
```

The default qp tests use small synthetic presenter peers to check client parsing,
admission, deduplication, expiry, display fencing and worker cancellation. Shared
protocol/schema conformance stays in the relay project. The opt-in external relay
test above checks actual interoperability; GUI animation still needs manual testing.

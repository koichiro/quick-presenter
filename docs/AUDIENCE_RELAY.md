# Audience Live relay client

qp implements Relay Protocol `1-draft.1` with TOML relay configuration, HTTP room
creation/termination and an outbound presenter WebSocket. qp remains GPL-3.0-only.
Server code, browser assets and shared protocol fixtures belong to the separate
`quick-presenter-relay` project. See the [configuration and lifecycle contract](AUDIENCE_RELAY_ROOM_CREATION.md).

## Configure once

Place `config.toml` in the standard qp configuration directory:

| Platform | File |
| --- | --- |
| macOS | `~/Library/Application Support/Quick Presenter/config.toml` |
| Windows | `%APPDATA%\Quick Presenter\config.toml` |
| Linux | `$XDG_CONFIG_HOME/quick-presenter/config.toml`, otherwise `~/.config/quick-presenter/config.toml` |

```toml
[audience]
transport = "remote"
relay = "https://relay.quickpresenter.com"
create_token = "REPLACE_WITH_THE_RELAY_CREATION_TOKEN"
```

The example credential is intentionally invalid. Use the relay operator's creation
credential, not a per-room presenter token. On Unix, the file must be owned by the
current user and have no group/world access (`chmod 600`). On Windows, put it in a
user-private directory with restrictive ACLs. Never commit or log a real credential.

No default file, no `[audience]` section, or `[audience]` with only
`transport = "local"` selects local hosting. Invalid configurations report an
Audience error without falling back to local hosting or interrupting PDF playback.
Restart after configuration edits; no settings GUI or hot reload is provided.

For debugging, replace the default configuration entirely:

```sh
quick-presenter --config /absolute/path/to/config.toml presentation.pdf
```

`--config` is a GUI executable option. The separate `qp` presentation-control CLI
continues to control the running GUI; it does not change its configuration.
The old `QUICK_PRESENTER_AUDIENCE_RELAY_CONFIG` variable no longer selects the
GUI transport. GUI smoke tests always inject local settings.

## Session operations

ON creates a room, authenticates the presenter and synchronizes display state,
then displays its join URL, code and QR. Reactions/comments use the existing
Audience and overlay engines. Pause immediately clears comment overlays while
reception/history continue; Resume never replays paused submissions.

OFF clears join metadata/overlays and attempts to end the room with its presenter
credential. The next ON creates a new room. An interrupted creation or failed
connection keeps its idempotency key in memory; manual retry recovers/revalidates
that room. Requests have bounded deadlines and retries. Failed termination can
leave a room until relay expiration. Application restart does not restore a room.
Networking runs on the existing dedicated worker and does not block live Slint/PDF
processing; no network runtime starts while OFF. Exit performs bounded cleanup
after the GUI event loop returns.

HTTPS/WSS is required remotely. Literal loopback HTTP/WS is supported for tests.
Configured and returned origins must match. HTTP redirects and environment proxies
are disabled. HTTP and WebSocket TLS use WebPKI roots; custom trust roots, certificate
bypasses, proxy settings and automatic infinite reconnect are not supported.

Incoming JSON is limited to 4096 bytes, rejects duplicate/unknown keys and unsupported
versions, and validates credentials, epochs, comment bounds and receiver-relative
TTL. The event dedup cache holds up to 256 results for 30 seconds. Log participant
IDs are local numeric hashes of opaque connection IDs. Neither HTTP management
nor WebSocket messages contain PDF contents, paths, notes or presentation commands.

## Interoperability test

In the relay project, build and start the real loopback runtime with temporary storage:

```sh
npm ci
npm run fixture -- --qp-config /absolute/path/to/new-config.toml
```

It writes an owner-only, non-overwriting TOML configuration; startup does not create
rooms. Launch qp with `--config` and enable Audience Live. Loopback links work on
the same machine only. Mobile access needs the relay's HTTPS reverse-proxy origin.

For automated interoperability in the qp repository, keep the runtime running:

```sh
QUICK_PRESENTER_TEST_CONFIG=/absolute/path/to/new-config.toml \
  cargo test --bin quick-presenter \
  audience_relay::fixture_tests::external_relay_room_creation_round_trip_and_termination -- --ignored
```

The test creates rooms over HTTP, sends reactions/comments through real sockets,
checks engine/overlay inputs, ends a room, and creates a different one. Synthetic
unit peers cover malformed responses, authorization failures, same-key retries,
response loss and cancellation. Shared schema conformance remains in the relay
project. Rendered QR scanning and actual GUI animation still need manual validation.

```sh
cargo fmt --check
cargo check --all-targets
cargo test
scripts/coverage.sh
```

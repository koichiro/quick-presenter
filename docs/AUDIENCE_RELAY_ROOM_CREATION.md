# Audience relay configuration and room creation proposal

Status: proposed design for v3.0.0. This document does not implement the CLI,
configuration loader, or room-management client described below.

## Goal and current implementation

Configure a relay once in a user-owned TOML file, then use the existing Audience
Live ON/OFF control. qp creates a room, connects its presenter socket, and displays
the returned join URL, session code, and QR code. No relay settings GUI is needed.
PDF playback must remain usable through configuration and network failures.

The existing client in `src/audience_relay.rs` consumes a pre-created room JSON
file selected by `QUICK_PRESENTER_AUDIENCE_RELAY_CONFIG`. `AudienceSession` selects
local or remote transport; `AudienceUi` owns the controls and overlay integration.
The remote session already owns a dedicated thread and current-thread Tokio
runtime. Reuse those boundaries and the existing event validation, queues, pause
fencing, heartbeat, and QR rendering.

The relay exposes authenticated creation and termination HTTP APIs. An endpoint
alone is not sufficient with its current contract: creation requires the relay
operator's `createToken`. qp stores that credential as `create_token` in its local
configuration. It never needs the relay's database configuration or storage key.

## Configuration contract

The default file is `config.toml` in the directory already resolved by
`settings_storage::platform_config_dir()`:

| Platform | Default file |
| --- | --- |
| macOS | `~/Library/Application Support/Quick Presenter/config.toml` |
| Windows | `%APPDATA%\Quick Presenter\config.toml` |
| Linux | `$XDG_CONFIG_HOME/quick-presenter/config.toml`, otherwise `~/.config/quick-presenter/config.toml` |

The file is separate from existing UI settings and recent-file storage. qp does
not generate credentials or rewrite this file. The normal user configuration is:

```toml
[audience]
transport = "remote"
relay = "https://audience.example.com"
create_token = "REPLACE_WITH_THE_RELAY_CREATION_TOKEN"
```

The example token is deliberately invalid. Use the creation credential supplied
by the relay administrator, not a room's `presenter_token`.

Local hosting can be selected explicitly:

```toml
[audience]
transport = "local"
```

Rules:

- Missing default file, absent `[audience]`, or omitted `transport` selects local
  hosting. Omitted `transport` is allowed only when relay-specific fields are absent.
- Remote mode requires both `relay` and `create_token`. Local mode rejects those
  fields. Unknown fields, unsupported modes, duplicate TOML keys, invalid URLs,
  and malformed credentials produce a configuration error.
- `relay` is an origin, with an optional trailing slash; reject credentials,
  non-root paths, query strings, and fragments. Reverse proxies must expose the
  protocol routes at the origin root for this milestone.
- Require HTTPS; allow HTTP only for literal loopback IP addresses used for tests.
  Validate returned URLs using the existing room credential checks, additionally
  requiring the configured origin. No redirect following or TLS verification bypass.
- Limit the file to 16 KiB and read through an opened file handle. On Unix, require
  current-user ownership and no group/world access for a remote credential file.
  On Windows, use a user-controlled directory with restrictive ACLs; do not claim
  Unix mode checks provide Windows ACL validation.
- No environment interpolation, includes, automatic configuration discovery in the
  working directory, credential logging, or hot reload. Restart qp after editing.

`--config <PATH>` selects exactly one TOML file for debugging or alternate setups:

```sh
qp --config /absolute/path/to/test-config.toml presentation.pdf
```

The explicit file replaces the default file; there is no merging or fallback.
Resolve relative paths against the launch working directory. Reject duplicate
`--config`, missing/empty values, and another option used as its value. Support
non-UTF-8 paths through the existing `OsString` CLI parser. Both executable entry
points use the same parser. `--help` must not read configuration or start networking.

An absent explicit file or malformed default file is an Audience configuration
error, never an implicit switch to local hosting. Report a short, redacted error
through the Audience status control while allowing PDF startup and playback.
Resolve configuration off the UI thread during initialization; disable only Audience
start until resolution completes. GUI smoke tests inject local settings and remain
independent of a developer's user configuration.

Remove the experimental environment-variable selection from the normal GUI path
when this flow lands. Keep pre-created JSON loading only as an explicit integration
test helper, if still needed. Do not let environment variables silently override
TOML or `--config`. Update `docs/AUDIENCE_RELAY.md` and interoperability instructions
with the implementation.

## Boundaries and dependencies

| Module | Responsibility |
| --- | --- |
| `cli.rs` | Parse `config_path`; no file or network IO |
| New `app_config.rs` | Resolve path, bounded TOML parsing, mode and credential validation |
| `settings_storage.rs` | Reuse the existing platform config directory resolver |
| New `audience_relay_http.rs` | Bounded create/end requests, response validation, redacted errors |
| `audience_relay.rs` | Own room credentials, worker, socket, retry identity, and lifecycle |
| `audience_session.rs` | Select a typed local/remote configuration rather than an optional JSON path |
| `audience_ui.rs` | Start/stop, progress/error presentation, join metadata and overlays |

Keep secrets in Rust-owned configuration/session state. Do not derive unrestricted
`Debug` for credential-bearing types, expose tokens as Slint properties, or put
credentials into diagnostics. Existing UI settings should not serialize them.
Create requests and room credentials do not contain PDF paths, notes, or slides.

Add a TOML parser and an async HTTP client with Rustls and JSON support, using
minimal features. Evaluate `toml` and `reqwest` against the repository's Rust and
platform requirements during implementation. Do not introduce another runtime,
HTTP framework, hand-written HTTP parser, or synchronous client on the UI thread.
The HTTP client and presenter socket must use compatible certificate trust
behavior. Proxy behavior must be explicit and covered in documentation/tests;
it must not leak bearer credentials through redirects or diagnostic logging.

## Room creation and connection

1. ON starts the remote worker and publishes `Creating room…`. There is no network
   runtime, HTTP request, or heartbeat while Audience Live is OFF.
2. Generate a cryptographically random idempotency key for this start operation.
   Submit `POST /api/relay/draft-1/rooms` with `Authorization: Bearer <create_token>`,
   JSON content type, and `{ "idempotency_key": "..." }`.
3. Accept 201 for a new room and 200 for an idempotent recovery. Validate a bounded
   response with exactly `room_id`, `code`, `join_url`, `presenter_url`, and
   `presenter_token`. Reuse the strict duplicate-key parser and existing token/URL
   validation rather than trusting server-supplied connection locations.
4. Connect and authenticate using the returned presenter credential. Publish join
   metadata only after welcome and initial display-state synchronization succeeds.
   The existing UI creates the QR code from `join_url`; no intermediate file is needed.
5. Feed events into the existing Audience engine and overlay path. Errors clear join
   metadata and never affect PDF rendering or presentation controls.

Use an overall creation budget of 15 seconds, at most three attempts, per-attempt
deadline up to five seconds, cancellable backoff, and a 4 KiB response limit.
Retry transport failures, ambiguous response loss, and temporary 503 errors with
the same key. Authentication/validation/denial failures fail promptly and do not
retry automatically. Do not display server response bodies or denial details
verbatim. Distinguish authentication, invalid configuration, unavailable relay,
unsupported protocol, room expiration, and cancellation with safe messages.

A timeout does not prove that creation failed. Keep the operation key and any
validated room credentials in application memory across manual retry of a failed
start. Recover the same room instead of allocating a replacement. A terminal-room
410 requires a fresh key on the next explicit ON. A create/connect failure must
not silently fall back to the local server. No automatic infinite reconnect loop
is included in this milestone.

## Lifecycle and termination

```text
Off -> Creating -> Connecting -> Synchronizing -> Connected
           |           |              |              |
           +-----------+--------------+--------------+-> Failed
           +-----------+--------------+--------------+-> Stopping -> Off
Failed -> explicit ON -> recover pending creation / reconnect existing room
```

OFF means end this Audience session, rather than suspend it. Immediately fence
events, clear overlays and QR metadata, and prevent further user starts until the
worker finishes. Send `POST /api/relay/draft-1/rooms/{room_id}/end` using the
room's presenter Bearer credential. This is different from the operator creation
credential. Bound termination to two seconds, then close the socket and runtime.
The next ON starts a new room and gets a new join URL. Pause Comments continues to
change display policy only; it does not terminate a room.

If OFF arrives during creation, cancel normal admission/connection. When a room
response is already available, attempt termination. If creation may have committed
but its response was lost, attempt bounded recovery with the same key and end the
recovered room within the cleanup budget. Do not create a replacement room merely
to finish cleanup. An unresolved cleanup can leave a server-side room until its
existing disconnection grace/deadline expires; do not claim cancellation rolls back
an HTTP request.

Application exit uses the same best-effort, bounded cleanup without waiting on
the UI thread for HTTP. Coordinate with the existing session shutdown path;
do not join a worker that is waiting for a UI callback. Process crashes and hard
kills rely on relay expiration. This milestone does not persist room credentials
or idempotency keys, resume rooms after application restart, or guarantee remote
termination during outages. An explicit stop invalidates its operation generation
so late replies cannot reactivate the UI or become a later session's room.

## Implementation sequence and acceptance

1. **Configuration:** typed mode, standard path resolution, `--config`, strict
   bounded TOML loading, redacted failures, and explicit local smoke injection.
2. **HTTP lifecycle:** create/end client, stable retry key, limits/cancellation,
   and origin-bound room response validation on the existing network worker.
3. **Integration:** ON creates/connects, OFF ends, retry recovers an ambiguous start,
   existing pause/event/QR behavior remains shared, and failure isolation stays intact.
4. **Validation and documentation:** replace the manual fixture JSON setup in the
   GUI instructions with a TOML setup and run interoperability against the relay's
   real self-hosted runtime. Keep all examples synthetic.

These are implementation tasks for the follow-up code PR, not GitHub issues.
This design PR changes documentation only.

Required tests for that code PR:

- CLI priority, duplicates, missing values, platform path resolution, absent default
  vs absent explicit file, invalid TOML, size bounds, secret permissions, and no
  fallback from an invalid remote configuration.
- Authenticated create and end, 200/201 handling, same-key retries after a server
  commits but drops the response, authentication/denial failures, 503 budgets,
  bounded response parsing, cross-origin responses, and redirect rejection.
- OFF during creation/connect/synchronization, stale completion rejection, failed
  end, worker shutdown deadlines, manual retry, new room after explicit OFF, and
  no network activity while OFF. PDF behavior remains independent of these failures.
- End-to-end: TOML -> ON -> room -> QR/join -> audience reaction/comment -> qp event
  engine; pause and OFF -> termination -> new ON with new credentials. Automated
  socket tests do not substitute for checking the rendered QR/overlay manually.

Run `cargo fmt --check`, `cargo check`, `cargo test`, and the coverage check required
by CI. Preserve the existing 80% coverage threshold. Verify supported Windows,
macOS, and Linux CI targets. Document any certificate/proxy/platform limits found
during implementation instead of adding insecure bypasses.

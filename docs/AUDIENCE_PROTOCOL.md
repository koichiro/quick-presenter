# Audience protocol `1-draft.1` (Local)

Audience Live development targets v3.0.0. The protocol carries events and session
metadata only; it exposes neither PDFs nor presentation-control commands. This
is the local audience connection contract, not a complete remote relay contract.

The embedded browser is the shared `quick-presenter-audience-web` module. Its
HTML, JavaScript, styles, keyboard behavior and status text match the same package
used by compatible relays. Apache-2.0 notices are scoped to those assets; qp
remains GPL-3.0-only. The local listener does not implement relay room-creation,
operator/presenter WebSocket or persistence APIs: native Rust owns its session.

The join page obtains a 256-bit base64url secret from the URL fragment, retains
it only in tab-scoped storage and removes the fragment after successful storage.
Reload can reuse it while the same session remains active. If storage is blocked,
the fragment remains. It opens `/ws/audience/{session-code}` on the same authority.
The first text message is:

```json
{"protocol":"qp.audience","version":"1-draft.1","type":"hello","token":"<43-character-secret>"}
```

The five-second authentication deadline, Host/Origin validation, heartbeat and
4096-byte frame/message bounds remain. The old `/ws` endpoint and `{v:1}` network
messages are unsupported. Existing links are ephemeral and session restart issues
a new code/secret. There is no draft/stable version negotiation or downgrade.

After authentication the server sends a complete welcome:

```json
{"protocol":"qp.audience","version":"1-draft.1","type":"welcome","connection_id":"1","capabilities":["reactions","comments"],"available":true,"limits":{"max_message_bytes":4096,"request_cache_ms":30000}}
```

The browser validates complete welcome identity/limits, exact fields, version,
duplicate decoded keys, canonical counters and capability Unicode before enabling
controls. Malformed messages close the socket without accepting a pending draft.
Unknown well-formed capability names do not introduce new message types. Current
reaction kinds are `thumbs_up`, `heart`, `applause`, `laugh` and `question`.
Question is a reaction, not Q&A. A reaction request is:

```json
{"protocol":"qp.audience","version":"1-draft.1","type":"reaction","request_id":"42","kind":"applause"}
```

Request IDs are nonempty ASCII letters/digits/hyphens/underscores, at most 64
bytes. Requests must use the exact draft version, the known fields, type and
kind. Duplicate decoded JSON keys, invalid Unicode and malformed messages close
the connection with 1002; bad authentication closes with 1008. Unsupported
questions and poll votes
are not silently accepted. New capabilities can be advertised when implemented.

The server responds with the same request ID:

```json
{"protocol":"qp.audience","version":"1-draft.1","type":"result","request_id":"42","status":"accepted"}
```

The local endpoint additionally emits `busy` for resource/rate pressure and
`stopped` for a stop racing with input. It never emits the old `rate_limited`
wire value. Acceptance means admission into the bounded session queue, not guaranteed screen display. Events
are transient and are never replayed or retried automatically. The local engine
is the receiver, so its successful bounded queue admission directly confirms
acceptance; there is no separate network presenter receipt hop. For 30 seconds,
identical duplicate requests on the same connection return the cached result
without another queue/log entry or budget charge. Conflicting content under the
same ID closes with 1002. At most 256 cached request records are retained per
connection (overflow closes with 1013); a new connection starts a fresh cache.
Cache expiry is not a promise of durable exactly-once delivery. The browser allows
one outstanding request and disconnects after four seconds without a response.

Reactions are always accepted while Audience Live is running, subject to the
resource budgets below. There is no reaction-specific presenter setting or
settings broadcast. Audience Live OFF stops the listener, invalidates the
session, and clears all pending events and presenter display state.
Each handle owns its queue, participant IDs, and sequence; a restarted session
cannot consume events from the previous handle. Participant IDs and timestamps
are assigned by the server and do not identify people across sessions.

Admission uses a per-connection token budget (burst 5, refill 2/s) and a shared
session budget (burst 120, refill 120/s). The shared budget prevents reconnects
from bypassing all input bounds. The queue holds 128 events and the UI drains at most 16 per 100 ms poll.
Reactions expire after two seconds; accepted comments remain in the delivery queue
until drained so overlay expiry never removes presenter history. State/stop notifications
remain independent. There is no limit based on audience size in the product.

The transport-specific code owns sockets and JSON responses. `audience_events`
contains the common event model and admission engine without Axum, Tokio, Slint,
or PDF dependencies. `audience_ui` converts batches to presenter display state.
The same internal events can later be produced by a relay adapter without
changing the PDF renderer or presentation-control protocol.

## Live comments

```json
{"protocol":"qp.audience","version":"1-draft.1","type":"comment","request_id":"43","body":"Hello everyone!","handle":"Anonymous"}
```

Comments use the same authentication, request ID rules, acknowledgements, and
4 KiB message bound. Both strings are required. Initial limits are 200 Unicode
scalar values / 800 UTF-8 bytes for the body and 32 scalars / 128 bytes for the
handle, checked before normalization. Whitespace-only bodies are rejected.
C0/C1 control characters are rejected except CR, LF and TAB; whitespace sequences
are normalized to a single space for display and body logging. The submitted
handle is preserved in the log. Neither field is interpreted as HTML or linked.
Empty/whitespace handles and trimmed ASCII-case-insensitive `anonymous` are
omitted from overlay text; other handles display as `handle: body`.

Comments additionally have a per-connection token budget (burst 2, refill 1/s)
and a session comment budget (burst 30, refill 30/s), while sharing the existing
all-event budget. These are initial policy values. Acceptance is bounded by the
128-event delivery queue and 128-record log queue. A full log queue returns
`busy`; a disk failure allows reception to continue with a presenter warning.

Pause/Resume increments an admission generation under the session engine lock.
Each accepted event captures that generation. UI delivery always contributes to
the latest-20 history, but only current-generation comments received while
unpaused may enter the overlay. Session sequence IDs suppress duplicate internal
event delivery. New sessions own a fresh engine and never replay old events.

Comments carry no PDF path, page, slide identifier, media time, lane, or geometry.
See [A7 implementation](AUDIENCE_COMMENTS.md) for display and logging policies.

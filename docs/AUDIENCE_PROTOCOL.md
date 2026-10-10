# Audience Protocol v1 (Local)

Audience Live development targets v3.0.0. The protocol carries events and session
metadata only; it exposes neither PDFs nor presentation-control commands. This
is the local audience connection contract, not a complete remote relay contract.

The join page obtains its secret from the URL fragment, removes the fragment,
and opens `/ws` on the same authority. The first text message is
`{"v":1,"token":"<secret>"}`. The existing authentication deadline, Host/Origin
validation, heartbeat, and 4 KiB frame/message bounds apply.

After authentication the server sends:

```json
{"v":1,"type":"welcome","capabilities":["reactions"],"reactions_enabled":true}
```

The browser enables only implemented capabilities. Current reaction kinds are
`thumbs_up`, `heart`, `applause`, `laugh`, and `question`. Question is a reaction,
not a Q&A submission. A reaction request is:

```json
{"v":1,"type":"reaction","request_id":"42","kind":"applause"}
```

Request IDs are nonempty ASCII letters/digits/hyphens/underscores, at most 64
bytes. Requests must use version 1, the known fields, type, and kind. Invalid
messages close the connection. Unsupported comments, questions, and poll votes
are not silently accepted. New capabilities can be advertised when implemented.

The server responds with the same request ID:

```json
{"v":1,"type":"result","request_id":"42","status":"accepted"}
```

Other statuses are `disabled`, `rate_limited`, and `busy`. Acceptance means
admission into the bounded session queue, not guaranteed screen display. Events
are transient and are never replayed or retried automatically. The browser allows
one outstanding request and disconnects after four seconds without a response.

Presenter changes are broadcast as
`{"v":1,"type":"settings","reactions_enabled":false}`. Reactions OFF clears
queued events immediately. Stop invalidates the session and all pending events.
Each handle owns its queue, participant IDs, and sequence; a restarted session
cannot consume events from the previous handle. Participant IDs and timestamps
are assigned by the server and do not identify people across sessions.

Admission uses a per-connection token budget (burst 5, refill 2/s) and a shared
session budget (burst 120, refill 120/s). The shared budget prevents reconnects
from bypassing all input bounds. The queue holds 128 events, expires after two
seconds, and the UI drains at most 16 per 100 ms poll. State/stop notifications
remain independent. There is no limit based on audience size in the product.

The transport-specific code owns sockets and JSON responses. `audience_events`
contains the common event model and admission engine without Axum, Tokio, Slint,
or PDF dependencies. `audience_ui` converts batches to presenter display state.
The same internal events can later be produced by a relay adapter without
changing the PDF renderer or presentation-control protocol.

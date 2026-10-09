# Presentation Control Protocol v1 (experimental)

Quick Presenter exposes a local automation interface on Windows, macOS, and
Linux in development builds and the supported packages described in [CLI](CLI.md). `qp` is its first client. The wire types live in
`src/control/protocol.rs` and contain no Slint types. This is an initial
implementation, not a declaration that the complete automation feature is
ready for release.

## Distribution scope for v2.0.0

On macOS, this protocol is supported by the Developer ID / DMG distribution.
Mac App Store CLI/control support is explicitly out of scope for v2.0.0: the
Store GUI does not provide a supported endpoint for `qp` or other controllers.
This exclusion also covers CLI-triggered GUI startup and PDF opening. Future
sandbox-compatible support is tracked in [#422](https://github.com/koichiro/quick-presenter/issues/422); it is not a v2.0.0
release requirement. Windows and Linux retain the distributions documented in
[CLI installation](CLI.md#installation).

## Ownership and architecture

```text
Slint callbacks --------> shared presentation/session operations
                                      ^
qp -> local IPC -> bounded requests -> GUI event loop
                                      |
                          existing render scheduler
                                      |
                        isolated renderer -> PDFium
```

`AppState` remains the state owner. The control server never keeps its own deck,
cursor, timer, or notes. It delivers typed requests and response channels to the
existing event loop. Queries are snapshots of that state. Timer access is
read-only: the sole timer method retrieves elapsed time. Clients can also
read the timer in state snapshots. Timer lifecycle events are not exposed.
Navigation and black screen commands share the GUI's command path; file opens share its asynchronous
open pipeline. No keyboard, mouse, accessibility, focus, or monitor simulation
is involved. `qp` does not load PDFium or initialize Slint.

## Transport and endpoint ownership

The endpoint is `$XDG_RUNTIME_DIR/quick-presenter/control.sock` when that variable
is set. The runtime directory must be absolute, owned by the current effective
user, and inaccessible to other users. If it is unset, the fallback on macOS
and Linux is `/tmp/quick-presenter-<effective-uid>/control.sock`. Both processes
must use the same runtime-directory environment.

The endpoint directory is mode 0700 and the socket is mode 0600. A private,
non-symlink lock file prevents two instances from taking over the same endpoint.
After acquiring the lock, the server can recover a stale, same-user socket,
but never removes a regular file, a symlink, or a live endpoint. Shutdown
removes only the socket inode it created; the lock file may remain.

On Windows, the endpoint is the byte-mode Named Pipe
`\\.\pipe\quick-presenter`. The first pipe instance atomically owns the name,
remote clients are rejected, and the Windows default pipe DACL limits write
access to the creator account, administrators, and LocalSystem. Other local
users cannot send control requests. No filesystem cleanup or stale-pipe recovery
is needed because Windows removes an instance when its last handle is closed.

There is one controllable application instance per user/runtime directory.
Additional GUI instances still work, but do not acquire that control endpoint.
Control initialization failure is logged and does not prevent GUI playback.
There is no TCP listener.

Each connection carries one request and one logical response, except an accepted
`presentation.watch` subscription, which continues with event/heartbeat responses. Frames are a four-byte,
big-endian, nonzero JSON byte length followed by exactly that many UTF-8 bytes.
The maximum JSON frame is 1 MiB in each direction. Large full-text responses
use bounded transfer frames as described below. Invalid framing closes the
connection. JSON/envelope errors receive typed errors where framing permits.

There are at most eight active clients and eight queued requests. Request
frames have a one-second total read deadline; writes have a one-second timeout.
An accepted request has a 30-second execution/response deadline. Expired queued
requests are not executed. Slow clients cannot grow unbounded queues or mutate
state outside the owner thread. A timeout or disconnected client after execution
begins does **not** prove that a mutation did not occur: query status before
retrying, especially for `next` and `previous`. There is no automatic replay.

## Envelopes and versioning

Application and control protocol versions are independent. Every envelope
contains `protocol_version: 1`. Requests also contain a numeric `id`, a method,
and typed `params`. Responses echo the ID and contain exactly a result or error.
Malformed envelopes may receive `id: null`.

```json
{"protocol_version":1,"id":42,"method":"presentation.goto","params":{"page":5}}
```

Single-response queries use `result.kind` equal to `status`, `timer_elapsed`,
`notes`, `slide`, or `context`; presentation commands use `mutation`. Watch
and large-text transport replies are described below. The following example
acknowledges slide navigation and includes a read-only timer snapshot:

```json
{"protocol_version":1,"id":42,"result":{"kind":"mutation","changed":true,"state":{"session_id":"opaque-instance-id","document_revision":1,"document":"/slides/demo.pdf","page":5,"pages":12,"fullscreen":false,"blackout":false,"timer":{"running":true,"elapsed_seconds":20},"opening":false,"render_state":"rendering","notes_state":"ready"}}}
```

```json
{"protocol_version":1,"id":42,"error":{"code":"INVALID_PAGE","message":"Page is outside the presentation."}}
```

Unknown methods, invalid parameters, and unsupported versions fail explicitly.
An incompatible change requires a protocol revision rather than relying on the
application version. The initial v1 contract remains subject to review before
release; clients should tolerate additive result fields.

## Methods

| Method | Params | Result |
| --- | --- | --- |
| `presentation.timer.elapsed` | `{}` | `kind: timer_elapsed`, session ID, revision, running, elapsed seconds (read only) |
| `presentation.watch` | `{}` | `kind: watching`, initial state and sequence, then event responses |
| `presentation.status` | `{}` | `kind: status` plus state fields |
| `presentation.next` | `{}` | Mutation with resulting state |
| `presentation.previous` | `{}` | Mutation with resulting state |
| `presentation.goto` | `{"page":5}` | Mutation with resulting state |
| `presentation.open` | `{"file":"/absolute/deck.pdf"}` | Mutation after open commits |
| `presentation.close` | `{}` | Mutation with empty presentation state |
| `presentation.blackout` | `{"value":true}` | Mutation with resulting state |
| `presentation.notes` | `{}` | `kind: notes`, session ID, revision, page, notes |
| `presentation.slide` | `{}` or `{"full":true}` | `kind: slide`, session ID, revision, pages, page text |
| `presentation.context` | `{}` or `{"full":true}` | `kind: context`, presentation state, current and next page text/notes |

Pages are one-based. `goto` outside `1..=pages` fails without changing state.
`next` at the last page and `previous` at the first page succeed with
`changed: false`. Blackout is a state setter and repeated identical values
succeed with `changed: false`. These presentation commands retain the GUI's
existing automatic timer behavior: leaving page one starts timing and returning
to page one resets it. These are navigation side effects, not independently
addressable timer controls. Timer queries and subscriptions never trigger them.

`open` uses an absolute UTF-8 path (at most 32 KiB, no NUL). The CLI resolves a
relative path using its own working directory. Success means the existing helper
open/initial preparation and session commit succeeded; it does not wait for
all notes, previews, or display updates. A failed replacement open retains the
previous deck when available. Competing opens/file dialogs are rejected with
`BUSY`; a GUI open can supersede an outstanding control open. `close` cancels
pending opens/reloads, invalidates late renderer events, requests helper shutdown,
and clears PDF, notes, cache, and blackout. The existing document-close
lifecycle also resets the GUI-owned timer; `close` is not a standalone timer
reset operation. It leaves windows and the application running. Repeated close
succeeds with no state change. A subsequent open may return `BUSY` briefly while helper shutdown finishes.

## State and notes

| Field | Meaning |
| --- | --- |
| `session_id` | Opaque runtime instance ID, including when no PDF is open |
| `document_revision` | Increases on committed open/reload or nonempty close |
| `document` | Active path or null; GUI-opened non-UTF-8 paths use a lossy display string |
| `page`, `pages` | Logical current page and count; null and zero when closed |
| `fullscreen` | Existing slide-window fullscreen state |
| `blackout` | Existing audience black-screen state |
| `timer` | Read-only snapshot of the existing GUI-owned timer: running flag and whole elapsed seconds |
| `opening` | A PDF open is pending; the active deck may still be available |
| `render_state` | `empty`, `rendering`, `ready`, or `failed` for the logical current page |
| `notes_state` | `empty`, `loading`, `ready`, or `failed` |

Navigation success acknowledges a logical state change, not a compositor frame.
`render_state: ready` means the current slide's requested image is in the cache;
it is not an assertion about OS scanout, focus, or visibility. During rendering
or failure, the audience window can retain its last good image.

Notes reuse the existing annotation extraction and `SpeakerNotes` model. The
initial implementation waits for whole-document background note extraction.
`NOTES_LOADING` and `NOTES_FAILED` distinguish unavailable data from an empty
note. A ready page with no supported notes returns `notes: ""`. No second parser,
AI-generated content, or OCR is involved.

## Slide text and presentation context

`presentation.slide` returns the logical current page's PDF source text:

```json
{"protocol_version":1,"id":42,"result":{"kind":"slide","session_id":"opaque-instance-id","document_revision":1,"pages":24,"page":7,"text":["Architecture","Rust","Slint","PDFium"],"truncated":false}}
```

`presentation.context` returns one consistent presentation-state snapshot plus
current and next page content. Each content object contains `page`, `text`,
`truncated`, and `notes`. `next` is null on the last page. Its `presentation`
object uses the same fields as `status`, including the instance ID, document
revision, read-only timer snapshot, blackout, and render/notes readiness.
Reading context has no effect on timing. The CLI adds
`protocol_version: 1` to the flattened result as usual.

```json
{"protocol_version":1,"kind":"context","presentation":{"session_id":"opaque-instance-id","document_revision":1,"document":"/slides/demo.pdf","page":7,"pages":24,"fullscreen":false,"blackout":false,"timer":{"running":true,"elapsed_seconds":420},"opening":false,"render_state":"ready","notes_state":"ready"},"current":{"page":7,"text":["Architecture"],"truncated":false,"notes":"Explain process isolation."},"next":{"page":8,"text":["Security"],"truncated":false,"notes":"Explain PDF sandboxing."}}
```

Text extraction runs on demand inside the existing isolated PDFium helper,
through the bounded render scheduler at background priority. It never opens
PDFium in the UI or CLI. Only queried pages are extracted; at most 32 compact page
results and two full page results are cached separately. Committed open/reload and close clear that source cache,
and results from old renderer sessions are ignored. Existing note extraction
and its `SpeakerNotes` model supply both pages' notes.

Text follows PDFium's document order, split at line boundaries. Line endings
are normalized and a final line terminator is omitted. Empty text is `[]`,
including image-only slides; it does not imply extraction failure. PDF source
order can differ from visual reading order. No OCR, layout reconstruction,
AI summaries, or narration are generated.

Each page's text is bounded to a UTF-8-safe 4 KiB prefix, with `truncated: true`
when shortened. This leaves room for both pages' existing speaker notes and
worst-case JSON escaping within the 1 MiB control frame for ordinary queries.
With `params: {"full":true}`, both methods return unabridged source text and
`truncated: false`. Full text never silently falls back to a compact prefix.
Each full page has a 4 MiB UTF-8 safety limit; exceeding it returns `TEXT_FAILED`.
Pages exceeding one
million PDFium text characters are rejected before bulk string allocation.
Native operations retain the helper's five-second watchdog. Extraction failure
returns `TEXT_FAILED` (cached until the next committed open/reload). If the
renderer worker has stopped, missing source text also fails immediately with
`TEXT_FAILED`; already-cached source remains readable.

Queries wait asynchronously for missing text under the control request's
30-second deadline. The UI continues to handle rendering and navigation.
`slide` does not require notes to be ready. `context` returns `NOTES_LOADING`
or `NOTES_FAILED` instead of presenting unavailable notes as empty strings.
Retry after `status.notes_state` becomes `ready`. A ready page without speaker
notes returns `notes: ""`.

If the logical page or document revision changes while a text query is pending,
the query returns `CANCELLED`; request it again. A timeout yields `TIMEOUT`.
Successful context fields are assembled together from the owner state, so they
never mix documents or current/next page numbers. As with status, this describes
the logical presentation, not a compositor-frame acknowledgement.

### Full-text response transfers

The `full` parameter defaults to false, so existing `{}` requests keep the
single-frame compact contract. A full response that fits in 1 MiB also uses
one ordinary response frame. Larger responses are serialized once from the
typed, consistent owner-state result and split into transport-only replies:

```json
{"protocol_version":1,"id":42,"result":{"kind":"transfer","chunk":{"sequence":0,"total_bytes":1500000,"data":[123,34,112,114]}}}
```

The example omits most payload bytes. `data` contains bytes of the original
serialized response, not application state or an independently interpreted JSON
object. Sequences start at zero and increase by one. Each chunk carries at most
64 KiB of data, the same ID/version, and the same `total_bytes`. Every outer
frame is independently valid JSON and remains under the 1 MiB frame limit.
UTF-8 code points can cross byte-chunk boundaries; decode only after reassembly.

The aggregate response is bounded to 32 MiB. Clients must reject invalid totals,
empty/oversized chunks, wrong IDs/versions, duplicate or missing sequences, and
incomplete transfers. After collecting exactly `total_bytes`, decode and validate
the original typed response. Transfer frames are accepted only for requests
with `full: true`. `qp` implements reassembly in its protocol client and prints
only the final single JSON object; no fragments or partial output reach stdout.

The existing 30-second server response deadline also bounds the whole transfer.
Private renderer IPC v4 uses corresponding typed, correlated transfer envelopes
for large helper text results. It does not raise the per-frame control limit or
change pixel framing. The GUI still never extracts PDF text itself.

## Read-only timer query

The only timer method is `presentation.timer.elapsed`, with empty parameters.
It returns the GUI-owned timer's running flag and whole elapsed seconds, requires
an open PDF, and has no state-changing side effects. The corresponding CLI
command is `qp timer elapsed`, optionally with `--json`.

```json
{"protocol_version":1,"id":42,"method":"presentation.timer.elapsed","params":{}}
```

```json
{"protocol_version":1,"id":42,"result":{"kind":"timer_elapsed","session_id":"instance","document_revision":1,"running":true,"elapsed_seconds":183}}
```

Timer access is limited to the elapsed-time query. Its empty parameters reject
state-changing fields with `INVALID_REQUEST`.

`status`, `context`, and `watch` also expose timer observations only. The GUI's
existing automatic navigation/open/close timer rules are unchanged; no query
or subscription invokes those transitions. The event stream contains no timer
lifecycle notifications; use the elapsed-time query for current timing.

## Event subscriptions

Send `presentation.watch` with empty parameters. The initial response is:

```json
{"protocol_version":1,"id":42,"result":{"kind":"watching","state":{"session_id":"instance","document_revision":1,"document":"/slides.pdf","page":1,"pages":3,"fullscreen":false,"blackout":false,"timer":{"running":false,"elapsed_seconds":0},"opening":false,"render_state":"ready","notes_state":"ready"},"sequence":12}}
```

Registration and initial state capture happen together on the presentation
owner's event loop. The client emits that initial state as
`presentation.snapshot`. Changes after registration continue on the same
length-prefixed connection:

```json
{"protocol_version":1,"id":42,"result":{"kind":"event","envelope":{"protocol_version":1,"session_id":"instance","document_revision":1,"sequence":13,"event":"page.changed","page":2,"pages":3}}}
```

`qp watch --json` prints the inner envelope, one valid JSON object per line,
with no request IDs, wrappers, logs, or transport messages.

| Event | Fields beyond version/session/revision/sequence |
| --- | --- |
| `presentation.snapshot` | `state`: initial status (synthesized by the client) |
| `presentation.opened` | `state`: committed PDF state, including same-file reopen |
| `presentation.reloaded` | `state`: committed hot-reload state |
| `presentation.closed` | No additional fields |
| `page.changed` | `page`, `pages` |
| `blackout.changed` | `value` |

Events are emitted from committed domain transitions shared by GUI and control
commands. No-op boundary navigation, repeated blackout settings, repeated close,
and queries emit no changes. Sequence increases across all event types for the
controllable instance; the initial snapshot gives the baseline. Every later event
must have the next sequence and the same instance session ID. Document revisions
change on open/reload/close. There is no replay; reconnect for a new snapshot.
An open/close/reload event carries the resulting document state or revision;
associated blackout changes follow it. Subscribing has no effect on timing.
The stream contains no timer lifecycle events, timer ticks, or render/notes
readiness acknowledgements.

The initial handshake retains the ordinary 30-second request deadline. An
accepted watch has no total lifetime deadline and survives document close/open.
The server sends `kind: heartbeat` replies approximately every two seconds while
healthy, using the same response ID/version. Clients ignore heartbeats and renew
transport read deadlines. Each server frame write has a one-second deadline;
watchers cannot block presentation transitions or server shutdown.

Up to four watches share the existing eight-connection server. Each watch has a
bounded 64-event queue. Queue overflow removes that subscriber and reports
`EVENTS_LAGGED` / CLI exit 11 when the transport remains writable. Blocked/broken
transports can instead fail with timeout/IPC errors. Terminal errors use the
ordinary typed error envelope; complete event lines already printed remain on
stdout, and the CLI prints errors only on stderr. Reconnect after any stream
failure to obtain a fresh baseline. Idle disconnects release their slots after
a heartbeat detects closure. No automatic reconnect or remote transport is added.

## Errors and current limits

Errors use stable codes; human messages are diagnostic text, not API keys.
The CLI mapping is documented in [CLI](CLI.md). Common protocol codes are
`NO_PRESENTATION`, `INVALID_PAGE`, `INVALID_REQUEST`, `UNKNOWN_METHOD`,
`PROTOCOL_MISMATCH`, `BUSY`, `TIMEOUT`, `OPEN_FAILED`, `NOTES_LOADING`,
`NOTES_FAILED`, `TEXT_FAILED`, `CANCELLED`, and `EVENTS_LAGGED`.

This interface has no explicit timer mutations, network control, session
discovery, or event/command replay. Renderer IPC
is a separate private protocol and must not be exposed as the presentation
control protocol.

## Compatibility validation

`tests/fixtures/control-v1.json` records historical request, response, event,
and error examples. `tests/control_contract.rs` checks their typed decoding,
required JSON fields, NDJSON formatting, and exit-code mapping. Open-request
paths follow the host OS's absolute-path syntax. These fixtures complement
command and IPC behavior tests; they are not a replacement for them.

Within protocol v1, preserve existing method names, required fields, field types,
numbering, and error semantics. Clients should ignore additional object fields.
New event variants require explicit compatibility design because existing typed
clients reject unknown event names. Breaking changes require a new protocol
version rather than following the application release number.

`qp --version --json` is local CLI metadata, not an IPC request. Its
`application_version` identifies the CLI build and `protocol_version` identifies
the protocol it supports. It is available even when Quick Presenter is closed.


## CLI-triggered GUI startup

When `qp open` cannot connect because no server is running, its CLI-only startup
adapter launches the companion GUI and probes `presentation.status` until ready.
It then sends the ordinary `presentation.open` request once. Concurrent CLI
startup is serialized with an OS lock. This does not add protocol methods or
move file loading into the CLI. Other commands do not start the GUI. See
[CLI startup](CLI.md#starting-a-presentation-from-the-cli) for deadlines and errors.

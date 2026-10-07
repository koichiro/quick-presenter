# Presentation Control Protocol v1 (experimental)

Development builds expose a local automation interface on macOS and Linux.
`qp` is its first client. The wire types live in `src/control/protocol.rs` and
contain no Slint types. This is an initial implementation, not a declaration
that the complete automation feature is ready for release.

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
existing event loop. Queries are snapshots of that state. Navigation and black
screen commands share the GUI's command path; file opens share its asynchronous
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

There is one controllable application instance per user/runtime directory.
Additional GUI instances still work, but do not acquire that control endpoint.
Control initialization failure is logged and does not prevent GUI playback.
There is no TCP listener. Windows currently reports `UNSUPPORTED_PLATFORM`;
a Windows GUI build retains its existing behavior.

Each connection carries one request and one response. Frames are a four-byte,
big-endian, nonzero JSON byte length followed by exactly that many UTF-8 bytes.
The maximum JSON frame is 1 MiB in each direction. Invalid framing closes the
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

A success has `result.kind` equal to `status`, `mutation`, or `notes`:

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
| `presentation.status` | `{}` | `kind: status` plus state fields |
| `presentation.next` | `{}` | Mutation with resulting state |
| `presentation.previous` | `{}` | Mutation with resulting state |
| `presentation.goto` | `{"page":5}` | Mutation with resulting state |
| `presentation.open` | `{"file":"/absolute/deck.pdf"}` | Mutation after open commits |
| `presentation.close` | `{}` | Mutation with empty presentation state |
| `presentation.blackout` | `{"value":true}` | Mutation with resulting state |
| `presentation.notes` | `{}` | `kind: notes`, session ID, revision, page, notes |

Pages are one-based. `goto` outside `1..=pages` fails without changing state.
`next` at the last page and `previous` at the first page succeed with
`changed: false`. Blackout is a state setter and repeated identical values
succeed with `changed: false`. Navigation preserves the GUI's automatic timer
behavior, including reset when returning to page one.

`open` uses an absolute UTF-8 path (at most 32 KiB, no NUL). The CLI resolves a
relative path using its own working directory. Success means the existing helper
open/initial preparation and session commit succeeded; it does not wait for
all notes, previews, or display updates. A failed replacement open retains the
previous deck when available. Competing opens/file dialogs are rejected with
`BUSY`; a GUI open can supersede an outstanding control open. `close` cancels
pending opens/reloads, invalidates late renderer events, requests helper shutdown,
and clears PDF, notes, cache, timer, and blackout. It leaves windows and the
application running. Repeated close succeeds with no state change. A subsequent
open may return `BUSY` briefly while helper shutdown finishes.

## State and notes

| Field | Meaning |
| --- | --- |
| `session_id` | Opaque runtime instance ID, including when no PDF is open |
| `document_revision` | Increases on committed open/reload or nonempty close |
| `document` | Active path or null; GUI-opened non-UTF-8 paths use a lossy display string |
| `page`, `pages` | Logical current page and count; null and zero when closed |
| `fullscreen` | Existing slide-window fullscreen state |
| `blackout` | Existing audience black-screen state |
| `timer` | Running flag and whole elapsed seconds from the existing timer |
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

## Errors and current limits

Errors use stable codes; human messages are diagnostic text, not API keys.
The CLI mapping is documented in [CLI](CLI.md). Common protocol codes are
`NO_PRESENTATION`, `INVALID_PAGE`, `INVALID_REQUEST`, `UNKNOWN_METHOD`,
`PROTOCOL_MISMATCH`, `BUSY`, `TIMEOUT`, `OPEN_FAILED`, `NOTES_LOADING`,
`NOTES_FAILED`, and `CANCELLED`.

This initial interface has no event stream, text/context query, explicit timer
controls, Windows Named Pipe, network control, session discovery, or command
replay. Renderer IPC is a separate private protocol and must not be exposed as
the presentation control protocol.

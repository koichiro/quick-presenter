# Renderer IPC Protocol v2

`src/renderer_protocol.rs` defines the transport and broker adapter from #371;
`src/renderer_helper.rs` connects it to production subprocesses in #372. Version
2 adds page-scoped notes to retain priority scheduling between pages. Deadlines,
resource budgets, and OS sandboxing remain #373 through #376.

## Framing and compatibility

Each direction is an ordered byte stream over an anonymous pipe. The helper's
standard error is disconnected by the broker; stdout carries protocol only.
`Framed<T>` uses the standard `Read` and `Write` traits, permitting
in-memory and fault-injection tests without launching PDFium or Slint windows.

Every frame contains, in order:

| Field | Encoding |
| --- | --- |
| Magic | 4 bytes: `QPRP` |
| Protocol version | little-endian u16: `2` |
| Control length | little-endian u32, nonzero |
| Pixel payload length | little-endian u32 |
| Control message | UTF-8 JSON, exactly control length bytes |
| Pixel payload | tightly packed RGBA8 bytes, exactly pixel length bytes |

JSON is used only for bounded control information. Pixel bytes are transferred
directly without base64, image compression, or JSON array expansion. Unknown
message kinds, fields, enum values, duplicate JSON fields, malformed JSON,
trailing data, and unsupported versions are errors. Control encoding is limited
while serializing, before writing any frame bytes.

The broker sends `Hello` and waits for `Ready` before sending document work. Both
use session ID zero. Both peers check the version in every frame header. The
parent and helper are shipped together; mismatch is fatal, with no downgrade or
in-process fallback. Version changes require an explicit protocol revision.

EOF before any header byte is clean stream closure (`Ok(None)`); EOF after the
first byte of a frame is a truncation error. Whether clean closure is expected
depends on the process lifecycle: EOF during an exchange is a helper failure,
while parent-input EOF terminates the helper. After any framing, validation, or IO
error, discard the connection; do not attempt to resynchronize or reuse it.
Short reads/writes and interrupted reads are handled by the codec.

## Control messages

Every envelope contains nonzero `request_id`, `session_id`, and a `message`
object tagged with `kind`. Responses echo the request and session IDs. IDs are
monotonic within one broker adapter and never wrap. Document sessions are
nonzero; `Hello`, `Ready`, `Shutdown`, and `Stopped` use session zero.

| Request | Success response | Data |
| --- | --- | --- |
| Hello | Ready | Version agreement via the frame header |
| Open | Opened | Native path; response title and page count |
| Render | Pixels | Zero-based page index and target width; RGBA8 dimensions |
| Notes | NotesLoaded | Sorted, unique one-based page numbers and note text |
| NotesPage | NotesLoaded | Zero-based requested page; only that page's notes |
| Close | Closed | Release the active document |
| Shutdown | Stopped | Stop the helper |

Document operations may return `Failed` with a bounded message and one of
`InvalidPdf`, `UnsupportedPdf`, `LimitExceeded`, `RenderFailed`, `NotesFailed`,
or `Internal`. Handshake and lifecycle failures are connection/process failures,
not document `Failed` responses. Non-pixel messages require zero payload bytes.

Paths preserve native filenames: `Unix` contains raw OS bytes, and `Windows`
contains little-endian UTF-16 bytes, including unpaired surrogates. NUL, empty,
overlong, odd-length Windows paths, and decoding for a different OS are rejected.
These are paths authorized by the broker, not arbitrary helper filesystem
requests. Brokered handles/data will be reviewed when sandboxing is added.

## Limits and validation order

| Limit | Value |
| --- | --- |
| Control JSON | 1 MiB |
| Pixel payload | 64 MiB |
| Width or height | 1 through 16,384 |
| Page count | 1 through 100,000 |
| Encoded path | 32 KiB |
| Title or error text | 4 KiB UTF-8 |
| Note per page | 64 KiB UTF-8 |
| Notes for a document | 512 KiB UTF-8 |
| Pending requests per helper | 64, plus one reserved shutdown request |

The reader validates header lengths before allocating the control buffer. JSON
decoding is bounded by that buffer and serde's recursion limit; decoded field
limits and semantic metadata are checked immediately after parsing. String
limits measure decoded UTF-8 bytes; escaped JSON must also fit the control cap.
The codec verifies RGBA8 dimensions and checked `width * height * 4` against the
declared payload before allocating pixel storage. These are wire safety limits;
#373 adds measured product budgets, operation deadlines, and process memory caps.

The broker must use `Broker::read_response()` to correlate request ID, session,
response kind, requested width, and note page range before allocating or reading
the pixel payload. `Broker::accept()` repeats validation and removes a pending
request only after complete acceptance. Unknown, duplicate, stale, wrong-kind,
and malformed responses cannot mutate broker state or produce render events.
The adapter does not own or mutate the active presentation session.

## Scheduler adapter

One `Broker` belongs to one helper/document session. `command()` maps `Open`,
`RenderPage`, `ExtractSpeakerNotes`, and `Shutdown` to protocol requests. Render
purpose, priority, and job identity remain broker-local: priority is resolved by
the scheduler before dispatch, and the original request/job is retained for the
matching `PageRendered` or `PageFailed` event. `close()` is an internal lifecycle
operation, not a new user-facing scheduler close action. Production uses
`notes_page()` instead of whole-document `Notes`, retaining the existing batched
background extraction. The broker rejects notes for any other page.

Close requires all document work to finish first. Shutdown has a reserved slot
even when document requests fill the backlog; no new work is accepted while a
close or shutdown acknowledgment is pending.

`Opened`, pixel, notes, and document failures map to existing `RenderEvent`
values. Notes failures produce empty notes and bounded warning status. Pixel
dimensions determine aspect ratio and byte accounting on the broker; those
values are not trusted helper fields. Conversion constructs a pixel buffer only
after validation; Slint images are still constructed by the existing UI path.

Reload prepare/commit/discard are broker transactions across active and
candidate helpers rather than wire commands sharing a PDFium process. The
adapter explicitly rejects those commands. The scheduler prepares a candidate
through its own broker, validates its initial render, then commits or discards it
through the existing session controller.

Tests cover every message variant, native paths, concatenated frames, all
truncated prefixes, short IO, header limits, unknown metadata, field limits,
dimensions, notes, broker correlation, and deterministic byte mutations.
Executable-level tests additionally cover real PDF open/render/notes, native
abort, EOF, version mismatch, candidate isolation, and parent-pipe cleanup.

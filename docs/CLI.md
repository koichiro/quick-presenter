# qp CLI (experimental)

`qp` opens and controls Quick Presenter through its local Presentation
Control Protocol. It is intended for automation tools and AI agents, and does
not implement narration, AI models, or presentation business logic.

## Installation

Release packages include the CLI and its documentation:

| Distribution | CLI location |
| --- | --- |
| Mac App Store | Presentation control is not supported in v2.0.0; use the Developer ID / DMG distribution |
| macOS Developer ID app/DMG | `/Applications/Quick Presenter.app/Contents/MacOS/qp` |
| Windows Store/MSIX | `qp.exe` app execution alias (enable it in Windows Settings if disabled) |
| Windows MSI validation package | `qp.exe` beside `quick-presenter.exe` in the installation directory |
| Linux Debian package | `/usr/bin/qp` |
| Raw binary artifact | `qp` or `qp.exe` beside the GUI executable |

On macOS, invoke the bundled executable directly, or add a user-local link:

```sh
"/Applications/Quick Presenter.app/Contents/MacOS/qp" status --json
mkdir -p "$HOME/.local/bin"
ln -s "/Applications/Quick Presenter.app/Contents/MacOS/qp" "$HOME/.local/bin/qp"
# Add $HOME/.local/bin to PATH in your shell configuration.
```

The MSI does not modify PATH. Invoke its installed `qp.exe` by full path, or
add its directory to your user PATH. Windows Store remains the supported Windows
distribution; MSI and direct MSIX packages are validation artifacts.
Mac App Store CLI/control support is explicitly out of scope for v2.0.0.
On macOS, use the Developer ID / DMG distribution for all presentation control,
including `qp open` GUI startup, state/content queries, navigation, and `watch`.
The Store distribution does not provide a supported Control Protocol endpoint;
using a separate CLI executable does not make the Store GUI controllable.
Future sandbox-compatible support is tracked in [#422](https://github.com/koichiro/quick-presenter/issues/422).

For development, build both binaries from the same checkout:

```sh
cargo build --bins
./target/debug/quick-presenter slides.pdf
```

In another terminal:

```sh
./target/debug/qp status --json
./target/debug/qp timer elapsed --json
./target/debug/qp next --json
./target/debug/qp prev
./target/debug/qp goto 5
./target/debug/qp blackout on
./target/debug/qp blackout off
./target/debug/qp notes --json
./target/debug/qp slide --json
./target/debug/qp context --json
./target/debug/qp slide --full --json
./target/debug/qp context --full --json
./target/debug/qp open "slides with spaces.pdf" --json
./target/debug/qp close
./target/debug/qp watch --json
```

After adding the binary directory to PATH, the same commands use `qp` directly.
On macOS and Linux, the GUI and CLI must share their `XDG_RUNTIME_DIR`
environment when it is set. Windows uses the per-machine name
`\\.\pipe\quick-presenter` with a local-only, write-restricted Named Pipe ACL.
`qp open <file>` starts the companion GUI when no control server is running,
then waits for local control before sending the open request. Other commands
require a running GUI. It has its own `--help`;
it does not replace the GUI's existing startup arguments.

## Starting a presentation from the CLI

```sh
qp open "slides with spaces.pdf" --json
qp status --json
```

`open` first tries the running instance. If the endpoint is absent or refuses
connections, it locates `quick-presenter` / `quick-presenter.exe` beside the
actual CLI executable and starts it without a PDF argument. This works with
bundled macOS binaries, Debian symlinks, Windows package aliases, and development
builds. A standalone CLI copy cannot start a missing companion GUI. The child
inherits the CLI environment, including `XDG_RUNTIME_DIR`, so both use the same
endpoint. It keeps running after `qp` exits. GUI diagnostics go to the existing
application log, never JSON stdout.

Concurrent CLI startup attempts use an OS lock. After a successful status probe,
`qp` submits the open request once through the ordinary protocol. Startup polling
has a 15-second deadline; ordinary IPC request deadlines still apply. Startup
exit or a missing GUI reports `NOT_RUNNING` (exit 3), startup timeout reports
`TIMEOUT` (exit 9), and launch/permission failures report `IPC_FAILURE` (exit 7).
An error from a reachable server never triggers startup or automatic command
replay. If startup times out, the GUI may still finish starting; inspect it before
retrying. `qp close` closes the document and leaves the GUI running.

## Version information

Version queries work without a running GUI, an IPC endpoint, or PDFium:

```sh
qp --version
qp --version --json
```

JSON contains `application_version` (the CLI build's package version) and
`protocol_version` (currently `1`). The application version is independent of
the protocol version; it does not report the running GUI's version. `-V` is an
alias for `--version`. Use CLI and GUI binaries from the same package.

## Machine-readable output

`--json` is available for every implemented command. Successful stdout contains
one JSON object and a newline, with no logs or human decoration.
`watch --json` streams one event per line (NDJSON) until interrupted. Unlike the IPC
response, CLI output omits the request envelope and exposes the result fields
alongside `protocol_version`:

```json
{"protocol_version":1,"kind":"status","session_id":"opaque-instance-id","document_revision":1,"document":"/slides/demo.pdf","page":4,"pages":18,"fullscreen":false,"blackout":false,"timer":{"running":true,"elapsed_seconds":183},"opening":false,"render_state":"ready","notes_state":"ready"}
```

```sh
qp status --json | jq '.page'
qp notes --json | jq -r '.notes'
qp slide --json | jq -r '.text[]'
qp context --json | jq -r '.current.notes'
qp context --json | jq '.next'
qp next --json | jq '.state.page'
```

Single-response failures leave stdout empty. A failing watch retains already
printed complete events and reports its terminal error only on stderr. With `--json`, stderr contains one JSON error object
such as `{"code":"NO_PRESENTATION","message":"No presentation is currently open."}`.
Without `--json`, stderr contains a diagnostic. Scripts should check the exit
code or JSON error code instead of matching English messages.

## Exit codes

| Exit | Category / error codes |
| --- | --- |
| 0 | Success |
| 2 | Invalid command/request or unknown method |
| 3 | `NOT_RUNNING` |
| 4 | `NO_PRESENTATION` |
| 5 | `INVALID_PAGE` |
| 6 | `PROTOCOL_MISMATCH` |
| 7 | `IPC_FAILURE`, `UNSUPPORTED_PLATFORM` |
| 8 | `BUSY`, `NOTES_LOADING` |
| 9 | `TIMEOUT` |
| 10 | `OPEN_FAILED`, `NOTES_FAILED`, `TEXT_FAILED`, `CANCELLED` |
| 11 | `EVENTS_LAGGED` (reconnect for a fresh snapshot) |

Pages are one-based. End-of-deck navigation succeeds without advancing and
returns `changed: false`. `blackout on/off` sets an explicit state. `close`
closes the PDF rather than quitting the GUI. Queries report pending rendering
and note extraction; an empty note is different from unavailable notes.

Timeouts and disconnections can have an uncertain mutation outcome. Query
`status` before retrying `next` or `prev`. There is one controllable instance
per user/runtime directory. Explicit timer mutation is unavailable in this version.
See [Control Protocol](CONTROL_PROTOCOL.md) for transport, completion semantics,
state fields, and compatibility expectations.

## Source context for automation

`qp slide` prints the current page's PDF text. `qp context` combines current
and next page text and speaker notes with presentation state in one query.
The JSON contract is documented in [Control Protocol](CONTROL_PROTOCOL.md).
The next page is null at the end of the deck. Image-only pages have an empty
text array; there is no OCR. Text is a maximum 4 KiB UTF-8 prefix per page,
with an explicit `truncated` flag. Add `--full` to `slide` or `context` to obtain
unabridged text with `truncated: false`. Other commands reject `--full`. Both
human and JSON output support it. PDF source order may differ from reading order.

After opening a deck, notes can still be loading. `context` reports
`NOTES_LOADING` with exit 8 until notes are ready; `slide` works independently.
A page or document change during a pending query reports `CANCELLED` with exit
10. Retry the query to obtain the new page. `TEXT_FAILED` is distinct from
successful empty text, and `TIMEOUT` bounds a stalled query.

An external agent can use `qp context --json`, generate and speak narration
with its own tools, then call `qp next --json`. Check `.state.page` or `.changed`
to detect the last page. Quick Presenter supplies only source information and
presentation control; the agent owns narration and the wait for audio completion.

```sh
qp slide --full --json | jq -r '.text[]'
qp context --full --json | jq -r '.current.text[]'
qp context --full --json | jq -r '.next.text[]?'
```

Full source results use the same output schema as compact results. Large IPC
responses are reassembled internally; stdout still contains exactly one JSON
object and a newline. Full pages retain a 4 MiB safety limit and the existing
one-million-PDFium-character guard. These limits yield `TEXT_FAILED` rather than
successful shortened text. The server bounds assembled responses to 32 MiB
and preserves its 30-second response deadline. Failed or incomplete transfers
leave stdout empty and report a typed error on stderr.


## Read elapsed time

```sh
qp timer elapsed
qp timer elapsed --json | jq '.elapsed_seconds'
```

Human output is whole elapsed seconds and a newline. JSON includes
`protocol_version`, `kind: timer_elapsed`, `session_id`, `document_revision`,
`running`, and `elapsed_seconds`. No open deck yields `NO_PRESENTATION`.
`qp timer elapsed` is the only timer command and has no state-changing side
effects. Existing GUI navigation still starts the timer
when leaving page one and resets it when returning to page one; opening and
closing a PDF retain their existing reset behavior.

## Watch presentation changes

```sh
qp watch
qp watch --json
qp watch --json | jq --unbuffered 'select(.event == "page.changed") | .page'
```

Every watch begins with `presentation.snapshot`, containing current state and
its sequence baseline. Later events include `presentation.opened`,
`presentation.reloaded`, `presentation.closed`, `page.changed`,
`blackout.changed`. Timer lifecycle events are not exposed. Watching never
changes the presentation or timer.
All lines include `protocol_version`, `session_id`, `document_revision`, and
`sequence`; page numbers remain one-based. Each line is flushed immediately.
Transport heartbeats stay invisible to stdout.

A watch remains connected across document close/open and idle periods. Stop
with Ctrl-C. Up to four simultaneous watchers are allowed, reserving connection
capacity for commands. Each has a 64-event queue. A slow consumer is disconnected
with `EVENTS_LAGGED` / exit 11 when a typed error can still be delivered; a blocked
or broken transport may instead produce `IPC_FAILURE` or `TIMEOUT`. Reconnect to
receive a fresh snapshot. Events are not replayed and the CLI does not reconnect
automatically. This stream reports committed domain changes, not render or note
readiness acknowledgements or timer ticks; query `status`, `context`, or
`timer elapsed` when those data are needed.

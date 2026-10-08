# qp CLI (experimental)

`qp` controls an already-running Quick Presenter through its local Presentation
Control Protocol. It is intended for automation tools and AI agents, and does
not implement narration, AI models, or presentation business logic.

This initial implementation supports Windows, macOS, and Linux development
builds. Build both binaries from the same checkout:

```sh
cargo build --bins
./target/debug/quick-presenter slides.pdf
```

In another terminal:

```sh
./target/debug/qp status --json
./target/debug/qp next --json
./target/debug/qp prev
./target/debug/qp goto 5
./target/debug/qp blackout on
./target/debug/qp blackout off
./target/debug/qp notes --json
./target/debug/qp open "slides with spaces.pdf" --json
./target/debug/qp close
```

After adding the binary directory to PATH, the same commands use `qp` directly.
On macOS and Linux, the GUI and CLI must share their `XDG_RUNTIME_DIR`
environment when it is set. Windows uses the per-machine name
`\\.\pipe\quick-presenter` with a local-only, write-restricted Named Pipe ACL.
The CLI does not launch the application automatically. It has its own `--help`;
it does not replace the GUI's existing startup arguments. Installation/package
integration for `qp` is not included in this initial change.

## Machine-readable output

`--json` is available for every implemented command. Successful stdout contains
one JSON object and a newline, with no logs or human decoration. Unlike the IPC
response, CLI output omits the request envelope and exposes the result fields
alongside `protocol_version`:

```json
{"protocol_version":1,"kind":"status","session_id":"opaque-instance-id","document_revision":1,"document":"/slides/demo.pdf","page":4,"pages":18,"fullscreen":false,"blackout":false,"timer":{"running":true,"elapsed_seconds":183},"opening":false,"render_state":"ready","notes_state":"ready"}
```

```sh
qp status --json | jq '.page'
qp notes --json | jq -r '.notes'
qp next --json | jq '.state.page'
```

Failures leave stdout empty. With `--json`, stderr contains one JSON error object
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
| 10 | `OPEN_FAILED`, `NOTES_FAILED`, `CANCELLED` |

Pages are one-based. End-of-deck navigation succeeds without advancing and
returns `changed: false`. `blackout on/off` sets an explicit state. `close`
closes the PDF rather than quitting the GUI. Queries report pending rendering
and note extraction; an empty note is different from unavailable notes.

Timeouts and disconnections can have an uncertain mutation outcome. Query
`status` before retrying `next` or `prev`. There is one controllable instance
per user/runtime directory. Event watching, slide/context extraction, and
explicit timer control are not implemented in this initial interface.
See [Control Protocol](CONTROL_PROTOCOL.md) for transport, completion semantics,
state fields, and compatibility expectations.

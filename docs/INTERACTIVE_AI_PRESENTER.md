# Interactive AI Presenter

Microsoft Store v2.0.0 is coming soon. Use the macOS DMG or Debian package
to try the released version now.

Use Quick Presenter's v2.0.0 interface with an external agent. The recorded demo
uses Codex, configured to execute the local CLI and read its output. Configure
that access for your chosen agent; support for every model or service is not
implied. Quick Presenter supplies PDF display, text/notes, state, and controls.
The agent interprets the material, generates answers, and chooses operations.

## Prepare matching binaries

Install Quick Presenter v2.0.0 for your OS from the
[download section](https://quickpresenter.com/#download). Use the GUI and CLI
from the same package; this workflow uses the v2.0.0 `export` interface.

For package paths and PATH setup, see the
[v2.0.0 CLI installation guide](https://github.com/koichiro/quick-presenter/blob/v2.0.0/docs/CLI.md#installation).
macOS Developer ID / DMG and nonsandboxed development builds support control;
Mac App Store does not in v2.0.0. Windows Store uses the `qp.exe` execution alias;
Debian provides `/usr/bin/qp`. Use matching binaries and share XDG_RUNTIME_DIR
when set. Commands below assume `qp` is on PATH. Use its full package path if needed.

## Read the sample and check notes

Save the checked-in [12-page sample](../website/src/assets/interactive-ai-sample.pdf)
as `interactive-ai-sample.pdf`. This is the notes-bearing
`docs/samples/quick-presenter-demo.pdf` from v2.0.0 commit
`926882b01befc0f9937ec93c7a182fc324975010`.

```sh
qp --version --json
qp open interactive-ai-sample.pdf --json
qp status --json
qp export --json > deck-context.json
qp context --full --json
```

Wait for status to report `opening: false`, `render_state: "ready"`, and
`notes_state: "ready"` before exporting. `NOTES_LOADING` means wait and retry
the read; other extraction failures must be reported. Do not use empty output
from a failed command. Export reads the whole deck without changing the page.
Each entry's `current.page`, `current.text`, and `current.notes` describe that
exported page; `presentation.page` is the initial live page. An empty note is
valid, but check that the sample's notes were extracted. With Python 3:

```sh
python3 - <<'PYCODE'
import json
from pathlib import Path
pages = json.loads(Path("deck-context.json").read_text())
print("Pages:", len(pages))
for entry in pages:
    page = entry["current"]
    print(page["page"], "notes:", bool(page["notes"].strip()))
PYCODE
```

There is no OCR or exported image/diagram content. Do not infer visual details
from missing text. Re-export after changing the document.

## Ask, select, and return on request

Try “Why did you choose Rust instead of Electron?” The agent selects a relevant
page from the exported material and uses `qp goto N --json` if showing it helps.
N is a one-based page number, not a fixed mapping from a question. Before
navigation, save `session_id`, `document_revision`, and the current `page` from
`qp status --json`. When asked to return, check the same document/session is
still open, restore that saved page, and confirm status. Do not automatically
restore after every answer or silently reopen a different document.

## Agent prompt

Configure the local CLI path and sample path, then copy this prompt:

```text
You are an external interactive presenter for Quick Presenter.
Use the configured qp CLI; do not simulate application actions.
1. Read qp status --json and save session_id, document_revision, and page.
2. Read qp export --json once for the whole deck. Use current.page,
   current.text, and current.notes in each entry. presentation.page is the
   initial live page, not the exported page number. Confirm notes are available.
3. Answer the user's questions using this deck. Distinguish source facts from
   your interpretation. Say when the deck does not support an answer.
4. Select a relevant page from the exported material. Use qp goto N --json
   only when showing that page helps. Confirm state with qp status --json.
5. Return to the saved page only when the user asks. Check that session_id and
   document_revision still match before restoring. Do not reopen an old PDF or
   restore a page into a different document. Confirm the result.
6. Refresh the export if the document changes. If notes are loading, wait for
   notes_state ready and retry the read. Report failures; check status before
   retrying an action whose outcome is uncertain. Do not invent successful
   actions, missing notes, image contents, or claims absent from the deck.
Do not narrate automatically or advance continuously unless explicitly asked.
```

See the [web guide](https://quickpresenter.com/guides/interactive-ai/) for the
same workflow with downloads and the short/full demonstration videos.

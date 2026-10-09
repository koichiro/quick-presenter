# AI autopresentation reference

This Python 3.10+ standard-library example is an **external agent**, not a new
Quick Presenter feature. It uses only the stable Control Protocol v1 `qp` JSON
commands. No model, TTS provider, Python package, OBS, or streaming account is
required. Use matching GUI/CLI binaries from a supported distribution (macOS
Store control is not supported; development and Developer ID/DMG builds work).

## Try it with your PDF

From the repository root:

```sh
python3 examples/autopresent/autopresent.py slides.pdf --dry-run
# macOS bundled CLI, if qp is not on PATH:
python3 examples/autopresent/autopresent.py slides.pdf --dry-run \
  --qp '/Applications/Quick Presenter.app/Contents/MacOS/qp'
```

Dry-run still opens the real PDF/GUI and advances its pages. It prints narration
as JSON instead of generating or playing audio. It uses speaker notes verbatim,
falling back to source text (or an explicit image-only placeholder). This is a
pipeline check, not AI-generated narration or OCR. The GUI remains open at the
last page. For a completely offline check without qp, PDFium, a PDF, or a GUI:

```sh
python3 -m unittest discover -s examples/autopresent -v
```

## Connect a model and speech player

Implement two executables, or Python scripts, then supply their argv as JSON
arrays. No shell interpolation is performed:

```sh
python3 examples/autopresent/autopresent.py slides.pdf \
  --generate-command '["python3", "/path/to/generate.py"]' \
  --play-command '["python3", "/path/to/speak.py"]'
```

The generator reads one full `qp context` JSON object from stdin, including
`presentation`, `current` and `next`. It writes one object to stdout:

```json
{"narration":"Explain this slide using the current notes and source text."}
```

It can use any model/service or deterministic local code. Its prompt should
explain the **current** slide, using `next` only for transitions, and avoid
inventing content missing from the PDF. The player receives the narration string
on stdin. It can synthesize and play audio using any provider or local engine;
**exit 0 only after playback has finished**, including buffered audio. Logging
belongs on stderr. Any failure, malformed generation response, or adapter timeout
stops the presentation before navigation. Credentials belong in the adapter's
own environment/configuration, not in the reference implementation.

For example, a local macOS player can read stdin and wait for `say`:

```python
import subprocess
import sys
subprocess.run(["say", sys.stdin.read()], check=True)
```

`--adapter-timeout` defaults to 300 seconds per generation/playback call;
`--timeout` defaults to 50 seconds per qp process (allowing CLI startup and IPC
response deadlines); `--wait` defaults to 60 seconds per readiness/reconnection
phase. Ctrl-C stops the orchestrator. An adapter is responsible for cleaning up
any background audio children on cancellation; do not launch detached playback.

## Progress and recovery policy

1. Open the absolute PDF path once and pin session, revision, document, page and
   page count from the committed result. Never automatically replay `open`.
2. Poll `context --full --json` until both rendering and notes are ready. Retry
   read-only loading/busy/cancelled/transport failures within a deadline, checking
   status identity on reconnection. Empty notes/text are valid; failed extraction
   is fatal. No watch is needed: watch events do not acknowledge render readiness.
3. Generate narration; revalidate state; play to completion; revalidate again.
4. Send `next --json` **once** and verify its resulting identity/page. Repeat
   readiness checks before narrating the next slide. On the final slide, verify
   the successful `changed: false` no-op and finish.

There is no automatic retry of navigation. A navigation timeout/disconnection
is ambiguous even if status still shows the old page: an in-flight mutation can
commit later. The example attempts a diagnostic status query and then stops;
inspect the GUI and restart explicitly. Definitive command failures also stop.

Use exclusive presentation control while running this example. v1 has neither
compare-and-swap navigation nor an ownership lease. Snapshot checks catch
observed competing navigation/reopen/reload/close, blackout and pending opens,
but cannot prevent a change between the final check and `next`, or detect a
page changed away and back between snapshots. Unexpected resulting state stops
execution, but cannot undo a race already committed by the GUI. Similarly,
`render_state: ready` acknowledges cache readiness, not OS scanout. These are
protocol limits, not exactly-once or display synchronization guarantees. Do not
manually navigate during narration; the synchronous reference does not interrupt
audio already playing when an external change occurs.

## Integration tests

The tests exercise a two-slide source/notes deck through generation, completion
of audio, navigation and the final no-op. They cover notes loading, transient
read disconnection, mutation-response loss after commit (no double advance),
identity conflicts during generation/playback, failed audio, failed extraction,
readiness deadlines and real subprocess stdin/stdout adapter boundaries. These
are orchestrator tests, not replacements for `check_qp.py` or the protocol suite.

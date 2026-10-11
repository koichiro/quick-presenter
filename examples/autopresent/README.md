# AI autopresentation reference

This Python 3.10+ standard-library example is an **external agent**, not a new
Quick Presenter feature. It uses only the stable Control Protocol v1 `qp` JSON
commands. No model, TTS provider, Python package, OBS, or streaming account is
required. Use matching GUI/CLI binaries from a supported distribution (macOS
Store control is not supported; development and Developer ID/DMG builds work).

## Hear the speaker-note demo on macOS

The checked-in [demo PDF](demo.pdf) is a four-slide, roughly one-minute talk.
Its short audience-facing slides have complete spoken explanations embedded as
Marp speaker notes. No model, API key, TTS service, Marp installation, or extra
Python package is needed to play the PDF.

From the repository root, with a matching development GUI and CLI built using
`cargo build --bins`:

```sh
python3 examples/autopresent/autopresent.py examples/autopresent/demo.pdf \
  --qp ./target/debug/qp \
  --generate-command '["python3", "examples/autopresent/notes_narration.py"]' \
  --play-command '["python3", "examples/autopresent/speak_macos.py"]'
```

For a Developer ID/DMG installation, replace `--qp ./target/debug/qp` with
`--qp '/Applications/Quick Presenter.app/Contents/MacOS/qp'`. Keep the repository
as your working directory so the relative adapter paths resolve.

`notes_narration.py` returns only the **current slide's speaker notes**, verbatim.
It stops on missing or whitespace-only notes instead of silently substituting
slide text. `speak_macos.py` passes that narration on stdin to the built-in `say`
command and exits only after speech finishes. It replaces its process with
`say`, so the orchestrator can terminate playback directly on timeout. The PDF
stays on the final slide after all four notes have been read. Keep exclusive
control of the presentation while it runs; the concurrency limits below apply.

The deck and narration are in English. To choose an installed English voice or
adjust the speed, list voices with `say -v '?'` and replace the player argv with,
for example:

```json
["python3", "examples/autopresent/speak_macos.py", "--voice", "Samantha", "--rate", "170"]
```

Voice availability varies by Mac; the default uses the system voice. Other
platforms can use `notes_narration.py` with their own speech player.

### Use local Kokoro speech instead of say

The optional `speak_kokoro.py` player uses the open-source
[Kokoro library](https://github.com/hexgrad/kokoro) and the Apache-2.0 licensed
[Kokoro-82M model](https://huggingface.co/hexgrad/Kokoro-82M). It supports American
and British English voices for this demo and requires no API key or speech
service. The original `say` command above remains available.

Use Python 3.12 (tested on Apple Silicon macOS). From the repository root:

```sh
# Install python@3.12 as well if python3.12 is not already available.
brew install espeak-ng
python3.12 -m venv examples/autopresent/.venv-kokoro
examples/autopresent/.venv-kokoro/bin/python -m pip install \
  -r examples/autopresent/requirements-kokoro.txt
examples/autopresent/.venv-kokoro/bin/python \
  examples/autopresent/speak_kokoro.py --prepare
```

`--prepare` downloads the model configuration, weights and default `af_heart`
voice from a fixed revision of the official Hugging Face repository, installs
the English language data if needed, and checks that they load. Model files and
rendered audio live in `examples/autopresent/.kokoro/`; that directory and the
virtual environment are ignored by Git. Use `--cache-dir` to choose another
model/audio location. eSpeak NG supplies fallback pronunciations; the macOS
player uses its Homebrew library and data together.

Then use Kokoro as the playback adapter:

```sh
python3 examples/autopresent/autopresent.py examples/autopresent/demo.pdf \
  --qp ./target/debug/qp \
  --generate-command '["python3", "examples/autopresent/notes_narration.py"]' \
  --play-command '["examples/autopresent/.venv-kokoro/bin/python", "examples/autopresent/speak_kokoro.py"]'
```

As with `say`, replace `--qp` with the bundled CLI path for a Developer ID/DMG
installation. The engine renders **all chunks of a slide's narration into one
complete WAV before starting playback**. It saves that file for reuse, then
replaces its process with macOS `afplay`, which returns after the entire file
finishes. A generation or playback failure stops before navigation. Adapter
timeouts terminate the active synthesis/playback process directly.

The first run pauses on each slide while its narration is synthesized on the
CPU. Subsequent runs reuse the completed WAVs and start playback promptly.

Normal playback uses only cached model files and installed English data; it
does not download missing models during a presentation. Run `--prepare` again
after choosing another voice, for example `--voice bf_emma`. Include the same
`--voice` in the player argv. `--speed 0.9` slows speech down, and `--speed 1.1`
speeds it up. Audio is cached by the exact narration, voice, speed and model
revision; a failed synthesis never publishes a partial WAV. If a cached WAV is
damaged, the player stops; remove that file before trying again.

Python dependencies are only needed for Kokoro. The ordinary orchestration tests
remain runnable without them; run the suite with the virtual environment's
Python as well to exercise WAV generation tests:

```sh
examples/autopresent/.venv-kokoro/bin/python -m unittest discover -s examples/autopresent -v
```

To inspect the notes without playing audio (this still advances the real GUI):

```sh
python3 examples/autopresent/autopresent.py examples/autopresent/demo.pdf \
  --qp ./target/debug/qp --dry-run
```

Edit [demo.md](demo.md) to change the slides and the HTML-comment speaker notes.
Rebuild the PDF with Marp CLI, explicitly keeping the PDF annotations:

```sh
marp --no-config --pdf --pdf-notes examples/autopresent/demo.md \
  --output examples/autopresent/demo.pdf
```

The checked-in PDF was exported with Marp CLI 4.5.1. Marp is needed only when
rebuilding the deck. Exporting without `--pdf-notes` removes the narration from
the PDF and the notes-only adapter will stop.

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

The included `speak_macos.py` is a local macOS player using `say`; it can also
play narration returned by your own model adapter.

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

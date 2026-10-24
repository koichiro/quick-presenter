# Quick Presenter

[Website](https://quickpresenter.com/) ·
[Downloads](https://github.com/koichiro/quick-presenter/releases) ·
[CLI reference](docs/CLI.md) ·
[Changelog](CHANGELOG.md) ·
[Privacy](https://quickpresenter.com/privacy/)

![Rust](https://img.shields.io/badge/language-Rust-b7410e)
[![License](https://img.shields.io/badge/license-GPL--3.0--only-blue)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)
![Input](https://img.shields.io/badge/input-PDF-orange)

![Quick Presenter icon](assets/icons/png/quick-presenter-icon-128.png)

**PDF slides, presenter notes, and local automation.**

Quick Presenter plays prepared PDF slide decks in separate presenter and audience
windows. The presenter sees the current slide, next-slide preview, speaker notes,
timer, clock, and page status. Keyboard navigation, fullscreen, blackout, and
saved window placement keep the presentation close at hand.

The bundled `qp` CLI lets scripts and external AI agents read slide text and
notes, control playback, and watch presentation changes. Quick Presenter handles
PDF display and presentation state; your integration supplies interpretation,
narration, or other automation. The app does not create slides or bundle an AI
model or TTS engine.

Version **2.5.0** adds sandbox-compatible Mac App Store CLI control and improves
PDF opening, keyboard focus, and macOS fullscreen transitions. See the
[changelog](CHANGELOG.md) for the release highlights. Store updates are
reviewed and distributed separately from GitHub releases; check the installed
version before using version-specific features.

## Install

Use the package for your platform:

| Platform | Installation | CLI |
| --- | --- | --- |
| macOS direct | Download the signed/notarized DMG from [GitHub Releases](https://github.com/koichiro/quick-presenter/releases) and copy the app to Applications or a writable user Applications folder. | `Quick Presenter.app/Contents/MacOS/qp` |
| Mac App Store | Install from the [Mac App Store](https://apps.apple.com/app/quick-presenter-pdf-slides/id6820406883); the Store channel requires macOS 14.4 or later. | In **2.5.0**: `Quick Presenter.app/Contents/Helpers/ControlCLI.app/Contents/MacOS/qp` |
| Windows x64 | Install from [Microsoft Store](https://apps.microsoft.com/detail/9N913S9NJ6D1). | Registered `qp.exe` app execution alias |
| Ubuntu x64 | Install the released `quick-presenter_<version>_amd64.deb` from [GitHub Releases](https://github.com/koichiro/quick-presenter/releases). | `/usr/bin/qp` |

Mac App Store versions before 2.5.0 do not provide CLI control. The 2.5.0 Store
rollouts and customer-installed checks are tracked in
[#520](https://github.com/koichiro/quick-presenter/issues/520); a GitHub release
alone does not establish Store availability. Windows MSI, direct MSIX, and raw
CI executables are validation artifacts, not supported public downloads.

Launch the installed app and choose **Open**, or drop a PDF onto its presenter
or slide window. On macOS, install the app before using PDF drag and drop:
dropping onto the app icon while it is still on the read-only DMG is unsupported.

See [installation and launch details](docs/RELEASE.md) and
[CLI paths and PATH setup](docs/CLI.md#installation). Use the GUI and CLI from
the same package; keep the macOS CLI inside its signed app bundle.

## Start with the CLI

After setting up the matching CLI on PATH:

```sh
qp --version --json
qp open slides.pdf --json
qp status --json
```

`qp open` can start the companion GUI. In the Mac App Store build, the first
open of a new PDF presents a permission picker: select the exact requested file
within 120 seconds. Later opens can reuse its saved read-only bookmark.

Wait for `opening: false`, `render_state: "ready"`, and `notes_state: "ready"`.
Then read the deck or request controls:

```sh
# Export the entire deck without navigating
qp export --json > deck-context.json

# Read the current and next page, then navigate
qp context --full --json
qp goto 3 --json
qp next --json

# Observe state; stop watch with Ctrl-C
qp status --json
qp watch --json
```

Presentation commands other than `open` require a running GUI; metadata commands
such as `--version` and `--help` work without one. Content queries also require
an open PDF. Navigation acknowledges the logical page, so query status before
assuming rendering or notes are ready. Check exit codes and JSON errors on
stderr before retrying a request whose outcome is uncertain.

| Capability | Commands |
| --- | --- |
| Read slide text and speaker notes | `qp slide --full --json`, `qp context --full --json`, `qp export --json` |
| Open/close and control playback | `qp open slides.pdf --json`, `qp close --json`, `qp next --json`, `qp goto 3 --json`, `qp blackout on --json` |
| Query state and observe changes | `qp status --json`, `qp timer elapsed --json`, `qp watch --json` |

The interface remains **Control Protocol v1**, introduced in 2.0.0. Parse JSON
fields and error codes, accept additional object fields, and treat watch output
as NDJSON. Export returns one JSON array; `--include-next` enables the older
format with duplicated next-page context. Text extraction does not perform OCR
or interpret images; image-only pages may have empty text.

See the [CLI reference](docs/CLI.md), [protocol contract](docs/CONTROL_PROTOCOL.md),
and [export JSON Schema](docs/schemas/qp-export.schema.json).

## Integration examples

### Interactive AI Presenter

An external agent reads the deck with `qp export --json`, answers questions,
and selects relevant pages. It can save the session/document identity and page,
then restore that position on request after checking the same document is open.

[![Interactive AI Presenter with Codex](website/src/assets/interactive-ai-poster.jpg)](https://quickpresenter.com/#agentic-presentation)

[Watch the short demo](https://quickpresenter.com/#agentic-presentation)
(29 seconds, no audio; processing waits edited) ·
[Prompt and reproduction guide](docs/INTERACTIVE_AI_PRESENTER.md) ·
[Sample PDF with notes](website/src/assets/interactive-ai-sample.pdf)

### Autopilot

The external Python runner reads prepared speaker notes aloud using optional
[Kokoro](https://github.com/hexgrad/kokoro) TTS, waits for audio completion, then
requests the next page. It checks rendering and note readiness and stops on
failure. TTS setup and model downloads are separate from Quick Presenter.

[![Autopilot narration and page advance](website/src/assets/kokoro-demo-poster.jpg)](https://quickpresenter.com/#autopilot)

[Watch Autopilot](https://quickpresenter.com/#autopilot)
(72 seconds, English narration) · [Runner and setup](examples/autopresent/README.md)

## PDF slides and speaker notes

Export your finished deck from Keynote, PowerPoint, Google Slides,
[Marp](https://marp.app/), or [LaTeX Beamer](https://ctan.org/pkg/beamer).
The app opens the resulting PDF, not the authoring project. Password-protected
PDFs are rejected without replacing an already active deck.

Speaker notes require supported PDF annotations. Ordinary slide exports or
visible notes-page layouts do not guarantee extractable presenter notes.
The tested workflows are:

- **Marp:** write notes in Markdown comments and export with
  `marp --no-config --pdf --pdf-notes talk.md --output talk.pdf`.
- **Beamer:** use transparent `pdfcomment` Text annotations as described in
  the [speaker-note compatibility guide](docs/SPEAKER_NOTE_COMPATIBILITY.md).

Check the exported notes before a talk. See [PDF compatibility](docs/PDF_COMPATIBILITY.md)
and the [notes-bearing sample](website/src/assets/interactive-ai-sample.pdf).

On orderly exit, the app remembers the PDF and normal slide-window placement.
The next launch reopens the PDF at page 1 in windowed mode; an explicit startup
PDF takes precedence. Missing or inaccessible files leave the app ready to
select another deck. Display changes use an available-screen fallback; Wayland
leaves placement to the compositor. Valid external PDF saves reload the active
deck while preserving the current page where possible and retaining the last
good display through incomplete or failed saves.

See [keyboard controls](docs/KEYBOARD.md), [window placement](docs/SLIDE_WINDOW_PLACEMENT.md),
[privacy/reset](docs/PRIVACY.md), and [diagnostic logs](docs/PACKAGING.md#diagnostic-logs).

<details>
<summary>macOS screenshots (recorded with v2.0.0)</summary>

![Presenter window with slide text and speaker notes](website/src/assets/presenter-window-v2-macos.png)

![Audience slide window](docs/assets/slide-window-title-en-macos.png)

The screenshots use the [repository-owned sample deck](docs/samples/quick-presenter-demo.md).

</details>

## Build and development

The app is built with Rust, Slint, and PDFium. Install Rust stable and Python 3:

```sh
python3 scripts/fetch_pdfium.py
cargo build --bins
cargo run --bin quick-presenter -- --pdf tests/fixtures/marp-speaker-notes.pdf
```

For development CLI use, keep `target/debug/qp` beside the GUI binary and use
both from the same checkout. Packaged macOS rendering needs its signed helper
layout; follow [packaging instructions](docs/PACKAGING.md) for channel builds.

Run the standard checks after changing Rust, Slint, or build configuration:

```sh
cargo fmt --check
cargo check
cargo test
```

Dependency review uses `scripts/audit_deps.sh`; coverage uses
`scripts/coverage.sh` (80% line target). See [architecture](docs/ARCHITECTURE.md),
[repository conventions](AGENTS.md), [release instructions](docs/RELEASE.md),
and [validation results](docs/validation/README.md). Keep state transitions,
PDF ownership, rendering and error handling in Rust; Slint handles display and
input events.

## Contributing

Open or join an issue before broad changes. Keep PDF playback predictable and
editing, model inference, and speech orchestration outside the presentation
core. Start with [repository conventions](AGENTS.md) and the
[architecture notes](docs/ARCHITECTURE.md).

Related project: [Présentation.app](https://iihm.imag.fr/blanch/software/osx-presentation/),
a PDF presentation tool for macOS.

## License and security

Quick Presenter is licensed under [GPL-3.0-only](LICENSE). Release packages
include bundled-component licenses, corresponding-source notices, and
[artifact-bound SBOMs](docs/SBOM.md). See the
[PDF processing security policy](docs/PDF_RENDERING_SECURITY.md) for helper
isolation, update policy, and security limitations.

Paid distribution is allowed under the GPL, while preserving recipients' GPL
freedoms and providing corresponding source for the exact distributed build.
Bundled PDFium packages must retain PDFium and third-party license files; see
[packaging](docs/PACKAGING.md) for distribution requirements.

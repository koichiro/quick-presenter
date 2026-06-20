# Quick Presenter

[![CI](https://img.shields.io/badge/CI-GitHub%20Actions-2088FF?logo=githubactions&logoColor=white)](https://github.com/koichiro/quick-presenter/actions/workflows/ci.yml)
[![Build Binaries](https://img.shields.io/badge/builds-GitHub%20Actions-2088FF?logo=githubactions&logoColor=white)](https://github.com/koichiro/quick-presenter/actions/workflows/build-binaries.yml)
![Rust](https://img.shields.io/badge/language-Rust-b7410e)
![Coverage target](https://img.shields.io/badge/coverage%20target-80%25-brightgreen)
[![License](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)
![Input](https://img.shields.io/badge/input-PDF-orange)

![Quick Presenter icon](assets/icons/png/quick-presenter-icon-128.png)

Quick Presenter is a lightweight cross-platform presenter tool for PDF slide
decks, built for academic talks, research presentations, technical conferences,
and online presentations over tools such as Zoom and Google Meet.

Quick Presenter focuses on one job: open a PDF slide deck and help the speaker
present it reliably. It is not a slide editor, a deck authoring tool, or a
document management app.

## Demo

![Quick Presenter demo showing PDF navigation in the presenter window](docs/assets/quick-presenter-demo.gif)

## Screenshots

### Presenter Window

![Presenter window with an English sample slide and speaker notes](docs/assets/presenter-window-en.png)

### Slide Window

![Slide window with an English title slide](docs/assets/slide-window-title-en.png)

![Slide window with an English sample workflow slide](docs/assets/slide-window-en.png)

![Slide window with a Japanese sample slide](docs/assets/slide-window-ja.png)

The screenshots use the repository-owned sample deck in
[docs/samples/quick-presenter-demo.md](docs/samples/quick-presenter-demo.md).

## What Quick Presenter Solves

Many speakers create slides in a separate authoring tool, then export the final
deck to PDF. PDF is stable and portable, but ordinary PDF viewers are not
designed around live presentation workflows.

Quick Presenter provides a focused playback experience:

- A presenter window with the current slide, next slide preview, speaker notes,
  elapsed time, clock, and page status.
- A separate slide window for audience-facing output.
- Keyboard-first navigation that works well with presentation remotes.
- A compact PDF-only workflow that avoids editor complexity during a talk.

The goal is to make presenting a prepared PDF feel predictable, especially when
switching between in-person talks, conference rooms, and online meetings.

## PDF Authoring Tools

Quick Presenter does not create or edit slide decks. It presents PDF files
exported from authoring tools such as:

| Tool | Notes |
| --- | --- |
| Keynote | Export the finished deck to PDF before presenting. |
| PowerPoint | Export the finished deck to PDF before presenting. |
| Google Slides | Export the finished deck to PDF before presenting. |
| [Marp](https://marp.app/) | OSS Markdown-based slide authoring; exported PDF speaker notes are supported in the presenter window. |
| [LaTeX Beamer](https://ctan.org/pkg/beamer) | OSS LaTeX class for PDF-first slide decks. |

The supported input remains the exported PDF, not the source project from any
authoring tool.

## Current Capabilities

- Open prepared PDF slide decks from the UI or at startup.
- Present with separate speaker-facing and audience-facing windows.
- Keep presenter context visible through page status, next-slide preview,
  speaker notes, timer, and clock.
- Support keyboard-first navigation, including common presenter remote keys.
- Bundle the PDF runtime in packaged builds for predictable playback.

## Basic Usage

Run the app:

```sh
cargo run --bin qp
```

Open a PDF at startup:

```sh
cargo run --bin qp -- --pdf path/to/slides.pdf
```

For development verification, this repository includes a small fixture PDF:

```sh
cargo run --bin qp -- --pdf tests/fixtures/marp-speaker-notes.pdf
```

The README screenshot deck can also be opened directly:

```sh
cargo run --bin qp -- --pdf docs/samples/quick-presenter-demo.pdf
```

The Cargo package remains `quick-presenter`, while the development executable is
named `qp`.

Keyboard controls are documented in [docs/KEYBOARD.md](docs/KEYBOARD.md).
Recent-file privacy behavior is documented in
[docs/PRIVACY.md](docs/PRIVACY.md).

## Build and Development

Install Rust stable and Python 3, then fetch the local PDFium binary used for
development:

```sh
python3 scripts/fetch_pdfium.py
```

CI and release packaging fetch PDFium with `--clean` so extraction starts from a
clean output directory. See [docs/PACKAGING.md](docs/PACKAGING.md) for the
release-oriented PDFium fetch requirements.

Build the app:

```sh
cargo build --bin qp
```

Run the standard checks:

```sh
cargo fmt --check
cargo check
cargo test
```

Coverage is measured with:

```sh
scripts/coverage.sh
```

Detailed packaging, release, keyboard, and speaker-note behavior is documented
under [docs/](docs/).

## Releases

Quick Presenter is an OSS project focused on reliable PDF presentation
playback. Download and launch steps for packaged artifacts are documented in
[docs/RELEASE.md](docs/RELEASE.md).

Packaging and signing details for maintainers are documented in
[docs/PACKAGING.md](docs/PACKAGING.md).

## Roadmap

The roadmap is intentionally guided by the core product scope: stable PDF
presentation playback. Future work should make live talks more predictable,
reduce setup risk, and improve presenter confidence without turning Quick
Presenter into a slide editor or document management system.

## Related Projects

- [Présentation.app](https://iihm.imag.fr/blanch/software/osx-presentation/) is
  a similar PDF presentation tool for macOS.

## Contributing

Contributions are welcome.

Before starting broad changes, please open or join an issue so the scope can
stay clear. Quick Presenter prioritizes predictable live presentation behavior,
so changes should keep PDF playback reliable and avoid turning the app into a
slide editor.

Useful starting points:

- Read [AGENTS.md](AGENTS.md) for repository conventions.
- Read the architecture notes in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
- Check keyboard behavior in [docs/KEYBOARD.md](docs/KEYBOARD.md).
- Check speaker-note behavior in [docs/NOTES.md](docs/NOTES.md).
- Run `cargo fmt --check`, `cargo check`, and `cargo test` before submitting a
  pull request.

## License

Quick Presenter is licensed under the GNU General Public License v3.0 or later.
See [LICENSE](LICENSE).

Paid distribution is allowed under the GPL, but every binary distribution must
preserve the recipient's GPL freedoms and provide the corresponding source code
for that exact build.

Quick Presenter uses PDFium through `pdfium-render`. Packaged builds that bundle
PDFium must include PDFium and third-party component license files. See
[pdfium/LICENSE](pdfium/LICENSE) and [docs/PACKAGING.md](docs/PACKAGING.md).

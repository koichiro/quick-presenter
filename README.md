# Quick Presenter

[![CI](https://github.com/koichiro/quick-presenter/actions/workflows/ci.yml/badge.svg)](https://github.com/koichiro/quick-presenter/actions/workflows/ci.yml)
[![Build Binaries](https://github.com/koichiro/quick-presenter/actions/workflows/build-binaries.yml/badge.svg)](https://github.com/koichiro/quick-presenter/actions/workflows/build-binaries.yml)
![Rust](https://img.shields.io/badge/language-Rust-b7410e)
![Coverage target](https://img.shields.io/badge/coverage%20target-80%25-brightgreen)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)
![Input](https://img.shields.io/badge/input-PDF-orange)

![Quick Presenter icon](assets/icons/png/quick-presenter-icon-128.png)

Quick Presenter is a lightweight cross-platform presenter tool for PDF slide
decks, built for academic talks, research presentations, technical conferences,
and online presentations over tools such as Zoom and Google Meet.

Quick Presenter focuses on one job: open a PDF slide deck and help the speaker
present it reliably. It is not a slide editor, a deck authoring tool, or a
document management app.

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

Many speakers create slides in tools such as Keynote, PowerPoint, Google
Slides, Marp, or LaTeX Beamer, then export the final deck to PDF. PDF is stable
and portable, but ordinary PDF viewers are not designed around live
presentation workflows.

Quick Presenter provides a focused playback experience:

- A presenter window with the current slide, next slide preview, speaker notes,
  elapsed time, clock, and page status.
- A separate slide window for audience-facing output.
- Keyboard-first navigation that works well with presentation remotes.
- A compact PDF-only workflow that avoids editor complexity during a talk.

The goal is to make presenting a prepared PDF feel predictable, especially when
switching between in-person talks, conference rooms, and online meetings.

## Current Capabilities

- Open a PDF from the app UI.
- Open a PDF at startup with `--pdf`.
- Show a presenter window and a separate slide window.
- Show the current page, next-page preview, page count, document title, timer,
  and current clock.
- Extract and display speaker notes from supported PDF speaker-note annotations.
- Navigate with buttons, menus, keyboard shortcuts, and common presenter remote
  keys.
- Jump to the first and last page.
- Toggle fullscreen for the audience-facing slide window.
- Reopen recently used PDF files.
- Show application, version, Quick Presenter license, and PDFium license
  information in the About dialog.
- Provide cross-platform icon assets and macOS application icon wiring.

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

## Build and Development

Install Rust stable and Python 3, then fetch the local PDFium binary used for
development:

```sh
python3 scripts/fetch_pdfium.py
```

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

Regenerate the README screenshot sample PDF from Marp Markdown:

```sh
marp docs/samples/quick-presenter-demo.md --pdf --pdf-notes --allow-local-files -o docs/samples/quick-presenter-demo.pdf
```

Coverage is measured with:

```sh
scripts/coverage.sh
```

Platform packaging notes are in [docs/PACKAGING.md](docs/PACKAGING.md).

## Release Status

Quick Presenter is moving toward an MVP-quality OSS release. The repository can
build development binaries in CI, but full end-user packaging is still being
completed.

Current distribution work focuses on:

- locating bundled PDFium reliably from packaged builds,
- macOS app bundle packaging,
- Windows executable icon and packaging metadata,
- Linux desktop entry and hicolor icon installation,
- smoke tests for staged release artifacts.

Until those items are complete, development runs may require a repository-local
PDFium directory or `PDFIUM_DYNAMIC_LIB_PATH`.

## Roadmap

Near-term work is focused on making live presentation playback reliable enough
for MVP use:

- Smoother page transitions through render caching and lightweight preloading.
- Presentation controls such as black screen mode for breaks, Q&A, and setup.
- Reliable packaging for macOS, Windows, and Linux.
- Clear release documentation for users who download packaged builds.

After the MVP, larger product directions include:

- Better workflows for online meetings and screen sharing.
- Tablet-friendly presenter controls and remote-control surfaces.
- App Store and Microsoft Store packaging, including donation-oriented
  distribution if appropriate.
- Slide annotation tools, such as laser-pointer-style marking or temporary
  drawing during a talk.
- Faster navigation for large decks, including thumbnail-based page jumping.
- More advanced display and projector management.

The roadmap is intentionally directional. Features should stay aligned with the
core product scope: stable PDF presentation playback.

## Related Projects

- [Présentation.app](https://iihm.imag.fr/blanch/software/osx-presentation/) is
  a similar PDF presentation tool for macOS.

## Contributing

Contributions are welcome as the project moves toward OSS publication.

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

Quick Presenter is licensed under the Apache License 2.0. See
[LICENSE](LICENSE).

Quick Presenter uses PDFium through `pdfium-render`. Packaged builds that bundle
PDFium must include PDFium and third-party component license files. See
[pdfium/LICENSE](pdfium/LICENSE) and [docs/PACKAGING.md](docs/PACKAGING.md).

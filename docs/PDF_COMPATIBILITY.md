# PDF Export Compatibility

Quick Presenter accepts PDF files exported by common slide-authoring tools. The
input preflight currently requires the `%PDF-` header to begin at byte zero.
This document records representative exports used to validate that policy.

## Compatibility fixtures

| Authoring tool | Fixture | Export details | Pages | Header offset | Result |
| --- | --- | --- | ---: | ---: | --- |
| Google Slides | `tests/fixtures/google-slide.pdf` | Creator metadata: Google | 2 | 0 | Pass |
| Keynote | `tests/fixtures/keynote-15-macos.pdf` | Keynote 15 on macOS; Quartz PDFContext | 2 | 0 | Pass |
| LaTeX Beamer | `tests/fixtures/latex-beamer.pdf` | Generated from the adjacent source with Tectonic 0.16.9; includes a `pdfcomment` speaker note | 2 | 0 | Pass |
| Marp | `tests/fixtures/marp-speaker-notes.pdf` | Creator and producer metadata: Created by Marp | 3 | 0 | Pass |
| PowerPoint | `tests/fixtures/power-point-16-macos.pdf` | PowerPoint 16 on macOS; Quartz PDFContext | 2 | 0 | Pass |

All representative exports place the PDF header at byte zero. No compatibility
exception or prefix search is therefore needed. Keeping the strict check also
ensures that obvious non-PDF inputs are rejected before they reach PDFium.

The fixtures are exercised by unit tests in `src/pdf.rs`. One checks every file
against the input preflight. A second opens each file with PDFium, verifies its
page count, and renders its first page when the local PDFium library is
available. The existing preflight tests separately cover empty files,
directories, missing files, short files, non-PDF headers, and the input size
limit.

The Beamer source defines `\presenternote` to retain Beamer's native `\note`
markup while also storing the same text in a transparent PDF `Text` annotation.
The annotation uses the standard `Note` icon name. This matches the speaker-note
representation that Quick Presenter already extracts without rendering
annotation icons onto the audience slide.

## Reproducing the Beamer fixture

The Beamer fixture has a small, reviewable source file next to the generated
PDF. With Tectonic installed, regenerate it from the repository root:

```sh
tectonic tests/fixtures/latex-beamer.tex --outdir tests/fixtures
```

To inspect any fixture's initial bytes and basic PDF metadata, use:

```sh
xxd -l 16 tests/fixtures/example.pdf
pdfinfo tests/fixtures/example.pdf
```

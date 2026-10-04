# Speaker Notes

Quick Presenter treats PDF files as the only presentation input. Speaker notes
are therefore read from metadata that already exists inside the PDF instead of
from a separate project file.

The supported notes representation is the one emitted by Marp and by the tested
LaTeX Beamer `pdfcomment` workflow. Each slide page can contain PDF annotations
with these properties:

- `Subtype`: `Text`
- `Name`: `Note`
- `Contents`: the speaker note text

Quick Presenter maps those annotations to the corresponding one-based PDF page
number. Multiple note annotations on the same page are joined with a blank line.
Empty notes and page number zero are ignored.

Speaker note annotations are used only as presenter metadata. They are not
rendered into slide images, so note markers and other PDF annotation icons do
not appear during playback.

Extraction requires `Text` annotations with non-empty `Contents` and one of
these supported annotation fingerprints:

- Marp: `/Rect [0 20 20 20]`, `/C [1 0.92 0.42]`, and `/CA 0.25`.
  PDFium exposes that color and opacity as RGBA bytes `[255, 234, 107, 63]`.
- The documented Beamer macro: `/T (Quick Presenter)` and zero opacity
  (`/CA 0`). The author is an intentional marker in this supported workflow.

There is no fallback for arbitrary text comments or missing metadata. The
document's Creator/Producer is not required: copied or rewritten PDFs can retain
supported annotations while changing document metadata.

`/Name /Note` is an icon choice, not a unique speaker-note marker. The pinned
`pdfium-render 0.9.1` public `name()` API reads the unrelated `/NM` identifier;
it exposes neither raw `/Name` nor `/Subj`. Detection therefore uses the public
bounds, stroke color (including opacity), and annotation creator (`/T`) APIs.
It does not inspect native handles or reinterpret `/NM`. This prevents the
notes-disappearance regression from issue #75 without changing PDFium ownership.

These fingerprints are compatibility heuristics, not proof of author intent.
A comment with identical metadata is indistinguishable from supported notes;
altering these fields can also make a supported note unrecognizable. Ordinary
comments, including those using a Note icon, are excluded. Beamer workflows
that change the macro's author marker are outside the supported format.

Real-generator compatibility tests cover Marp, Beamer, the README sample, and
long notes. `tests/fixtures/generate_note_annotations.py` reproducibly creates
the generic-comment and mixed-note PDFs used for rejection and coexistence tests.

For Marp, the workflow is:

1. Authors write slides and speaker notes in Markdown.
2. Marp generates a PDF that embeds the notes as PDF comments.
3. Quick Presenter opens the PDF and shows the notes in the presenter window.

For LaTeX Beamer, use `pdfcomment` to add a transparent `Text` annotation on the
same slide as each native `\note`. The complete tested macro and usage example
are documented in the README, and the reproducible source is in
`tests/fixtures/latex-beamer.tex`.

Future formats can be added by extending the extraction layer, but they should
still produce the same in-memory `SpeakerNotes` model so the presenter UI does
not need to know where the notes came from.

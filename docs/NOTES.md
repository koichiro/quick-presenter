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

See [speaker-note PDF compatibility research](SPEAKER_NOTE_COMPATIBILITY.md)
for measured PowerPoint PDF saving, export, and print-to-PDF, Keynote, Google
Slides, and native Beamer exports, provisional assessments of Windows PDF/XPS
and Impress, and the remaining sample coverage.
Visible notes-page text is not supported presenter-note metadata.

## Automatic presenter text size

Speaker notes automatically use the largest whole-pixel font size between
12 and 24 logical pixels that fits the notes viewport. These fixed bounds are
independent of the platform theme (9–18pt in Slint's unit conversion). There are
no text-size controls or saved preferences. The **No notes** placeholder stays
at 12px.

Slint measures the note using read-only, invisible TextInput probes for each
candidate size, with the same font and word wrapping as the visible notes.
Visible notes and measurement probes share a line-height factor of 1.25 relative
to the font's natural line height, giving Japanese and other multiline notes
more breathing room. Rust chooses the largest measured height that fits the
viewport, including
10px of bottom padding. Measurements depend on the available width, so explicit
newlines, Japanese text, and wrapped paragraphs are handled without estimating
from character counts. The probes do not depend on the chosen size, avoiding
feedback between size selection and measurement.

The size is recalculated when notes arrive or change, pages change, or the
notes viewport changes through resizing or a different slide aspect ratio.
If the full note does not fit at 12px, it stays at 12px and scrolls instead of
shrinking further. Resizing retains the pixel scroll offset, clamped to the
new range when necessary; text reflow can change the words at that offset.
Automatic sizing does not request PDF rendering or modify notes content,
page navigation, the timer, or the audience display.

The presenter minimum window size remains 800 × 560 logical pixels, with
at least 200px reserved for the notes area. There are no additional focusable
controls, so the existing presentation keyboard workflow is preserved.

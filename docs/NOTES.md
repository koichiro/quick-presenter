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

The current PDFium extraction path reads `Text` annotations with non-empty
`Contents`. The Marp `/Name /Note` value documents the expected source format,
but `pdfium-render`'s public annotation name API reads the `/NM` identifier
field rather than the text annotation icon name. Until Quick Presenter has a
small, well-tested low-level PDFium helper for the raw `/Name` dictionary key,
annotations without an exposed name are accepted as a fallback.

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

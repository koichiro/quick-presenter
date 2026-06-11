# Speaker Notes

Quick Presenter treats PDF files as the only presentation input. Speaker notes
are therefore read from metadata that already exists inside the PDF instead of
from a separate project file.

The first supported notes format is the one emitted by Marp when speaker notes
are exported into the generated slide PDF. In that format, each slide page can
contain PDF annotations with these properties:

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

This keeps the initial workflow simple:

1. Authors write slides and speaker notes in Markdown.
2. Marp generates a PDF that embeds the notes as PDF comments.
3. Quick Presenter opens the PDF and shows the notes in the presenter window.

Future formats can be added by extending the extraction layer, but they should
still produce the same in-memory `SpeakerNotes` model so the presenter UI does
not need to know where the notes came from.

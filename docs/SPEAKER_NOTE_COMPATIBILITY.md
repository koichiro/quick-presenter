# Speaker-note PDF compatibility research

Research for [issue #168](https://github.com/koichiro/quick-presenter/issues/168),
recorded on 2026-10-07, with PowerPoint verification added on 2026-10-08.
This investigation does not change note extraction or
declare additional authoring tools supported.

## Recommendation and evidence status

Keep the existing Marp and documented Beamer `pdfcomment` workflows. The new
PowerPoint, Keynote, Google Slides, and native Beamer samples provide no additional safe
candidate for extracting private presenter notes from ordinary slide pages.
Their note-inclusive exports place notes in the visible page content. Reading
that text is possible, but identifying it as speaker notes and preserving a
clean audience slide would require additional rules for layout and rendering.

Controlled PDF output now covers all four required authoring tools, including
PowerPoint for Mac's note-inclusive print-to-PDF path and activated Save As /
Export PDF paths. Both commands were tested with Best for printing and Best
for electronic distribution and accessibility; none of these four exports
preserves the notes. Windows PDF/XPS was not tested. LibreOffice remains an optional documentation-only comparison and
was not installed. Existing two-page compatibility PDFs lack a known source
deck with distinct notes and cannot establish whether an exporter drops notes.

## Compatibility matrix

"Possible but fragile" means readable visible page text without a verified,
tool-specific semantic mapping. It is not supported presenter-note input.
"Not supported" includes absent notes or exports that cannot supply the
original slide pages. Recommendations marked provisional require samples.

| Tool | Export mode | Notes representation | Extractable with PDFium / `pdfium-render`? | Recommendation | Evidence |
| --- | --- | --- | --- | --- | --- |
| PowerPoint 16.113.3 for Mac | Print > Slides > PDF > Save as PDF | Notes absent from the tested slide-only output | No note annotations or separate payload found | Not supported for notes | Three-page controlled print-to-PDF output |
| PowerPoint 16.113.3 for Mac | Print > Notes > PDF > Save as PDF | Visible note text below a raster slide thumbnail; no structure tree or annotations | Public page-text API reads both notes; no semantic note boundary or original slide-page reference | Possible but fragile | Three-page controlled print-to-PDF output |
| PowerPoint 16.113.3 for Mac | Save As / Export PDF, Best for printing | Notes absent; untagged slide pages | No note annotations or separate payload found | Not supported for notes | Two three-page controlled exports after activation |
| PowerPoint 16.113.3 for Mac | Save As / Export PDF, Best for electronic distribution and accessibility (Microsoft online service) | Notes absent; slide-content structure tags and document XMP | No note annotations; tags and XMP contain no note payload | Not supported for notes | Two three-page controlled exports after activation |
| PowerPoint | Windows PDF/XPS notes-page output | Documented notes-page layout; actual PDF dictionaries unverified | Page text may be readable; no verified semantic note mapping | Possible but fragile, provisional | Microsoft documentation; Windows not tested |
| Keynote 15.4 | PDF, presenter notes unchecked | Notes absent from the tested slide PDF | No note annotations or separate payload found | Not supported for notes | Three-page controlled export |
| Keynote 15.4 | PDF, Include presenter notes | Visible text below a reduced slide; ordinary `/P` structure tags, no note annotation | Page text is readable; tags do not uniquely identify presenter notes | Possible but fragile | Three-page controlled export |
| Google Slides | Download / standard PDF export | Notes absent from tested slide PDF | No note annotations or separate payload found | Not supported for notes | Three-page controlled export |
| Google Slides | Print preview > 1 slide with notes > Download as PDF | Visible text below a reduced slide, no structure tree or note annotations | Page text is readable; no semantic note boundary | Possible but fragile | Three-page controlled export |
| LaTeX Beamer | `hide notes` (default) | Native `\note` content omitted | No native note payload found | Not supported for notes | Three-page controlled build |
| LaTeX Beamer | `show notes` | Separate visible notes pages interleaved with slides | Page text is readable, but notes are not metadata on slide pages | Possible but fragile | Five-page controlled build |
| LaTeX Beamer | `show only notes` | Only visible notes pages, with small slide thumbnails | Text readable; original full-size slide pages absent | Not supported as an ordinary slide deck with presenter metadata | Two-page controlled build from three frames |
| LaTeX Beamer | `show notes on second screen=right` | Audience slide and visible notes composed into one double-width PDF page | Text readable; separating audience and notes needs an explicit layout mode | Possible but fragile for automatic extraction | Three-page controlled build |
| LaTeX Beamer | Documented `pdfcomment` macro, native notes hidden | Transparent `Text` annotation on its owning slide, `/T (Quick Presenter)`, `/CA 0` | Yes, existing public annotation APIs and extractor fingerprint | Safe support candidate; already supported within the documented macro | Three-page controlled build plus existing compatibility fixture |
| LibreOffice Impress | `ExportNotesPages`; optionally `ExportOnlyNotesPages` | Documented notes-page export; emitted PDF structure unverified | Page text may be readable; no verified semantic note mapping | Possible but fragile, provisional | LibreOffice PDF filter documentation only |
| Marp | PDF with speaker notes | Existing supported text annotations on slide pages | Yes, existing extractor | Safe support candidate; already supported within the tested fingerprint | Existing `tests/fixtures/marp-speaker-notes.pdf` |

## Controlled samples and inspection

Samples and sources are in [research/speaker-notes](research/speaker-notes).
All new source decks have three slides: Alpha and Beta contain different notes,
and Gamma has no note. The note sentinels are `QP168-NOTE-ALPHA` and
`QP168-NOTE-BETA`. These are research identifiers, not detection rules.

| Files | Producer / mode | PDF pages | Pages with visible Alpha / Beta note text | Text annotations containing notes |
| --- | --- | ---: | --- | --- |
| `powerpoint-print-slides.pdf`, `powerpoint-print-notes.pdf` | PowerPoint 16.113.3 for Mac; Quartz on macOS 26.7; Print > Slides / Notes > PDF > Save as PDF; all slides, color, fit to paper, A4 landscape / portrait | 3 / 3 | none / 1 and 2 | none in either |
| `powerpoint-save-as-print.pdf`, `powerpoint-export-print.pdf` | Activated PowerPoint 16.113.3 for Mac; Save As / Export > PDF > Best for printing; Quartz, 720 × 405 pt | 3 / 3 | none in either | none in either |
| `powerpoint-save-as-accessible.pdf`, `powerpoint-export-accessible.pdf` | Same source and app; Save As / Export > PDF > Best for electronic distribution and accessibility; Microsoft online service, 720 × 405.36 pt | 3 / 3 | none in either | none in either |
| `keynote-slides.pdf`, `keynote-with-notes.pdf` | Keynote 15.4; Quartz on macOS 26.7; highest quality; comments, builds, skipped slides, password off | 3 / 3 | none / 1 and 2 | none in either |
| `google-slides.pdf`, `google-with-notes.pdf` | Google Slides web; standard export / print preview, one slide with notes, portrait, background retained | 3 / 3 | none / 1 and 2 | none in either |
| `beamer-hidden.pdf` | Tectonic 0.17.0; native notes hidden | 3 | none | none |
| `beamer-interleaved.pdf` | Native notes shown | 5 | 2 and 4 | none; notes pages have navigation links |
| `beamer-only.pdf` | Only native notes | 2 | 1 and 2 | none; pages have navigation links |
| `beamer-second-screen.pdf` | Native notes on the right | 3 | 1 and 2 | none; pages have navigation links |
| `beamer-annotations.pdf` | Transparent `pdfcomment` annotations | 3 | none | owning pages 1 and 2 |

The inspection checks annotations, standard document metadata, catalog/page
metadata streams, attachments, associated-file entries, structure tags, page
dimensions, and visible note text. No tested PDF has embedded files, page
metadata streams, or associated-file entries. The two PowerPoint online-service
exports have catalog XMP streams containing dates and document identifiers,
without the note strings; the other outputs have no catalog metadata stream.
Standard document metadata contains dates, titles, and producer information,
not the two note strings.
Keynote's notes export adds two ordinary `/P` tags to its structure tree; slide
content also uses `/P`. Google Slides and these Beamer builds have no structure
tree. Neither PowerPoint print output nor Best for printing export has
structure tags. The online-service exports have `/Slide` (3), `/H1` (3), `/P`
(3), `/Textbox` (6), and `/Span` (6), describing the visible slide text. This is evidence for
these samples, not proof that every version or
export path behaves identically.

[inspection.json](research/speaker-notes/inspection.json) records file hashes,
page dimensions, visible note markers, annotation fields, metadata, and tag
counts. It was generated with pypdf 6.10.0 and Poppler. pypdf's text extraction
misdecodes some Quartz font mappings in these samples, so the inspection uses
`pdftotext` for visible text and pypdf for PDF structure. Notes-page layouts were
also rendered and visually inspected. The Beamer notes-only PDF causes pypdf
warnings about undefined navigation objects; the second-screen build emits
duplicate navigation-destination warnings. Neither is a reliable note mapping.
The PowerPoint notes PDF and the two Best for printing exports also trigger
pypdf warnings about zero-offset xref entries. Poppler and PDFium successfully
process all their pages; these parser
diagnostics do not provide a semantic slide-to-note association.

[smoke-open.txt](research/speaker-notes/smoke-open.txt) records processing with
the existing debug Quick Presenter binary: PDFium open, first-page rendering,
and note requests for every PDF page. It does not assert returned note text.
Application sources and extraction behavior were not changed or rebuilt for
this research. The bundled PDFium version is 156.0.8076.0.

[pdfium-api.txt](research/speaker-notes/pdfium-api.txt) independently verifies
the two note strings using `pdfium-render 0.9.1` public APIs. `PdfPageText::all()`
reads the visible notes from each note-layout export. `contents()` reads only
the two Beamer text annotations, on pages 1 and 2; page 3 is empty. The probe
linked an existing local `pdfium-render` build without modifying the app.

### PowerPoint follow-up, 2026-10-08

PowerPoint 16.113.3 opened the retained `google-source.pptx` in view-only mode.
Its print preview and the resulting PDF confirmed the distinct Alpha and Beta
notes, plus Gamma with no note text. All three slides were selected. Printing
used A4, color, fit to paper, and the default one-page-per-sheet layout; Slides
selected landscape and Notes selected portrait. Only PDF saving was performed,
not a physical print job. The PDF save sheet's Title and Author were set to the
research title and `Quick Presenter`; no security option was enabled.

The slide-only PDF contains readable slide text and no note markers. The notes
PDF has one image XObject per page for the slide thumbnail, while the two note
strings are ordinary page text. Neither output has annotations, a structure
tree, attachments, associated files, or catalog/page metadata streams. The
PDFium public-API probe reads Alpha on page 1 and Beta on page 2, finds no note
on page 3, and finds no `Text` annotation on any page. All six output pages were
rendered and visually inspected; both PDFs passed the application's helper
open/render/notes smoke check.

The image XObjects are a layout observation, not a format contract. Extracting
the image could recover these sample thumbnails, but would not establish a
general original-slide mapping or preserve full-size slide rendering across
notes masters, image content, and exporter versions. Automatic support is not
recommended from this evidence.

### Activated PowerPoint Save As / Export follow-up, 2026-10-08

After subscription activation, both commands became available in the same
PowerPoint 16.113.3 installation. Each PDF dialog offered Best for printing and
Best for electronic distribution and accessibility (Microsoft online service).
Neither dialog exposed a notes-page or presenter-note option. The unchanged
`google-source.pptx` was exported through both commands using both choices,
producing four additional PDFs. Online conversion of this three-slide dummy
research deck was explicitly authorized by the user.

All four outputs contain three full-size slide pages and no visible note
markers, annotations, attachments, associated files, or page metadata streams.
Best for printing uses Quartz and produces untagged 720 × 405 pt pages. The
online-service output has 720 × 405.36 pt pages, slide-content structure tags,
and document-level XMP with dates and UUIDs. Its structure tags do not contain
speaker notes. An additional traversal of reachable PDF values and decoded
streams found neither literal sentinel; this supplements the public page-text
and annotation checks rather than establishing a general format guarantee.

The public PDFium APIs find no note text or annotation contents in any of the
12 pages. All 12 pages were rendered and visually inspected; all four outputs
passed the existing helper open/render/notes smoke check. The sandbox prevented
creation of the diagnostic log in `~/Library/Logs/Quick Presenter`; the smoke
command still completed successfully. No application source was changed.
The activated paths therefore move from provisional documentation evidence to
measured absence of notes for this version, source deck, and these settings.

## API feasibility and reliable page mapping

The pinned `pdfium-render = 0.9.1` source was inspected, including
`pdf/document/page/annotation.rs`, `pdf/document/page/text.rs`,
`pdf/document/metadata.rs`, and `pdf/document/attachments.rs`.

- The public page annotation iterator and `annotation_type()`, `contents()`,
  `creator()`, `bounds()`, and `stroke_color()` expose the information needed
  for the existing note fingerprints. An annotation belongs directly to its
  PDF page; no note-page pairing or text-layout inference is needed. See
  [NOTES.md](NOTES.md) and the existing extraction in `src/pdf.rs`.
- `PdfPageText::all()` and text geometry APIs can expose visible page text.
  They do not distinguish note text from slide content, headings, handout text,
  or dates. Extracting a string does not create a safe presenter-note mapping.
- `PdfMetadata` exposes eight standard document-level tags, not arbitrary
  per-slide note dictionaries. `PdfAttachments` exposes embedded files, but
  none occur in these samples. Parsing an embedded authoring project would
  require a separate format parser and mapping contract; it is not justified
  by this evidence.
- Native structure-tree functions exist in the bindings, but this version has
  no corresponding high-level page structure-tree accessor. More significantly,
  the observed tags have no unique speaker-note role. Additional native API
  plumbing would not solve that semantic ambiguity.

Notes-page ordinal position is not an original-slide identifier. The Beamer
sample already breaks a universal alternating-page rule: the unnoted Gamma
frame has no following notes page. Overlays, multiple notes, skipped slides,
build stages, page selections, or custom notes templates need separate tests.
Likewise, recognizing a double-width page is insufficient to infer which half
is private. Automatic extraction or cropping would change audience playback
and is outside this research issue.

## Reproduction and remaining coverage

To rebuild Beamer exports and regenerate the structure report from the repo
root (Python with pypdf, Tectonic, and Poppler required):

```sh
python3 docs/research/speaker-notes/rebuild_beamer.py
python3 docs/research/speaker-notes/inspect_pdfs.py > /tmp/notes-inspection.json
```

The standalone public-API probe source is retained as
[pdfium-probe.txt](research/speaker-notes/pdfium-probe.txt). With a locally built
`pdfium-render 0.9.1` rlib and its matching Rust compiler, reproduce it from the
repository root (substitute the actual rlib hash):

```sh
rustc --edition=2021 docs/research/speaker-notes/pdfium-probe.txt \
  --crate-name pdfium_probe \
  --extern pdfium_render=target/debug/deps/libpdfium_render-HASH.rlib \
  -L dependency=target/debug/deps -o /tmp/qp168-pdfium-probe
/tmp/qp168-pdfium-probe docs/research/speaker-notes/*.pdf
```

The probe uses the local macOS library path; adjust it for another platform.
This is a research program for these small authored samples, not a production
extractor or a tool for processing untrusted PDFs outside the renderer helper.

`beamer.tex` retains both native notes and the optional existing annotation
macro. Generated PDFs contain dates, so rebuilding need not reproduce their
hashes. `keynote-source.key` retains the three-slide Keynote source. Open it in
Keynote and export PDF with Include presenter notes off, then on, keeping the
other options listed above unchanged.

`google-source.pptx` is the editable export of the Google Slides source. Its
three notes-slide XML parts were checked: Alpha and Beta contain the distinct
notes; Gamma is empty. The PowerPoint follow-up used this unchanged source and
verified the notes again through print preview and actual PDF output. For Google
Slides, use a deck with that same content and compare its normal PDF download
against Print preview > 1 slide with notes > Download as PDF.

To reproduce the PowerPoint samples, open `google-source.pptx`, choose Print,
select all slides and the Slides layout, and save through PDF > Save as PDF.
Repeat with the Notes layout, retaining the settings recorded above. With an
activated app, also use File > Save As > PDF and File > Export > PDF, choosing
Best for printing and then Best for electronic distribution and accessibility
for each command. The latter uploads the source to Microsoft online services.
Keep any future Windows PDF/XPS results separate from these macOS paths, and
record version, OS, exporter, accessibility settings, and page
selections. An optional Impress run using the same source should compare
default export, `ExportNotesPages`, and `ExportOnlyNotesPages`; its similarly
named `ExportNotes` option must not be assumed to mean presenter notes.

Any implementation proposal needs a separate issue with an explicit format
contract, reliable mapping to clean audience pages, and handling for absent
notes. This research provides no basis for broadening automatic detection.

## Primary references

- Microsoft: [Print speaker notes](https://support.microsoft.com/en-us/powerpoint/print-speaker-notes),
  [Print your PowerPoint slides, handouts, or notes](https://support.microsoft.com/en-au/powerpoint/training/print-your-powerpoint-slides-handouts-or-notes),
  and [Save PowerPoint presentations as PDF files](https://support.microsoft.com/en-us/powerpoint/training/save-powerpoint-presentations-as-pdf-files).
  These document notes-page printing and the macOS PDF export limitation;
  they do not establish the internal dictionaries of an untested PDF.
- Apple: [Export to PowerPoint or another file format in Keynote on Mac](https://support.apple.com/en-gb/guide/keynote/tana0d19882a/mac).
  Presenter notes and comments are separate export options.
- Google: [Print a file - Computer](https://support.google.com/docs/answer/143346?co=GENIE.Platform%3DDesktop&hl=en).
  Documents the one-slide-with-notes printing path.
- Beamer: [User guide](https://tug.ctan.org/macros/latex/contrib/beamer/doc/beameruserguide.pdf),
  sections 19 and 22, and [native notes implementation](https://github.com/josephwright/beamer/blob/main/base/beamerbasenotes.sty).
  Describes hidden, interleaved, notes-only, and second-screen output.
- LibreOffice: [PDF command-line parameters](https://help.libreoffice.org/latest/en-US/text/shared/guide/pdf_params.html).
  Documents the separate notes-page and embedded-source export options.

References were consulted on 2026-10-07. Exporter details can change; the retained
samples define the bounds of the observed results.

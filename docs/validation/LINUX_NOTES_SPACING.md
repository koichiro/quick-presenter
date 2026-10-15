# Linux notes line spacing validation

Date: 2026-10-09 (JST). Environment: Ubuntu 26.04.1 LTS, GNOME Wayland,
x86_64, one 2560 x 1080 display at scale 1.0.

## Change and measurement

Linux now uses natural font line height (factor 1.0) for speaker notes instead
of applying the shared 1.25 multiplier. macOS and Windows retain factor 1.25.
The visible TextInput and all 12–24px measurement probes use one typography
property, initialized by Rust when creating the presenter window.

With the same 12px font, width, and 150 repeated mixed English/Japanese lines,
the final Linux GUI measured 2468px content height at factor 1.0 versus 3081px at
factor 1.25, including 10px bottom padding. This is approximately 20% less
vertical space. The regression check also verifies that visible content height
matches the measurement probe and returns to the original height after toggling
the factor back. This compares line spacing without changing font size.

The earlier GUI smoke failure required an intermediate font size for eight
Japanese lines in the minimum presenter layout. With natural line height,
the largest fitting intermediate size is now selected, and the assertion passes
without relaxing its requirements.

## Results

- `cargo fmt --check`: passed.
- `cargo check --locked -j 2`: passed.
- `cargo test --locked -j 2` with the matching installed PDFium: 478 passed.
- Native Wayland GUI smoke: 70 passed, zero failed.
- XWayland GUI smoke in the same physical desktop session: 70 passed,
  zero failed. This is not a native Xorg session.
- At an actual 800 x 560 presenter size, before/after screenshots of
  `long-speaker-notes.pdf` show tighter English line spacing at the unchanged
  minimum font size. `mixed-speaker-notes.pdf` is also inspected for Japanese
  text, wrapping, and explicit paragraph breaks.
- Both GUI suites cover 24px short notes, 12px scrolling overflow, Japanese
  multiline fitting, scroll clamping after content changes, resizing, portrait
  notes layout, and preservation of presentation state during typography changes.

## Compact blank lines

After the initial natural-line-height change, the user confirmed improved line
spacing but found blank lines too large. Linux now renders blank lines as
paragraph gaps of half the font size. Single line breaks and indentation stay
inside their paragraphs; repeated blank lines retain repeated compact gaps.
Rust prepares only the display paragraphs. PDF notes and CLI source queries
retain their original text, and other platforms use a single original-text
block with their existing line spacing.

At 12px, mixed English/Japanese sizing probes measured 33px for two normal
lines, 40px with one blank line, and 46px with two blank lines. Each separator
uses 6px spacing; independently rounded paragraph heights explain the 1px
difference in the first comparison. Both GUI backends verify these gaps and
that overflowing paragraph notes match visible/probe heights and remain
scrollable. Unit tests cover single newlines, indentation, Unicode, CRLF,
whitespace-only blank lines, repeated separators, and empty text.

Before/after inspection at 800 x 560 with `long-speaker-notes.pdf` confirms
smaller paragraph gaps. The blank-line comparison baseline is the initial
line-height-only fix at `9fcc0e3`.

The baseline is the debug package built from `5241c1b` on the `v1.9.0` branch.
That development branch was subsequently renamed to `v2.0.0`; the revision
and package used for this historical validation are unchanged.
Both before/after manual windows use the same Linux desktop, font environment,
size, and test PDF. Configuration/state are isolated in temporary directories.
Raw reports and screenshots remain in `/tmp`, rather than in the repository.

## Short-note alignment

The notes box is anchored at `(0, 0)` inside the scroll content. The paragraph
layout explicitly starts at the top, and each text block uses top-left alignment.
This prevents a short paragraph from being centered in the
scroll viewport when its content is shorter than the available height.
Before/after inspection of page 2 in `long-speaker-notes.pdf` checks the short
Japanese note at the same 800 x 560 presenter size. Page 1 and both GUI suites
also check that long-note scrolling and compact paragraph gaps remain intact.
The short-note GUI check also verifies the notes box stays at the top of the
viewport while fitting at 24px.

The final full Rust run passes 478 tests with `--test-threads=1`; the initial
parallel run hit an unrelated transient `Text file busy` error while launching
the CLI fixture, and that suite passes when retried.

No macOS or Windows desktop is available in this run, so visual equivalence to
macOS is not claimed. Their existing line-height value is preserved. This
change does not remove intentional blank lines from PDF source annotations or impose
identical font metrics across operating systems.

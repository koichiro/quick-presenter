use std::collections::BTreeMap;

/// Readability bounds in logical pixels, independent of platform theme defaults.
pub const MIN_NOTES_FONT_SIZE: u16 = 12;
pub const MAX_NOTES_FONT_SIZE: u16 = 24;
pub const NOTES_BOTTOM_PADDING: f32 = 10.0;

/// Select the largest measured size that fits, falling back to scrolling at 12px.
/// Heights must cover every whole-pixel size from 12px through 24px in order.
pub fn fit_notes_font_size(
    has_notes: bool,
    viewport_width: f32,
    viewport_height: f32,
    measured_heights: &[f32],
) -> u16 {
    let expected_count = usize::from(MAX_NOTES_FONT_SIZE - MIN_NOTES_FONT_SIZE + 1);
    if !has_notes
        || !viewport_width.is_finite()
        || viewport_width <= 0.0
        || !viewport_height.is_finite()
        || viewport_height <= 0.0
        || measured_heights.len() != expected_count
        || measured_heights
            .iter()
            .any(|height| !height.is_finite() || *height <= 0.0)
    {
        return MIN_NOTES_FONT_SIZE;
    }

    (MIN_NOTES_FONT_SIZE..=MAX_NOTES_FONT_SIZE)
        .zip(measured_heights)
        .rev()
        .find(|(_, height)| **height + NOTES_BOTTOM_PADDING <= viewport_height)
        .map_or(MIN_NOTES_FONT_SIZE, |(size, _)| size)
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SpeakerNotes {
    pages: BTreeMap<u32, String>,
}

impl SpeakerNotes {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_page_notes(notes: impl IntoIterator<Item = (u32, String)>) -> Self {
        let mut pages: BTreeMap<u32, Vec<String>> = BTreeMap::new();

        for (page_number, note) in notes {
            let note = note.trim();
            if page_number == 0 || note.is_empty() {
                continue;
            }

            pages.entry(page_number).or_default().push(note.to_owned());
        }

        Self {
            pages: pages
                .into_iter()
                .map(|(page_number, notes)| (page_number, notes.join("\n\n")))
                .collect(),
        }
    }

    pub fn note_for_page_number(&self, page_number: u32) -> Option<&str> {
        self.pages.get(&page_number).map(String::as_str)
    }

    pub fn note_for_page_index(&self, page_index: u32) -> Option<&str> {
        self.note_for_page_number(page_index + 1)
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }
}

/// Metadata exposed by pdfium-render without accessing private native handles.
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfNoteMetadata<'a> {
    /// PDF `/Rect`, ordered as left, bottom, right, top.
    pub bounds: Option<[f32; 4]>,
    /// PDF `/C` and `/CA`, converted by PDFium to RGBA bytes.
    pub color: Option<[u8; 4]>,
    /// PDF `/T` (annotation author), not document creator or `/NM`.
    pub author: Option<&'a str>,
}

/// Recognize only the supported generators' annotation fingerprints.
///
/// Callers must first require a Text annotation. `/Name /Note` is an icon choice,
/// not a speaker-note marker; pdfium-render's `name()` reads `/NM` instead.
/// See docs/NOTES.md for the compatibility rules and their limitations.
pub fn is_pdf_speaker_note_annotation(
    metadata: PdfNoteMetadata<'_>,
    contents: Option<&str>,
) -> bool {
    if !contents.is_some_and(|contents| !contents.trim().is_empty()) {
        return false;
    }

    let marp = metadata.bounds == Some([0.0, 20.0, 20.0, 20.0])
        && metadata.color == Some([255, 234, 107, 63]);
    let beamer = metadata.author == Some("Quick Presenter")
        && metadata.color.is_some_and(|color| color[3] == 0);

    marp || beamer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_font_size_uses_largest_fitting_measurement_including_padding() {
        let heights = [
            20.0, 22.0, 24.0, 26.0, 28.0, 30.0, 32.0, 34.0, 36.0, 38.0, 40.0, 42.0, 44.0,
        ];
        assert_eq!(fit_notes_font_size(true, 300.0, 100.0, &heights), 24);
        assert_eq!(fit_notes_font_size(true, 300.0, 40.0, &heights), 17);
        assert_eq!(fit_notes_font_size(true, 300.0, 39.9, &heights), 16);
        assert_eq!(fit_notes_font_size(true, 300.0, 30.0, &heights), 12);
    }

    #[test]
    fn overflowing_notes_keep_the_minimum_readable_size() {
        assert_eq!(fit_notes_font_size(true, 300.0, 200.0, &[1000.0; 13]), 12);
    }

    #[test]
    fn no_notes_uses_fixed_placeholder_size() {
        assert_eq!(fit_notes_font_size(false, 300.0, 200.0, &[20.0; 13]), 12);
    }

    #[test]
    fn notes_font_size_rejects_incomplete_or_invalid_layout() {
        assert_eq!(fit_notes_font_size(true, 300.0, 200.0, &[]), 12);
        assert_eq!(fit_notes_font_size(true, 300.0, 200.0, &[20.0; 12]), 12);
        assert_eq!(fit_notes_font_size(true, 300.0, 200.0, &[20.0; 14]), 12);
        for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(fit_notes_font_size(true, invalid, 200.0, &[20.0; 13]), 12);
            assert_eq!(fit_notes_font_size(true, 300.0, invalid, &[20.0; 13]), 12);
            let mut heights = [20.0; 13];
            heights[6] = invalid;
            assert_eq!(fit_notes_font_size(true, 300.0, 200.0, &heights), 12);
        }
    }

    #[test]
    fn fitting_uses_all_measurements_without_assuming_monotonic_heights() {
        let mut heights = [200.0; 13];
        heights[4] = 80.0;
        heights[8] = 70.0;
        assert_eq!(fit_notes_font_size(true, 300.0, 100.0, &heights), 20);
    }

    #[test]
    fn empty_notes_return_none() {
        let notes = SpeakerNotes::empty();

        assert_eq!(notes.note_for_page_number(1), None);
        assert_eq!(notes.note_for_page_index(0), None);
        assert!(notes.is_empty());
    }

    #[test]
    fn page_number_lookup_is_one_based() {
        let notes = SpeakerNotes::from_page_notes([(1, "Opening".to_owned())]);

        assert_eq!(notes.note_for_page_number(1), Some("Opening"));
        assert_eq!(notes.note_for_page_number(0), None);
    }

    #[test]
    fn page_index_lookup_maps_zero_to_page_one() {
        let notes =
            SpeakerNotes::from_page_notes([(1, "Opening".to_owned()), (2, "Second".to_owned())]);

        assert_eq!(notes.note_for_page_index(0), Some("Opening"));
        assert_eq!(notes.note_for_page_index(1), Some("Second"));
    }

    #[test]
    fn empty_notes_and_page_zero_are_ignored() {
        let notes = SpeakerNotes::from_page_notes([
            (0, "Invalid".to_owned()),
            (1, "   ".to_owned()),
            (2, "Valid".to_owned()),
        ]);

        assert_eq!(notes.note_for_page_number(0), None);
        assert_eq!(notes.note_for_page_number(1), None);
        assert_eq!(notes.note_for_page_number(2), Some("Valid"));
    }

    #[test]
    fn multiple_notes_on_same_page_are_joined() {
        let notes = SpeakerNotes::from_page_notes([
            (3, "First note".to_owned()),
            (3, "Second note".to_owned()),
        ]);

        assert_eq!(
            notes.note_for_page_number(3),
            Some("First note\n\nSecond note")
        );
    }

    #[test]
    fn notes_are_trimmed() {
        let notes = SpeakerNotes::from_page_notes([(1, "\n  Opening remarks  \n".to_owned())]);

        assert_eq!(notes.note_for_page_number(1), Some("Opening remarks"));
    }

    #[test]
    fn pdf_speaker_note_annotation_requires_contents() {
        assert!(is_pdf_speaker_note_annotation(
            marp_metadata(),
            Some("Speaker note")
        ));
        assert!(!is_pdf_speaker_note_annotation(
            marp_metadata(),
            Some("   \n")
        ));
        assert!(!is_pdf_speaker_note_annotation(marp_metadata(), None));
    }

    #[test]
    fn pdf_speaker_note_annotation_has_no_unmarked_fallback() {
        assert!(!is_pdf_speaker_note_annotation(
            PdfNoteMetadata::default(),
            Some("Review comment")
        ));
    }

    #[test]
    fn marp_notes_require_both_geometry_and_color() {
        for bounds in [
            None,
            Some([0.0, 0.0, 20.0, 20.0]),
            Some([0.0, 20.0, 20.0, 21.0]),
        ] {
            assert!(!is_pdf_speaker_note_annotation(
                PdfNoteMetadata {
                    bounds,
                    ..marp_metadata()
                },
                Some("Review comment")
            ));
        }
        for color in [None, Some([255, 234, 107, 255]), Some([255, 255, 0, 63])] {
            assert!(!is_pdf_speaker_note_annotation(
                PdfNoteMetadata {
                    color,
                    ..marp_metadata()
                },
                Some("Review comment")
            ));
        }
    }

    #[test]
    fn beamer_notes_require_the_documented_author_and_transparency() {
        let metadata = PdfNoteMetadata {
            author: Some("Quick Presenter"),
            color: Some([0, 0, 255, 0]),
            bounds: Some([142.226, 125.624, 155.776, 139.173]),
        };
        assert!(is_pdf_speaker_note_annotation(
            metadata,
            Some("Beamer note")
        ));
        for author in [None, Some("Reviewer"), Some("quick presenter")] {
            assert!(!is_pdf_speaker_note_annotation(
                PdfNoteMetadata { author, ..metadata },
                Some("Review comment")
            ));
        }
        for color in [None, Some([0, 0, 255, 255])] {
            assert!(!is_pdf_speaker_note_annotation(
                PdfNoteMetadata { color, ..metadata },
                Some("Review comment")
            ));
        }
    }

    fn marp_metadata() -> PdfNoteMetadata<'static> {
        PdfNoteMetadata {
            bounds: Some([0.0, 20.0, 20.0, 20.0]),
            color: Some([255, 234, 107, 63]),
            author: None,
        }
    }
}

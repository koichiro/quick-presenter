use std::collections::BTreeMap;

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

pub fn is_pdf_speaker_note_annotation(name: Option<&str>, contents: Option<&str>) -> bool {
    let has_note_name = name == Some("Note");
    let has_contents = contents.is_some_and(|contents| !contents.trim().is_empty());

    has_note_name && has_contents
}

#[cfg(test)]
mod tests {
    use super::*;

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
            Some("Note"),
            Some("Speaker note")
        ));
        assert!(!is_pdf_speaker_note_annotation(Some("Note"), Some("   ")));
        assert!(!is_pdf_speaker_note_annotation(Some("Note"), None));
    }

    #[test]
    fn pdf_speaker_note_annotation_rejects_missing_name() {
        assert!(!is_pdf_speaker_note_annotation(None, Some("Speaker note")));
    }

    #[test]
    fn pdf_speaker_note_annotation_rejects_other_names() {
        assert!(!is_pdf_speaker_note_annotation(
            Some("Comment"),
            Some("Speaker note")
        ));
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PresentationState {
    document: Option<PresentationDocument>,
}

impl PresentationState {
    pub fn empty() -> Self {
        Self { document: None }
    }

    pub fn open_document(title: impl Into<String>, total_pages: u32) -> Self {
        if total_pages == 0 {
            return Self::empty();
        }

        Self {
            document: Some(PresentationDocument {
                title: title.into(),
                pages: PageCursor::new(total_pages),
            }),
        }
    }

    pub fn next_page(&mut self) {
        if let Some(document) = self.document.as_mut() {
            document.pages.next();
        }
    }

    pub fn previous_page(&mut self) {
        if let Some(document) = self.document.as_mut() {
            document.pages.previous();
        }
    }

    pub fn first_page(&mut self) {
        if let Some(document) = self.document.as_mut() {
            document.pages.first();
        }
    }

    pub fn last_page(&mut self) {
        if let Some(document) = self.document.as_mut() {
            document.pages.last();
        }
    }

    pub fn snapshot(&self) -> Option<PageSnapshot> {
        self.document.as_ref().map(PresentationDocument::snapshot)
    }
}

impl Default for PresentationState {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PresentationDocument {
    title: String,
    pages: PageCursor,
}

impl PresentationDocument {
    fn snapshot(&self) -> PageSnapshot {
        PageSnapshot {
            title: self.title.clone(),
            current_index: self.pages.current_index(),
            current_number: self.pages.current_number(),
            total_pages: self.pages.total_pages(),
            next_index: self.pages.next_index(),
            page_label: self.pages.label(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PageSnapshot {
    pub title: String,
    pub current_index: u32,
    pub current_number: u32,
    pub total_pages: u32,
    pub next_index: Option<u32>,
    pub page_label: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct PageCursor {
    current_index: u32,
    total_pages: u32,
}

impl PageCursor {
    fn new(total_pages: u32) -> Self {
        Self {
            current_index: 0,
            total_pages,
        }
    }

    fn next(&mut self) {
        if self.current_index < self.last_index() {
            self.current_index += 1;
        }
    }

    fn previous(&mut self) {
        if self.current_index > 0 {
            self.current_index -= 1;
        }
    }

    fn first(&mut self) {
        self.current_index = 0;
    }

    fn last(&mut self) {
        self.current_index = self.last_index();
    }

    fn current_index(&self) -> u32 {
        self.current_index
    }

    fn current_number(&self) -> u32 {
        self.current_index + 1
    }

    fn total_pages(&self) -> u32 {
        self.total_pages
    }

    fn next_index(&self) -> Option<u32> {
        if self.current_index < self.last_index() {
            Some(self.current_index + 1)
        } else {
            None
        }
    }

    fn label(&self) -> String {
        format!("{} / {}", self.current_number(), self.total_pages)
    }

    fn last_index(&self) -> u32 {
        self.total_pages.saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_has_no_snapshot() {
        let state = PresentationState::empty();

        assert_eq!(state.snapshot(), None);
    }

    #[test]
    fn zero_page_document_becomes_empty_state() {
        let state = PresentationState::open_document("Empty", 0);

        assert_eq!(state.snapshot(), None);
    }

    #[test]
    fn opened_document_starts_on_first_page() {
        let state = PresentationState::open_document("Deck", 3);

        assert_eq!(
            state.snapshot(),
            Some(PageSnapshot {
                title: "Deck".to_owned(),
                current_index: 0,
                current_number: 1,
                total_pages: 3,
                next_index: Some(1),
                page_label: "1 / 3".to_owned(),
            })
        );
    }

    #[test]
    fn next_page_advances_until_last_page() {
        let mut state = PresentationState::open_document("Deck", 3);

        state.next_page();
        assert_eq!(state.snapshot().unwrap().page_label, "2 / 3");
        assert_eq!(state.snapshot().unwrap().next_index, Some(2));

        state.next_page();
        assert_eq!(state.snapshot().unwrap().page_label, "3 / 3");
        assert_eq!(state.snapshot().unwrap().next_index, None);

        state.next_page();
        assert_eq!(state.snapshot().unwrap().page_label, "3 / 3");
    }

    #[test]
    fn previous_page_moves_back_until_first_page() {
        let mut state = PresentationState::open_document("Deck", 3);
        state.next_page();
        state.next_page();

        state.previous_page();
        assert_eq!(state.snapshot().unwrap().page_label, "2 / 3");

        state.previous_page();
        assert_eq!(state.snapshot().unwrap().page_label, "1 / 3");

        state.previous_page();
        assert_eq!(state.snapshot().unwrap().page_label, "1 / 3");
    }

    #[test]
    fn single_page_document_has_no_next_page() {
        let mut state = PresentationState::open_document("Deck", 1);

        assert_eq!(state.snapshot().unwrap().next_index, None);

        state.next_page();
        state.previous_page();

        assert_eq!(state.snapshot().unwrap().page_label, "1 / 1");
        assert_eq!(state.snapshot().unwrap().next_index, None);
    }

    #[test]
    fn first_page_jumps_back_to_first_page() {
        let mut state = PresentationState::open_document("Deck", 4);
        state.next_page();
        state.next_page();

        state.first_page();

        assert_eq!(state.snapshot().unwrap().current_index, 0);
        assert_eq!(state.snapshot().unwrap().page_label, "1 / 4");
    }

    #[test]
    fn last_page_jumps_to_final_page() {
        let mut state = PresentationState::open_document("Deck", 4);

        state.last_page();

        assert_eq!(state.snapshot().unwrap().current_index, 3);
        assert_eq!(state.snapshot().unwrap().page_label, "4 / 4");
        assert_eq!(state.snapshot().unwrap().next_index, None);
    }

    #[test]
    fn page_jumps_are_safe_for_single_page_document() {
        let mut state = PresentationState::open_document("Deck", 1);

        state.last_page();
        assert_eq!(state.snapshot().unwrap().page_label, "1 / 1");

        state.first_page();
        assert_eq!(state.snapshot().unwrap().page_label, "1 / 1");
    }

    #[test]
    fn navigation_on_empty_state_is_a_noop() {
        let mut state = PresentationState::empty();

        state.next_page();
        state.previous_page();
        state.first_page();
        state.last_page();

        assert_eq!(state.snapshot(), None);
    }
}

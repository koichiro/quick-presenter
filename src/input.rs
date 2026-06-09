use crate::presentation::PresentationState;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PresentationCommand {
    NextPage,
    PreviousPage,
    ExitSlideFullscreen,
}

pub fn apply_presentation_command(
    presentation: &mut PresentationState,
    command: PresentationCommand,
) {
    match command {
        PresentationCommand::NextPage => presentation.next_page(),
        PresentationCommand::PreviousPage => presentation.previous_page(),
        PresentationCommand::ExitSlideFullscreen => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_page_command_advances_until_last_page() {
        let mut presentation = PresentationState::open_document("Deck", 2);

        apply_presentation_command(&mut presentation, PresentationCommand::NextPage);
        assert_eq!(presentation.snapshot().unwrap().page_label, "2 / 2");

        apply_presentation_command(&mut presentation, PresentationCommand::NextPage);
        assert_eq!(presentation.snapshot().unwrap().page_label, "2 / 2");
    }

    #[test]
    fn previous_page_command_moves_back_until_first_page() {
        let mut presentation = PresentationState::open_document("Deck", 2);
        apply_presentation_command(&mut presentation, PresentationCommand::NextPage);

        apply_presentation_command(&mut presentation, PresentationCommand::PreviousPage);
        assert_eq!(presentation.snapshot().unwrap().page_label, "1 / 2");

        apply_presentation_command(&mut presentation, PresentationCommand::PreviousPage);
        assert_eq!(presentation.snapshot().unwrap().page_label, "1 / 2");
    }

    #[test]
    fn commands_on_empty_presentation_are_noops() {
        let mut presentation = PresentationState::empty();

        apply_presentation_command(&mut presentation, PresentationCommand::NextPage);
        apply_presentation_command(&mut presentation, PresentationCommand::PreviousPage);
        apply_presentation_command(&mut presentation, PresentationCommand::ExitSlideFullscreen);

        assert_eq!(presentation.snapshot(), None);
    }

    #[test]
    fn exit_fullscreen_command_does_not_change_pages() {
        let mut presentation = PresentationState::open_document("Deck", 2);
        apply_presentation_command(&mut presentation, PresentationCommand::NextPage);

        apply_presentation_command(&mut presentation, PresentationCommand::ExitSlideFullscreen);

        assert_eq!(presentation.snapshot().unwrap().page_label, "2 / 2");
    }
}

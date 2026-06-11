use crate::presentation::PresentationState;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PresentationCommand {
    NextPage,
    PreviousPage,
    FirstPage,
    LastPage,
    ExitSlideFullscreen,
    ToggleBlackScreen,
}

pub fn apply_presentation_command(
    presentation: &mut PresentationState,
    command: PresentationCommand,
) {
    match command {
        PresentationCommand::NextPage => presentation.next_page(),
        PresentationCommand::PreviousPage => presentation.previous_page(),
        PresentationCommand::FirstPage => presentation.first_page(),
        PresentationCommand::LastPage => presentation.last_page(),
        PresentationCommand::ExitSlideFullscreen | PresentationCommand::ToggleBlackScreen => {}
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
    fn first_page_command_jumps_back_to_first_page() {
        let mut presentation = PresentationState::open_document("Deck", 3);
        apply_presentation_command(&mut presentation, PresentationCommand::LastPage);

        apply_presentation_command(&mut presentation, PresentationCommand::FirstPage);

        assert_eq!(presentation.snapshot().unwrap().page_label, "1 / 3");
    }

    #[test]
    fn last_page_command_jumps_to_final_page() {
        let mut presentation = PresentationState::open_document("Deck", 3);

        apply_presentation_command(&mut presentation, PresentationCommand::LastPage);

        assert_eq!(presentation.snapshot().unwrap().page_label, "3 / 3");
        assert_eq!(presentation.snapshot().unwrap().next_index, None);
    }

    #[test]
    fn commands_on_empty_presentation_are_noops() {
        let mut presentation = PresentationState::empty();

        apply_presentation_command(&mut presentation, PresentationCommand::NextPage);
        apply_presentation_command(&mut presentation, PresentationCommand::PreviousPage);
        apply_presentation_command(&mut presentation, PresentationCommand::FirstPage);
        apply_presentation_command(&mut presentation, PresentationCommand::LastPage);
        apply_presentation_command(&mut presentation, PresentationCommand::ExitSlideFullscreen);
        apply_presentation_command(&mut presentation, PresentationCommand::ToggleBlackScreen);

        assert_eq!(presentation.snapshot(), None);
    }

    #[test]
    fn exit_fullscreen_command_does_not_change_pages() {
        let mut presentation = PresentationState::open_document("Deck", 2);
        apply_presentation_command(&mut presentation, PresentationCommand::NextPage);

        apply_presentation_command(&mut presentation, PresentationCommand::ExitSlideFullscreen);

        assert_eq!(presentation.snapshot().unwrap().page_label, "2 / 2");
    }

    #[test]
    fn toggle_black_screen_command_does_not_change_pages() {
        let mut presentation = PresentationState::open_document("Deck", 2);
        apply_presentation_command(&mut presentation, PresentationCommand::NextPage);

        apply_presentation_command(&mut presentation, PresentationCommand::ToggleBlackScreen);

        assert_eq!(presentation.snapshot().unwrap().page_label, "2 / 2");
    }
}

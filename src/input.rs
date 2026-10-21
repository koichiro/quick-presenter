use crate::presentation::PresentationState;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PresentationCommand {
    NextPage,
    PreviousPage,
    FirstPage,
    LastPage,
    JumpToPage(u32),
    ExitSlideFullscreen,
    ToggleBlackScreen,
    SetBlackScreen(bool),
    Close,
}

/// Unmodified navigation takes precedence over the read-only notes cursor.
/// Keep selection/copy shortcuts and modified cursor movement in TextInput.
pub fn notes_navigation_command(
    event: &slint::language::KeyEvent,
    physical_control_is_meta: bool,
) -> Option<PresentationCommand> {
    use slint::platform::Key;
    let modifiers = event.modifiers;
    if modifiers.shift || modifiers.alt {
        return None;
    }
    if modifiers.control || modifiers.meta {
        let control_only = if physical_control_is_meta {
            modifiers.meta && !modifiers.control
        } else {
            modifiers.control && !modifiers.meta
        };
        return (control_only && event.text.eq_ignore_ascii_case("e"))
            .then_some(PresentationCommand::LastPage);
    }
    let key = event.text.chars().next()?;
    match key {
        ' ' => Some(PresentationCommand::NextPage),
        key if [Key::Return, Key::RightArrow, Key::DownArrow, Key::PageDown]
            .into_iter()
            .any(|candidate| key == char::from(candidate)) =>
        {
            Some(PresentationCommand::NextPage)
        }
        key if [Key::LeftArrow, Key::UpArrow, Key::PageUp, Key::Backspace]
            .into_iter()
            .any(|candidate| key == char::from(candidate)) =>
        {
            Some(PresentationCommand::PreviousPage)
        }
        key if key == char::from(Key::Home) => Some(PresentationCommand::FirstPage),
        key if key == char::from(Key::End) => Some(PresentationCommand::LastPage),
        _ => None,
    }
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
        PresentationCommand::JumpToPage(page_index) => presentation.jump_to_page_index(page_index),
        PresentationCommand::ExitSlideFullscreen
        | PresentationCommand::ToggleBlackScreen
        | PresentationCommand::SetBlackScreen(_)
        | PresentationCommand::Close => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_cursor_keys_route_to_presentation_navigation() {
        use slint::platform::Key;
        for (key, expected) in [
            (Key::Return, PresentationCommand::NextPage),
            (Key::RightArrow, PresentationCommand::NextPage),
            (Key::DownArrow, PresentationCommand::NextPage),
            (Key::PageDown, PresentationCommand::NextPage),
            (Key::LeftArrow, PresentationCommand::PreviousPage),
            (Key::UpArrow, PresentationCommand::PreviousPage),
            (Key::PageUp, PresentationCommand::PreviousPage),
            (Key::Backspace, PresentationCommand::PreviousPage),
            (Key::Home, PresentationCommand::FirstPage),
            (Key::End, PresentationCommand::LastPage),
        ] {
            let mut event = slint::language::KeyEvent {
                text: key.into(),
                ..Default::default()
            };
            assert_eq!(notes_navigation_command(&event, false), Some(expected));
            event.modifiers.shift = true;
            assert_eq!(notes_navigation_command(&event, false), None);
        }
    }

    #[test]
    fn notes_preserve_selection_copy_and_modified_cursor_movement() {
        for text in ["a", "c", "x", "v", "\u{f703}"] {
            let mut event = slint::language::KeyEvent {
                text: text.into(),
                ..Default::default()
            };
            event.modifiers.control = true;
            assert_eq!(notes_navigation_command(&event, false), None);
            event.modifiers.control = false;
            event.modifiers.meta = true;
            assert_eq!(notes_navigation_command(&event, true), None);
        }
        let mut event = slint::language::KeyEvent {
            text: "e".into(),
            ..Default::default()
        };
        event.modifiers.control = true;
        assert_eq!(
            notes_navigation_command(&event, false),
            Some(PresentationCommand::LastPage)
        );
        assert_eq!(notes_navigation_command(&event, true), None);
        event.modifiers.control = false;
        event.modifiers.meta = true;
        assert_eq!(
            notes_navigation_command(&event, true),
            Some(PresentationCommand::LastPage)
        );
        event.modifiers.alt = true;
        assert_eq!(notes_navigation_command(&event, true), None);
    }

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
    fn jump_to_page_command_moves_to_requested_page() {
        let mut presentation = PresentationState::open_document("Deck", 3);

        apply_presentation_command(&mut presentation, PresentationCommand::JumpToPage(1));

        assert_eq!(presentation.snapshot().unwrap().page_label, "2 / 3");
    }

    #[test]
    fn jump_to_page_command_clamps_to_last_page() {
        let mut presentation = PresentationState::open_document("Deck", 3);

        apply_presentation_command(&mut presentation, PresentationCommand::JumpToPage(99));

        assert_eq!(presentation.snapshot().unwrap().page_label, "3 / 3");
    }

    #[test]
    fn commands_on_empty_presentation_are_noops() {
        let mut presentation = PresentationState::empty();

        apply_presentation_command(&mut presentation, PresentationCommand::NextPage);
        apply_presentation_command(&mut presentation, PresentationCommand::PreviousPage);
        apply_presentation_command(&mut presentation, PresentationCommand::FirstPage);
        apply_presentation_command(&mut presentation, PresentationCommand::LastPage);
        apply_presentation_command(&mut presentation, PresentationCommand::JumpToPage(3));
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

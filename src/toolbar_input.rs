use slint::{language::KeyEvent, platform::Key};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolbarShortcut {
    Next,
    Previous,
    First,
    Last,
    ExitFullscreen,
    ToggleFullscreen,
    Blackout,
    SwapDisplays,
}

/// Space/Return activate the focused button; other presentation keys take
/// precedence over native button handling. This applies only to the toolbar.
pub fn toolbar_shortcut(
    event: &KeyEvent,
    physical_control_is_meta: bool,
) -> Option<ToolbarShortcut> {
    use ToolbarShortcut::*;
    let modifiers = event.modifiers;
    if modifiers.shift || modifiers.alt {
        return None;
    }
    if modifiers.control || modifiers.meta {
        let physical_control_only = if physical_control_is_meta {
            modifiers.meta && !modifiers.control
        } else {
            modifiers.control && !modifiers.meta
        };
        return if physical_control_only && event.text.eq_ignore_ascii_case("a") {
            Some(First)
        } else if physical_control_only && event.text.eq_ignore_ascii_case("e") {
            Some(Last)
        } else {
            None
        };
    }
    let mut characters = event.text.chars();
    let key = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    if [Key::RightArrow, Key::DownArrow, Key::PageDown]
        .into_iter()
        .any(|k| key == char::from(k))
    {
        Some(Next)
    } else if [Key::LeftArrow, Key::UpArrow, Key::PageUp, Key::Backspace]
        .into_iter()
        .any(|k| key == char::from(k))
    {
        Some(Previous)
    } else if key == char::from(Key::Home) {
        Some(First)
    } else if key == char::from(Key::End) {
        Some(Last)
    } else if key == char::from(Key::Escape) {
        Some(ExitFullscreen)
    } else if key == char::from(Key::F5) || key.eq_ignore_ascii_case(&'f') {
        Some(ToggleFullscreen)
    } else if key.eq_ignore_ascii_case(&'b') {
        Some(Blackout)
    } else if key.eq_ignore_ascii_case(&'x') {
        Some(SwapDisplays)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(text: impl Into<slint::SharedString>) -> KeyEvent {
        let mut event = KeyEvent::default();
        event.text = text.into();
        event
    }

    #[test]
    fn toolbar_preserves_button_activation_and_tab_traversal() {
        let keys: [slint::SharedString; 5] = [
            " ".into(),
            Key::Return.into(),
            Key::Tab.into(),
            Key::Backtab.into(),
            "bc".into(),
        ];
        for text in keys {
            assert_eq!(toolbar_shortcut(&event(text), false), None);
        }
    }

    #[test]
    fn presentation_keys_override_toolbar_controls() {
        use ToolbarShortcut::*;
        for (key, expected) in [
            (Key::RightArrow, Next),
            (Key::DownArrow, Next),
            (Key::PageDown, Next),
            (Key::LeftArrow, Previous),
            (Key::UpArrow, Previous),
            (Key::PageUp, Previous),
            (Key::Backspace, Previous),
            (Key::Home, First),
            (Key::End, Last),
            (Key::Escape, ExitFullscreen),
            (Key::F5, ToggleFullscreen),
        ] {
            assert_eq!(
                toolbar_shortcut(&event(slint::SharedString::from(key)), false),
                Some(expected)
            );
        }
        for (key, expected) in [
            ("b", Blackout),
            ("f", ToggleFullscreen),
            ("x", SwapDisplays),
        ] {
            assert_eq!(toolbar_shortcut(&event(key), false), Some(expected));
        }
    }

    #[test]
    fn physical_control_mapping_preserves_command_shortcuts_and_modified_keys() {
        for physical_control_is_meta in [false, true] {
            for (key, expected) in [("a", ToolbarShortcut::First), ("e", ToolbarShortcut::Last)] {
                let mut input = event(key);
                input.modifiers.control = !physical_control_is_meta;
                input.modifiers.meta = physical_control_is_meta;
                assert_eq!(
                    toolbar_shortcut(&input, physical_control_is_meta),
                    Some(expected)
                );
                input.modifiers.alt = true;
                assert_eq!(toolbar_shortcut(&input, physical_control_is_meta), None);
                input.modifiers.alt = false;
                input.modifiers.shift = true;
                assert_eq!(toolbar_shortcut(&input, physical_control_is_meta), None);
            }
            let mut command = event("a");
            command.modifiers.control = physical_control_is_meta;
            command.modifiers.meta = !physical_control_is_meta;
            assert_eq!(toolbar_shortcut(&command, physical_control_is_meta), None);
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WindowMenuState {
    presenter_visible: bool,
    slide_visible: bool,
}

impl Default for WindowMenuState {
    fn default() -> Self {
        Self {
            presenter_visible: true,
            slide_visible: true,
        }
    }
}

impl WindowMenuState {
    pub fn presenter_visible(&self) -> bool {
        self.presenter_visible
    }

    pub fn slide_visible(&self) -> bool {
        self.slide_visible
    }

    pub fn set_presenter_visible(&mut self, visible: bool) {
        self.presenter_visible = visible;
    }

    pub fn set_slide_visible(&mut self, visible: bool) {
        self.slide_visible = visible;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_menu_state_starts_with_both_windows_visible() {
        let state = WindowMenuState::default();

        assert!(state.presenter_visible());
        assert!(state.slide_visible());
    }

    #[test]
    fn presenter_visibility_can_be_changed() {
        let mut state = WindowMenuState::default();

        state.set_presenter_visible(false);

        assert!(!state.presenter_visible());
        assert!(state.slide_visible());
    }

    #[test]
    fn slide_visibility_can_be_changed() {
        let mut state = WindowMenuState::default();

        state.set_slide_visible(false);

        assert!(state.presenter_visible());
        assert!(!state.slide_visible());
    }
}

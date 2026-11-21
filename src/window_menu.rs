#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WindowMenuState {
    presenter_visible: bool,
    audience_visible: bool,
}

impl Default for WindowMenuState {
    fn default() -> Self {
        Self {
            presenter_visible: true,
            audience_visible: true,
        }
    }
}

impl WindowMenuState {
    pub fn presenter_visible(&self) -> bool {
        self.presenter_visible
    }

    pub fn audience_visible(&self) -> bool {
        self.audience_visible
    }

    pub fn set_presenter_visible(&mut self, visible: bool) {
        self.presenter_visible = visible;
    }

    pub fn set_audience_visible(&mut self, visible: bool) {
        self.audience_visible = visible;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_menu_state_starts_with_both_windows_visible() {
        let state = WindowMenuState::default();

        assert!(state.presenter_visible());
        assert!(state.audience_visible());
    }

    #[test]
    fn presenter_visibility_can_be_changed() {
        let mut state = WindowMenuState::default();

        state.set_presenter_visible(false);

        assert!(!state.presenter_visible());
        assert!(state.audience_visible());
    }

    #[test]
    fn audience_visibility_can_be_changed() {
        let mut state = WindowMenuState::default();

        state.set_audience_visible(false);

        assert!(state.presenter_visible());
        assert!(!state.audience_visible());
    }
}

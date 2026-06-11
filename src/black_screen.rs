#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct BlackScreenState {
    active: bool,
}

impl BlackScreenState {
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn toggle(&mut self) -> bool {
        self.set_active(!self.active)
    }

    pub fn set_active(&mut self, active: bool) -> bool {
        self.active = active;
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_inactive() {
        let state = BlackScreenState::default();

        assert!(!state.is_active());
    }

    #[test]
    fn toggle_enables_and_disables() {
        let mut state = BlackScreenState::default();

        assert!(state.toggle());
        assert!(state.is_active());

        assert!(!state.toggle());
        assert!(!state.is_active());
    }

    #[test]
    fn set_active_is_idempotent() {
        let mut state = BlackScreenState::default();

        assert!(state.set_active(true));
        assert!(state.set_active(true));
        assert!(state.is_active());

        assert!(!state.set_active(false));
        assert!(!state.set_active(false));
        assert!(!state.is_active());
    }
}

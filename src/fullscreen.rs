#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct FullscreenState {
    audience_fullscreen: bool,
}

impl FullscreenState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_audience_fullscreen(self) -> bool {
        self.audience_fullscreen
    }

    pub fn toggle_audience_fullscreen(&mut self) -> bool {
        self.set_audience_fullscreen(!self.audience_fullscreen)
    }

    pub fn exit_audience_fullscreen(&mut self) -> bool {
        self.set_audience_fullscreen(false)
    }

    pub fn set_audience_fullscreen(&mut self, fullscreen: bool) -> bool {
        self.audience_fullscreen = fullscreen;
        self.audience_fullscreen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_state_starts_windowed() {
        let state = FullscreenState::new();

        assert!(!state.is_audience_fullscreen());
    }

    #[test]
    fn toggle_audience_fullscreen_flips_state() {
        let mut state = FullscreenState::new();

        assert!(state.toggle_audience_fullscreen());
        assert!(state.is_audience_fullscreen());
        assert!(!state.toggle_audience_fullscreen());
        assert!(!state.is_audience_fullscreen());
    }

    #[test]
    fn exit_audience_fullscreen_clears_state() {
        let mut state = FullscreenState::new();
        state.set_audience_fullscreen(true);

        assert!(!state.exit_audience_fullscreen());
        assert!(!state.is_audience_fullscreen());
    }
}

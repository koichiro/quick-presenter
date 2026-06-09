#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct FullscreenState {
    slide_fullscreen: bool,
}

impl FullscreenState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_slide_fullscreen(self) -> bool {
        self.slide_fullscreen
    }

    pub fn toggle_slide_fullscreen(&mut self) -> bool {
        self.set_slide_fullscreen(!self.slide_fullscreen)
    }

    pub fn exit_slide_fullscreen(&mut self) -> bool {
        self.set_slide_fullscreen(false)
    }

    pub fn set_slide_fullscreen(&mut self, fullscreen: bool) -> bool {
        self.slide_fullscreen = fullscreen;
        self.slide_fullscreen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_state_starts_windowed() {
        let state = FullscreenState::new();

        assert!(!state.is_slide_fullscreen());
    }

    #[test]
    fn toggle_slide_fullscreen_flips_state() {
        let mut state = FullscreenState::new();

        assert!(state.toggle_slide_fullscreen());
        assert!(state.is_slide_fullscreen());
        assert!(!state.toggle_slide_fullscreen());
        assert!(!state.is_slide_fullscreen());
    }

    #[test]
    fn exit_slide_fullscreen_clears_state() {
        let mut state = FullscreenState::new();
        state.set_slide_fullscreen(true);

        assert!(!state.exit_slide_fullscreen());
        assert!(!state.is_slide_fullscreen());
    }
}

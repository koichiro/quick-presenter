// SPDX-License-Identifier: Apache-2.0

use std::cell::Cell;

thread_local! {
    static FULLSCREEN_TRANSITIONS: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn fullscreen_transition_active() -> bool {
    FULLSCREEN_TRANSITIONS.with(|count| count.get() != 0)
}

// Report the latest queued intent, including an explicit exit, so consumers
// cannot overwrite it with an intermediate native resize notification.
pub(crate) fn reported_fullscreen<T: Clone>(
    current: &Option<T>,
    pending: &Option<Option<T>>,
) -> Option<T> {
    pending.as_ref().unwrap_or(current).clone()
}

#[derive(Debug, Default)]
pub(crate) struct FullscreenTransition(Cell<bool>);

impl FullscreenTransition {
    pub(crate) fn get(&self) -> bool {
        self.0.get()
    }

    pub(crate) fn set(&self, active: bool) {
        if self.0.replace(active) != active {
            FULLSCREEN_TRANSITIONS.with(|count| {
                count.set(if active {
                    count.get() + 1
                } else {
                    count.get() - 1
                });
            });
        }
    }
}

impl Drop for FullscreenTransition {
    fn drop(&mut self) {
        self.set(false);
    }
}

#[cfg(test)]
mod fullscreen_input_tests {
    use super::{fullscreen_transition_active, reported_fullscreen, FullscreenTransition};

    #[test]
    fn queued_reversal_survives_intermediate_native_state_reports() {
        assert_eq!(reported_fullscreen(&Some(1), &Some(None)), None);
        assert_eq!(reported_fullscreen(&None, &Some(Some(2))), Some(2));
        assert_eq!(reported_fullscreen(&Some(1), &None), Some(1));
        assert_eq!(reported_fullscreen::<u8>(&None, &None), None);
    }

    #[test]
    fn transition_delivery_ends_only_after_all_windows_settle() {
        let first = FullscreenTransition::default();
        let second = FullscreenTransition::default();
        assert!(!fullscreen_transition_active());
        first.set(true);
        first.set(true); // Duplicate native notification must not leak the count.
        second.set(true);
        first.set(false);
        assert!(fullscreen_transition_active());
        second.set(false); // Also covers a failed transition.
        second.set(false);
        assert!(!fullscreen_transition_active());
    }

    #[test]
    fn destroying_a_transitioning_window_restores_normal_delivery() {
        let transition = FullscreenTransition::default();
        transition.set(true);
        assert!(fullscreen_transition_active());
        drop(transition);
        assert!(!fullscreen_transition_active());
    }
}

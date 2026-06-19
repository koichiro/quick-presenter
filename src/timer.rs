use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone)]
pub struct PresentationTimer {
    started_at: Option<Instant>,
}

impl PresentationTimer {
    pub fn reset(&mut self) {
        self.started_at = None;
    }

    pub fn start(&mut self, now: Instant) {
        if self.started_at.is_none() {
            self.started_at = Some(now);
        }
    }

    pub fn is_running(&self) -> bool {
        self.started_at.is_some()
    }

    pub fn elapsed_at(&self, now: Instant) -> Duration {
        self.started_at
            .and_then(|started_at| now.checked_duration_since(started_at))
            .unwrap_or_default()
    }

    pub fn elapsed_label_at(&self, now: Instant) -> String {
        format_elapsed(self.elapsed_at(now))
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TimerTransition {
    None,
    Start,
    Reset,
}

pub fn format_elapsed(elapsed: Duration) -> String {
    let total_seconds = elapsed.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

pub fn timer_transition_for_page_change(
    before_index: Option<u32>,
    after_index: Option<u32>,
) -> TimerTransition {
    if before_index == Some(0) && after_index.is_some_and(|index| index > 0) {
        TimerTransition::Start
    } else if before_index.is_some_and(|index| index > 0) && after_index == Some(0) {
        TimerTransition::Reset
    } else {
        TimerTransition::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_timer_displays_zero() {
        let timer = PresentationTimer::default();

        assert_eq!(timer.elapsed_label_at(Instant::now()), "00:00");
    }

    #[test]
    fn started_timer_formats_minutes_and_seconds() {
        let start = Instant::now();
        let mut timer = PresentationTimer::default();

        timer.start(start);

        assert_eq!(
            timer.elapsed_label_at(start + Duration::from_secs(125)),
            "02:05"
        );
    }

    #[test]
    fn hour_plus_duration_uses_hour_prefix() {
        assert_eq!(format_elapsed(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn repeated_start_does_not_restart_running_timer() {
        let start = Instant::now();
        let mut timer = PresentationTimer::default();

        timer.start(start);
        timer.start(start + Duration::from_secs(60));

        assert_eq!(
            timer.elapsed_label_at(start + Duration::from_secs(90)),
            "01:30"
        );
    }

    #[test]
    fn reset_returns_timer_to_zero() {
        let start = Instant::now();
        let mut timer = PresentationTimer::default();

        timer.start(start);
        timer.reset();

        assert!(!timer.is_running());
        assert_eq!(
            timer.elapsed_label_at(start + Duration::from_secs(30)),
            "00:00"
        );
    }

    #[test]
    fn leaving_first_page_starts_timer() {
        assert_eq!(
            timer_transition_for_page_change(Some(0), Some(1)),
            TimerTransition::Start
        );
        assert_eq!(
            timer_transition_for_page_change(Some(0), Some(4)),
            TimerTransition::Start
        );
    }

    #[test]
    fn returning_to_first_page_resets_timer() {
        assert_eq!(
            timer_transition_for_page_change(Some(1), Some(0)),
            TimerTransition::Reset
        );
        assert_eq!(
            timer_transition_for_page_change(Some(4), Some(0)),
            TimerTransition::Reset
        );
    }

    #[test]
    fn other_page_transitions_do_not_change_timer() {
        assert_eq!(
            timer_transition_for_page_change(None, Some(0)),
            TimerTransition::None
        );
        assert_eq!(
            timer_transition_for_page_change(None, Some(1)),
            TimerTransition::None
        );
        assert_eq!(
            timer_transition_for_page_change(Some(0), Some(0)),
            TimerTransition::None
        );
        assert_eq!(
            timer_transition_for_page_change(Some(0), None),
            TimerTransition::None
        );
        assert_eq!(
            timer_transition_for_page_change(Some(1), Some(2)),
            TimerTransition::None
        );
        assert_eq!(
            timer_transition_for_page_change(Some(2), Some(1)),
            TimerTransition::None
        );
    }
}

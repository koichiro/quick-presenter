//! Fixed logical canvas. No PDF, page, window-size, or transport input.
use crate::audience_events::{comment_display, AcceptedAudienceEvent, AudienceEvent};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub const WIDTH: f32 = 1280.0;
pub const SPEED: f32 = 160.0;
pub const LANES: usize = 6;
pub const MAX_ACTIVE: usize = 24;
pub const MAX_PENDING: usize = 32;
pub const WAIT: Duration = Duration::from_secs(2);
#[derive(Clone, Debug)]
pub struct CommentFrame {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
}
struct Pending {
    text: String,
    width: f32,
    received: Instant,
}
struct Active {
    text: String,
    width: f32,
    started: Instant,
    lane: usize,
}
#[derive(Default)]
pub struct CommentEngine {
    active: Vec<Active>,
    pending: VecDeque<Pending>,
    last_lane: Option<usize>,
    generation: u64,
    paused: bool,
    last_sequence: u64,
    history: VecDeque<String>,
}
impl CommentEngine {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn set_paused(&mut self, paused: bool, generation: u64) {
        self.paused = paused;
        self.generation = generation;
        self.active.clear();
        self.pending.clear();
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn history(&self) -> &VecDeque<String> {
        &self.history
    }
    pub fn receive(&mut self, event: &AcceptedAudienceEvent, width: f32, now: Instant) {
        let AudienceEvent::Comment { body, handle } = &event.event else {
            return;
        };
        if event.sequence <= self.last_sequence {
            return;
        }
        self.last_sequence = event.sequence;
        let text = comment_display(body, handle);
        self.history.push_back(text.clone());
        if self.history.len() > 20 {
            self.history.pop_front();
        }
        self.advance(now);
        if self.paused
            || event.display_generation != self.generation
            || now.saturating_duration_since(event.received_at) > WAIT
            || !width.is_finite()
            || width < 0.0
            || self.active.len() >= MAX_ACTIVE
            || self.pending.len() >= MAX_PENDING
        {
            return;
        }
        self.pending.push_back(Pending {
            text,
            width,
            received: event.received_at,
        });
        self.advance(now);
    }
    fn advance(&mut self, now: Instant) {
        self.active.retain(|a| {
            WIDTH + a.width - SPEED * now.saturating_duration_since(a.started).as_secs_f32() > 0.0
        });
        self.pending
            .retain(|p| now.saturating_duration_since(p.received) <= WAIT);
        while !self.pending.is_empty() && self.active.len() < MAX_ACTIVE {
            let start = self.last_lane.map_or(0, |lane| (lane + 1) % LANES);
            let lane = (0..LANES)
                .map(|offset| (start + offset) % LANES)
                .find(|lane| {
                    self.active
                        .iter()
                        .rev()
                        .find(|a| a.lane == *lane)
                        .is_none_or(|a| {
                            WIDTH + a.width
                                - SPEED * now.saturating_duration_since(a.started).as_secs_f32()
                                <= WIDTH - 32.0
                        })
                });
            let Some(lane) = lane else {
                break;
            };
            let p = self.pending.pop_front().unwrap();
            self.active.push(Active {
                text: p.text,
                width: p.width,
                started: now,
                lane,
            });
            self.last_lane = Some(lane);
        }
    }
    pub fn frames(&mut self, now: Instant) -> Vec<CommentFrame> {
        self.advance(now);
        self.active
            .iter()
            .map(|a| CommentFrame {
                text: a.text.clone(),
                width: a.width,
                x: WIDTH - SPEED * now.saturating_duration_since(a.started).as_secs_f32(),
                y: 48.0 + a.lane as f32 * 64.0,
            })
            .collect()
    }
    pub fn has_work(&self) -> bool {
        !self.active.is_empty() || !self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(sequence: u64, now: Instant, generation: u64) -> AcceptedAudienceEvent {
        AcceptedAudienceEvent {
            sequence,
            participant_id: 1,
            received_at: now,
            display_generation: generation,
            event: AudienceEvent::Comment {
                body: format!("Comment {sequence}"),
                handle: "Anonymous".into(),
            },
        }
    }
    #[test]
    fn round_robin_spacing_fifo_and_constant_lifetime() {
        let now = Instant::now();
        let mut e = CommentEngine::default();
        for i in 1..=8 {
            e.receive(&event(i, now, 0), 128.0, now);
        }
        let first = e.frames(now);
        assert_eq!(first.len(), 6);
        for (lane, f) in first.iter().enumerate() {
            assert_eq!(f.y, 48.0 + lane as f32 * 64.0);
        }
        assert_eq!(e.frames(now + Duration::from_millis(999)).len(), 6);
        let frames = e.frames(now + Duration::from_secs(1));
        assert_eq!(frames.len(), 8);
        assert_eq!(frames[6].text, "Comment 7");
        assert_eq!(frames[6].y, 48.0);
        assert_eq!(frames[6].x - (frames[0].x + frames[0].width), 32.0);
        assert_eq!(e.frames(now + Duration::from_millis(8800)).len(), 2);
        assert!(e.frames(now + Duration::from_millis(9800)).is_empty());
    }
    #[test]
    fn pause_generations_keep_history_without_replay() {
        let now = Instant::now();
        let mut e = CommentEngine::default();
        e.receive(&event(1, now, 0), 100.0, now);
        e.set_paused(true, 1);
        e.receive(&event(2, now, 1), 100.0, now);
        e.set_paused(false, 2);
        e.receive(&event(3, now, 1), 100.0, now);
        assert!(e.frames(now).is_empty());
        assert_eq!(e.history().len(), 3);
        e.receive(&event(4, now, 2), 100.0, now);
        assert_eq!(e.frames(now).len(), 1);
        e.receive(&event(4, now, 2), 100.0, now);
        assert_eq!(e.history().len(), 4);
        e.reset();
        assert!(!e.paused());
        assert!(e.history().is_empty());
    }
    #[test]
    fn expired_or_overflowed_comments_stay_in_twenty_item_history() {
        let now = Instant::now();
        let mut e = CommentEngine::default();
        for i in 1..=80 {
            e.receive(&event(i, now, 0), 1000.0, now);
        }
        assert_eq!(e.active.len(), 6);
        assert_eq!(e.pending.len(), MAX_PENDING);
        assert_eq!(e.history().len(), 20);
        assert_eq!(e.history().back().unwrap(), "Comment 80");
        assert_eq!(e.frames(now + WAIT + Duration::from_millis(1)).len(), 6);
        assert!(e.pending.is_empty());
        assert!(e.frames(now + Duration::from_secs(60)).is_empty());
        e.receive(&event(81, now, 0), 10.0, now + Duration::from_secs(60));
        assert!(!e.has_work());
        assert_eq!(e.history().back().unwrap(), "Comment 81");
    }
    #[test]
    fn active_limit_and_invalid_measurements_are_bounded() {
        let now = Instant::now();
        let mut e = CommentEngine::default();
        for i in 1..=30 {
            let at = now + Duration::from_millis(i * 300);
            e.receive(&event(i, at, 0), 0.0, at);
        }
        assert!(e.active.len() <= MAX_ACTIVE);
        assert!(e.pending.len() <= MAX_PENDING);
        e.receive(&event(31, now, 0), f32::NAN, now);
        assert_eq!(e.history().len(), 20);
    }
}

//! Presentation-independent lifetime and density management for reaction overlays.
use crate::audience_events::{AcceptedAudienceEvent, AudienceEvent, ReactionKind};
use std::time::{Duration, Instant};

pub const MAX_VISIBLE: usize = 24;
pub const LIFETIME: Duration = Duration::from_millis(1800);
#[derive(Clone, Copy, Debug)]
pub struct OverlayFrame {
    pub kind: ReactionKind,
    pub x: f32,
    pub y: f32,
    pub opacity: f32,
}
struct Active {
    kind: ReactionKind,
    started: Instant,
    slot: usize,
}
#[derive(Default)]
pub struct OverlayEngine {
    active: Vec<Active>,
    cleared_at: Option<Instant>,
}
impl OverlayEngine {
    pub fn clear(&mut self, now: Instant) {
        self.active.clear();
        self.cleared_at = Some(now);
    }
    pub fn push(&mut self, event: &AcceptedAudienceEvent, now: Instant) {
        self.expire(now);
        if self
            .cleared_at
            .is_some_and(|cutoff| event.received_at <= cutoff)
            || now.saturating_duration_since(event.received_at) >= LIFETIME
            || self.active.len() >= MAX_VISIBLE
        {
            return;
        }
        let AudienceEvent::Reaction { kind } = event.event;
        let slot = (0..MAX_VISIBLE)
            .find(|slot| !self.active.iter().any(|a| a.slot == *slot))
            .unwrap();
        self.active.push(Active {
            kind,
            started: event.received_at,
            slot,
        });
    }
    fn expire(&mut self, now: Instant) {
        self.active
            .retain(|a| now.saturating_duration_since(a.started) < LIFETIME);
    }
    pub fn frames(&mut self, now: Instant) -> Vec<OverlayFrame> {
        self.expire(now);
        self.active
            .iter()
            .map(|a| {
                let progress =
                    now.saturating_duration_since(a.started).as_secs_f32() / LIFETIME.as_secs_f32();
                OverlayFrame {
                    kind: a.kind,
                    x: 0.06 + (a.slot % 8) as f32 * 0.12,
                    y: 0.9 - progress * 0.65 - (a.slot / 8) as f32 * 0.05,
                    opacity: ((1.0 - progress) / 0.35).clamp(0.0, 1.0),
                }
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event(at: Instant) -> AcceptedAudienceEvent {
        AcceptedAudienceEvent {
            sequence: 1,
            participant_id: 1,
            received_at: at,
            event: AudienceEvent::Reaction {
                kind: ReactionKind::Heart,
            },
        }
    }
    #[test]
    fn density_expiry_and_motion_are_bounded() {
        let now = Instant::now();
        let mut engine = OverlayEngine::default();
        for _ in 0..MAX_VISIBLE + 100 {
            engine.push(&event(now), now);
        }
        let initial = engine.frames(now);
        assert_eq!(initial.len(), MAX_VISIBLE);
        let later = engine.frames(now + Duration::from_millis(1500));
        assert!(later[0].y < initial[0].y && later[0].opacity < initial[0].opacity);
        assert!(later.iter().all(|f| (0.0..=1.0).contains(&f.x)
            && (0.0..=1.0).contains(&f.y)
            && (0.0..=1.0).contains(&f.opacity)));
        assert!(engine.frames(now + LIFETIME).is_empty());
        engine.push(&event(now), now + LIFETIME);
        assert!(engine.frames(now + LIFETIME).is_empty());
    }
    #[test]
    fn clearing_prevents_late_delivery_from_reappearing() {
        let now = Instant::now();
        let mut engine = OverlayEngine::default();
        engine.push(&event(now), now);
        engine.clear(now + Duration::from_millis(10));
        engine.push(&event(now), now + Duration::from_millis(20));
        assert!(engine.frames(now + Duration::from_millis(20)).is_empty());
        engine.push(
            &event(now + Duration::from_millis(21)),
            now + Duration::from_millis(30),
        );
        assert_eq!(engine.frames(now + Duration::from_millis(30)).len(), 1);
    }
    #[test]
    fn freed_slots_are_reused_without_exceeding_density() {
        let now = Instant::now();
        let mut engine = OverlayEngine::default();
        engine.push(&event(now), now);
        engine.push(
            &event(now + Duration::from_millis(100)),
            now + Duration::from_millis(100),
        );
        engine.push(&event(now + LIFETIME), now + LIFETIME);
        let frames = engine.frames(now + LIFETIME);
        assert_eq!(frames.len(), 2);
        assert_ne!(frames[0].x, frames[1].x);
    }
}

//! Transport-independent reaction validation, admission, and bounded delivery.
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub const EVENT_CAPACITY: usize = 128;
pub const UI_BATCH: usize = 16;
pub const EVENT_TTL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactionKind {
    ThumbsUp,
    Heart,
    Applause,
    Laugh,
    Question,
}
impl ReactionKind {
    pub fn emoji(self) -> &'static str {
        match self {
            Self::ThumbsUp => "👍",
            Self::Heart => "❤️",
            Self::Applause => "👏",
            Self::Laugh => "😂",
            Self::Question => "❓",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudienceEvent {
    Reaction { kind: ReactionKind },
}
#[derive(Clone, Debug)]
pub struct AcceptedAudienceEvent {
    pub sequence: u64,
    pub participant_id: u64,
    pub received_at: Instant,
    pub event: AudienceEvent,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReactionRequest {
    v: u32,
    r#type: String,
    request_id: String,
    kind: ReactionKind,
}
pub fn parse_reaction(text: &str) -> Option<(String, ReactionKind)> {
    let r: ReactionRequest = serde_json::from_str(text).ok()?;
    (r.v == 1
        && r.r#type == "reaction"
        && !r.request_id.is_empty()
        && r.request_id.len() <= 64
        && r.request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    .then_some((r.request_id, r.kind))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Admission {
    Accepted,
    Stopped,
    RateLimited,
    Busy,
}

/// Tokens bound bursts as well as sustained input; no participant-count limit.
pub struct RateBudget {
    tokens: f64,
    capacity: f64,
    per_second: f64,
    updated: Instant,
}
impl RateBudget {
    pub fn new(capacity: u32, per_second: u32, now: Instant) -> Self {
        Self {
            tokens: capacity as f64,
            capacity: capacity as f64,
            per_second: per_second as f64,
            updated: now,
        }
    }
    pub fn allow(&mut self, now: Instant) -> bool {
        self.tokens = (self.tokens
            + now.saturating_duration_since(self.updated).as_secs_f64() * self.per_second)
            .min(self.capacity);
        self.updated = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}
/// Owned by one session handle. Dropping that handle also drops all old events.
pub struct AudienceEngine {
    stopped: bool,
    sequence: u64,
    queue: VecDeque<AcceptedAudienceEvent>,
    budget: RateBudget,
}
impl AudienceEngine {
    pub fn new(now: Instant) -> Self {
        Self {
            stopped: false,
            sequence: 0,
            queue: VecDeque::new(),
            budget: RateBudget::new(120, 120, now),
        }
    }
    pub fn stop(&mut self) {
        self.stopped = true;
        self.queue.clear();
    }
    pub fn accept(&mut self, kind: ReactionKind, participant_id: u64, now: Instant) -> Admission {
        if self.stopped {
            return Admission::Stopped;
        }
        if !self.budget.allow(now) {
            return Admission::RateLimited;
        }
        self.expire(now);
        if self.queue.len() >= EVENT_CAPACITY {
            return Admission::Busy;
        }
        self.sequence += 1;
        self.queue.push_back(AcceptedAudienceEvent {
            sequence: self.sequence,
            participant_id,
            received_at: now,
            event: AudienceEvent::Reaction { kind },
        });
        Admission::Accepted
    }
    fn expire(&mut self, now: Instant) {
        while self
            .queue
            .front()
            .is_some_and(|e| now.saturating_duration_since(e.received_at) >= EVENT_TTL)
        {
            self.queue.pop_front();
        }
    }
    pub fn drain(&mut self, now: Instant) -> Vec<AcceptedAudienceEvent> {
        self.expire(now);
        let count = self.queue.len().min(UI_BATCH);
        self.queue.drain(..count).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_version_kind_and_request_id() {
        assert_eq!(
            parse_reaction(r#"{"v":1,"type":"reaction","request_id":"1","kind":"applause"}"#),
            Some(("1".into(), ReactionKind::Applause))
        );
        for text in [
            r#"{"v":2,"type":"reaction","request_id":"1","kind":"heart"}"#,
            r#"{"v":1,"type":"reaction","request_id":"1","kind":"unknown"}"#,
            r#"{"v":1,"type":"comment","request_id":"1","kind":"heart"}"#,
            r#"{"v":1,"type":"reaction","request_id":"","kind":"heart"}"#,
            r#"{"v":1,"type":"reaction","request_id":"1","kind":"heart","extra":0}"#,
        ] {
            assert!(parse_reaction(text).is_none());
        }
    }
    #[test]
    fn budget_limits_bursts_and_refills() {
        let now = Instant::now();
        let mut b = RateBudget::new(5, 2, now);
        for _ in 0..5 {
            assert!(b.allow(now));
        }
        assert!(!b.allow(now));
        assert!(!b.allow(now + Duration::from_millis(250)));
        assert!(b.allow(now + Duration::from_millis(500)));
        assert!(!b.allow(now + Duration::from_millis(500)));
    }
    #[test]
    fn delivery_is_bounded_expires_and_stops_permanently() {
        let now = Instant::now();
        let mut e = AudienceEngine::new(now);
        for n in 0..EVENT_CAPACITY {
            assert_eq!(
                e.accept(
                    ReactionKind::Heart,
                    9,
                    now + Duration::from_millis(n as u64 * 10)
                ),
                Admission::Accepted
            );
        }
        assert_eq!(
            e.accept(ReactionKind::Heart, 9, now + Duration::from_millis(1300)),
            Admission::Busy
        );
        let batch = e.drain(now + Duration::from_millis(1300));
        assert_eq!(batch.len(), UI_BATCH);
        assert_eq!(batch[0].sequence, 1);
        assert_eq!(batch[0].participant_id, 9);
        e.stop();
        assert_eq!(e.accept(ReactionKind::Heart, 9, now), Admission::Stopped);
        assert!(e.drain(now).is_empty());
        let mut e = AudienceEngine::new(now);
        assert_eq!(
            e.accept(ReactionKind::Heart, 1, now + Duration::from_secs(5)),
            Admission::Accepted
        );
        assert!(e.drain(now + Duration::from_secs(7)).is_empty());
    }
    #[test]
    fn session_budget_applies_across_participants() {
        let now = Instant::now();
        let mut e = AudienceEngine::new(now);
        for id in 0..120 {
            assert_eq!(e.accept(ReactionKind::Heart, id, now), Admission::Accepted);
        }
        assert_eq!(
            e.accept(ReactionKind::Heart, 121, now),
            Admission::RateLimited
        );
    }
}

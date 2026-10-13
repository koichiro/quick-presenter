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
    Comment { body: String, handle: String },
}
#[derive(Clone, Debug)]
pub struct AcceptedAudienceEvent {
    pub sequence: u64,
    pub participant_id: u64,
    pub received_at: Instant,
    pub display_generation: u64,
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
pub const BODY_CHARS: usize = 200;
pub const BODY_BYTES: usize = 800;
pub const HANDLE_CHARS: usize = 32;
pub const HANDLE_BYTES: usize = 128;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CommentRequest {
    v: u32,
    r#type: String,
    request_id: String,
    body: String,
    handle: String,
}
pub fn parse_event(text: &str) -> Option<(String, AudienceEvent)> {
    if let Some((id, kind)) = parse_reaction(text) {
        return Some((id, AudienceEvent::Reaction { kind }));
    }
    let r: CommentRequest = serde_json::from_str(text).ok()?;
    if r.v != 1
        || r.r#type != "comment"
        || r.request_id.is_empty()
        || r.request_id.len() > 64
        || !r
            .request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || r.body.len() > BODY_BYTES
        || r.body.chars().count() > BODY_CHARS
        || r.handle.len() > HANDLE_BYTES
        || r.handle.chars().count() > HANDLE_CHARS
        || r.body
            .chars()
            .chain(r.handle.chars())
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        || r.body.trim().is_empty()
    {
        return None;
    }
    let body = single_line(&r.body);
    Some((
        r.request_id,
        AudienceEvent::Comment {
            body,
            handle: r.handle,
        },
    ))
}
fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub fn comment_display(body: &str, handle: &str) -> String {
    let name = single_line(handle);
    if name.is_empty() || name.eq_ignore_ascii_case("anonymous") {
        body.to_owned()
    } else {
        format!("{name}: {body}")
    }
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
    comment_budget: RateBudget,
    generation: u64,
    session_id: String,
    pub(crate) logger: Option<std::sync::Arc<crate::audience_log::CommentLogger>>,
}
impl AudienceEngine {
    pub fn new(now: Instant) -> Self {
        Self {
            stopped: false,
            sequence: 0,
            queue: VecDeque::new(),
            budget: RateBudget::new(120, 120, now),
            comment_budget: RateBudget::new(30, 30, now),
            generation: 0,
            session_id: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            logger: None,
        }
    }
    pub fn stop(&mut self) {
        self.stopped = true;
        self.queue.clear();
    }
    pub fn accept(&mut self, kind: ReactionKind, participant_id: u64, now: Instant) -> Admission {
        self.accept_event(AudienceEvent::Reaction { kind }, participant_id, now)
    }
    pub fn advance_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }
    pub fn accept_event(
        &mut self,
        event: AudienceEvent,
        participant_id: u64,
        now: Instant,
    ) -> Admission {
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
        if matches!(event, AudienceEvent::Comment { .. }) {
            if !self.comment_budget.allow(now) {
                return Admission::RateLimited;
            }
            if let Some(logger) = &self.logger {
                if !logger.submit(&self.session_id, self.sequence + 1, participant_id, &event) {
                    return Admission::Busy;
                }
            }
        }
        self.sequence += 1;
        self.queue.push_back(AcceptedAudienceEvent {
            sequence: self.sequence,
            participant_id,
            received_at: now,
            display_generation: self.generation,
            event,
        });
        Admission::Accepted
    }
    fn expire(&mut self, now: Instant) {
        self.queue.retain(|e| {
            matches!(e.event, AudienceEvent::Comment { .. })
                || now.saturating_duration_since(e.received_at) < EVENT_TTL
        });
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
    fn validates_comments_normalizes_lines_and_preserves_handle() {
        let request = |body: &str, handle: &str| {
            serde_json::json!({"v":1,"type":"comment","request_id":"c-1","body":body,"handle":handle}).to_string()
        };
        assert_eq!(
            parse_event(&request(" hello\r\nworld\t! ", " Anonymous "))
                .unwrap()
                .1,
            AudienceEvent::Comment {
                body: "hello world !".into(),
                handle: " Anonymous ".into()
            }
        );
        for (body, handle) in [("  ", "A"), ("x\u{0000}", "A"), ("x", "A\u{007f}")] {
            assert!(parse_event(&request(body, handle)).is_none());
        }
        assert!(parse_event(&request(&"あ".repeat(200), &"名".repeat(32))).is_some());
        assert!(parse_event(&request(&"x".repeat(201), "A")).is_none());
        assert!(parse_event(&request("x", &"名".repeat(33))).is_none());
        assert!(parse_event(&request(&"😀".repeat(200), "A")).is_some());
        assert!(parse_event(&request("x", "A").replace("\"v\":1", "\"v\":2")).is_none());
        for handle in ["", "  ", " Anonymous ", "ANONYMOUS", "anonymous"] {
            assert_eq!(comment_display("hello", handle), "hello");
        }
        assert_eq!(
            comment_display("<script>hello</script>", " Koichiro "),
            "Koichiro: <script>hello</script>"
        );
    }
    #[test]
    fn comment_delivery_survives_overlay_deadline_and_marks_generation() {
        let now = Instant::now();
        let mut e = AudienceEngine::new(now);
        let comment = AudienceEvent::Comment {
            body: "hi".into(),
            handle: "".into(),
        };
        e.accept_event(comment.clone(), 1, now);
        e.advance_generation();
        e.accept_event(comment, 1, now);
        let events = e.drain(now + Duration::from_secs(3));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].display_generation, 0);
        assert_eq!(events[1].display_generation, 1);
    }
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

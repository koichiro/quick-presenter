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
    pub overlay_deadline: Instant,
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
        self.accept_relay(event, participant_id, now, now + EVENT_TTL, self.generation)
    }
    /// Relay admission shares queue/log limits, but keeps receiver-relative expiry
    /// and display fencing separate from the local session generation.
    pub fn accept_relay(
        &mut self,
        event: AudienceEvent,
        participant_id: u64,
        now: Instant,
        overlay_deadline: Instant,
        generation: u64,
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
            overlay_deadline,
            display_generation: generation,
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

    fn comment_request() -> serde_json::Value {
        serde_json::json!({"v":1,"type":"comment","request_id":"c-1","body":"Hello","handle":"Anonymous"})
    }

    fn comment() -> AudienceEvent {
        AudienceEvent::Comment {
            body: "Hello".into(),
            handle: "Anonymous".into(),
        }
    }

    #[test]
    fn comment_schema_requires_all_fields_and_rejects_unknown_fields() {
        let base = comment_request();
        for field in ["v", "type", "request_id", "body", "handle"] {
            let mut missing = base.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                parse_event(&missing.to_string()).is_none(),
                "missing {field}"
            );
            let mut null = base.clone();
            null[field] = serde_json::Value::Null;
            assert!(parse_event(&null.to_string()).is_none(), "null {field}");
        }
        for (field, value) in [
            ("v", serde_json::json!("1")),
            ("type", serde_json::json!("reaction")),
            ("request_id", serde_json::json!(1)),
            ("body", serde_json::json!(["Hello"])),
            ("handle", serde_json::json!(false)),
            ("extra", serde_json::json!("ignored?")),
        ] {
            let mut request = base.clone();
            request[field] = value;
            assert!(parse_event(&request.to_string()).is_none(), "{field}");
        }
    }

    #[test]
    fn comment_request_ids_have_exact_ascii_boundaries() {
        for (id, valid) in [
            ("a_9-Z".to_owned(), true),
            ("x".repeat(64), true),
            ("x".repeat(65), false),
            (String::new(), false),
            ("name space".to_owned(), false),
            ("é".to_owned(), false),
            ("a/b".to_owned(), false),
        ] {
            let mut request = comment_request();
            request["request_id"] = serde_json::json!(id);
            let parsed = parse_event(&request.to_string());
            assert_eq!(parsed.is_some(), valid, "{id:?}");
            if valid {
                assert_eq!(parsed.unwrap().0, id);
            }
        }
    }

    #[test]
    fn unicode_limits_apply_before_whitespace_normalization() {
        let mut request = comment_request();
        request["body"] = serde_json::json!("😀".repeat(BODY_CHARS));
        request["handle"] = serde_json::json!("🦀".repeat(HANDLE_CHARS));
        let (_, event) = parse_event(&request.to_string()).unwrap();
        assert_eq!(
            event,
            AudienceEvent::Comment {
                body: "😀".repeat(BODY_CHARS),
                handle: "🦀".repeat(HANDLE_CHARS),
            }
        );
        for (field, value) in [
            ("body", "😀".repeat(BODY_CHARS + 1)),
            ("handle", "🦀".repeat(HANDLE_CHARS + 1)),
            ("body", format!("x{}", " ".repeat(BODY_CHARS))),
        ] {
            let mut invalid = request.clone();
            invalid[field] = serde_json::json!(value);
            assert!(parse_event(&invalid.to_string()).is_none(), "{field}");
        }
        request["body"] = serde_json::json!("\u{2003}\u{a0}\t\r\n");
        assert!(parse_event(&request.to_string()).is_none());
    }

    #[test]
    fn comments_reject_control_characters_in_both_fields() {
        for code in (0..=0x1f).chain(0x7f..=0x9f) {
            let character = char::from_u32(code).unwrap();
            for field in ["body", "handle"] {
                let mut request = comment_request();
                request[field] = serde_json::json!(format!("a{character}b"));
                assert_eq!(
                    parse_event(&request.to_string()).is_some(),
                    matches!(character, '\n' | '\r' | '\t'),
                    "{field}: U+{code:04X}"
                );
            }
        }
    }

    #[test]
    fn event_parser_preserves_every_reaction_kind() {
        for (wire, kind) in [
            ("thumbs_up", ReactionKind::ThumbsUp),
            ("heart", ReactionKind::Heart),
            ("applause", ReactionKind::Applause),
            ("laugh", ReactionKind::Laugh),
            ("question", ReactionKind::Question),
        ] {
            let request =
                serde_json::json!({"v":1,"type":"reaction","request_id":"r-1","kind":wire});
            assert_eq!(
                parse_event(&request.to_string()),
                Some(("r-1".into(), AudienceEvent::Reaction { kind }))
            );
        }
    }

    #[test]
    fn expired_reactions_behind_comments_do_not_block_bounded_delivery() {
        let now = Instant::now();
        let mut engine = AudienceEngine::new(now);
        for id in 0..20 {
            assert_eq!(engine.accept_event(comment(), id, now), Admission::Accepted);
            assert_eq!(
                engine.accept(ReactionKind::Heart, id, now),
                Admission::Accepted
            );
        }
        let first = engine.drain(now + EVENT_TTL);
        let second = engine.drain(now + EVENT_TTL);
        assert_eq!(first.len(), UI_BATCH);
        assert_eq!(second.len(), 20 - UI_BATCH);
        let events: Vec<_> = first.into_iter().chain(second).collect();
        for (id, event) in events.iter().enumerate() {
            assert_eq!(event.sequence, id as u64 * 2 + 1);
            assert_eq!(event.participant_id, id as u64);
            assert_eq!(event.event, comment());
        }
        assert!(engine.drain(now + EVENT_TTL).is_empty());
    }

    #[test]
    fn comment_budget_is_shared_refills_and_leaves_reactions_available() {
        let now = Instant::now();
        let mut engine = AudienceEngine::new(now);
        for id in 0..30 {
            assert_eq!(engine.accept_event(comment(), id, now), Admission::Accepted);
        }
        assert_eq!(
            engine.accept_event(comment(), 30, now),
            Admission::RateLimited
        );
        assert_eq!(
            engine.accept(ReactionKind::Heart, 31, now),
            Admission::Accepted
        );
        let refilled = now + Duration::from_millis(34);
        assert_eq!(
            engine.accept_event(comment(), 32, refilled),
            Admission::Accepted
        );
        assert_eq!(
            engine.accept_event(comment(), 33, refilled),
            Admission::RateLimited
        );
        let events: Vec<_> = engine
            .drain(refilled)
            .into_iter()
            .chain(engine.drain(refilled))
            .collect();
        assert_eq!(events.len(), 32);
        assert_eq!(events[30].sequence, 31);
        assert_eq!(events[31].sequence, 32);
        assert_eq!(events[31].participant_id, 32);
    }

    #[test]
    fn retained_comments_fill_queue_until_a_ui_batch_is_drained() {
        let now = Instant::now();
        let mut engine = AudienceEngine::new(now);
        for id in 0..EVENT_CAPACITY {
            let received = now + Duration::from_secs(id as u64);
            assert_eq!(
                engine.accept_event(comment(), id as u64, received),
                Admission::Accepted
            );
        }
        let later = now + Duration::from_secs(EVENT_CAPACITY as u64);
        assert_eq!(engine.accept_event(comment(), 999, later), Admission::Busy);
        assert_eq!(engine.drain(later).len(), UI_BATCH);
        assert_eq!(
            engine.accept_event(comment(), 999, later),
            Admission::Accepted
        );
        let mut events = Vec::new();
        loop {
            let batch = engine.drain(later);
            if batch.is_empty() {
                break;
            }
            events.extend(batch);
        }
        assert_eq!(events.len(), EVENT_CAPACITY - UI_BATCH + 1);
        assert_eq!(events.last().unwrap().sequence, EVENT_CAPACITY as u64 + 1);
        engine.stop();
        assert_eq!(
            engine.accept_event(comment(), 1000, later),
            Admission::Stopped
        );
        assert!(engine.drain(later).is_empty());
    }

    #[test]
    fn relay_metadata_does_not_change_local_admission_generation() {
        let now = Instant::now();
        let mut engine = AudienceEngine::new(now);
        assert_eq!(engine.advance_generation(), 1);
        let deadline = now + Duration::from_millis(100);
        assert_eq!(
            engine.accept_relay(comment(), 7, now, deadline, 42),
            Admission::Accepted
        );
        assert_eq!(engine.accept_event(comment(), 8, now), Admission::Accepted);
        let events = engine.drain(now);
        assert_eq!(events[0].received_at, now);
        assert_eq!(events[0].overlay_deadline, deadline);
        assert_eq!(events[0].display_generation, 42);
        assert_eq!(events[1].overlay_deadline, now + EVENT_TTL);
        assert_eq!(events[1].display_generation, 1);
    }

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

//! Opt-in draft relay client. No network or credential IO occurs on the UI thread.
use crate::{
    audience::{SessionSnapshot, SessionStatus},
    audience_events::{
        AcceptedAudienceEvent, Admission, AudienceEngine, AudienceEvent, ReactionKind,
    },
};
use futures_util::{SinkExt, StreamExt};
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    fmt,
    fs::File,
    hash::{Hash, Hasher},
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::{protocol::WebSocketConfig, Message};

pub const VERSION: &str = "1-draft.1";
const MAX_BYTES: usize = 4096;
const TIMEOUT: Duration = Duration::from_secs(10);
const SEND_TIMEOUT: Duration = Duration::from_secs(2);
const DEDUP_TTL: Duration = Duration::from_secs(30);
const DEDUP_CAPACITY: usize = 256;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RelayConfig {
    pub(crate) room_id: String,
    code: String,
    pub(crate) join_url: String,
    presenter_url: String,
    pub(crate) presenter_token: String,
}
fn identifier(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn token(s: &str) -> bool {
    s.len() == 43 && identifier(s, 43)
}
fn counter(s: &str) -> Option<u128> {
    (!s.is_empty()
        && s.len() <= 20
        && (s == "0" || !s.starts_with('0'))
        && s.bytes().all(|b| b.is_ascii_digit()))
    .then(|| s.parse().ok())
    .flatten()
}
impl RelayConfig {
    fn load(path: &PathBuf) -> Result<Self, &'static str> {
        let file = File::open(path).map_err(|_| "Cannot open relay configuration.")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if file
                .metadata()
                .map_err(|_| "Cannot inspect relay configuration.")?
                .permissions()
                .mode()
                & 0o077
                != 0
            {
                return Err("Relay configuration must be readable only by its owner (chmod 600).");
            }
        }
        let mut bytes = Vec::new();
        file.take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read relay configuration.")?;
        let config: Self = serde_json::from_value(strict_json(&bytes)?)
            .map_err(|_| "Invalid relay configuration.")?;
        config.validate()?;
        Ok(config)
    }
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let bad = "Invalid relay configuration.";
        if !identifier(&self.room_id, 64)
            || !identifier(&self.code, 64)
            || !token(&self.presenter_token)
        {
            return Err(bad);
        }
        let join = url::Url::parse(&self.join_url).map_err(|_| bad)?;
        let socket = url::Url::parse(&self.presenter_url).map_err(|_| bad)?;
        for url in [&join, &socket] {
            if !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.host_str().is_none()
            {
                return Err(bad);
            }
        }
        if join.path() != format!("/join/{}", self.room_id)
            || !join
                .fragment()
                .is_some_and(|f| f.strip_prefix("k=").is_some_and(token))
            || socket.fragment().is_some()
            || join.host_str() != socket.host_str()
            || join.port_or_known_default() != socket.port_or_known_default()
        {
            return Err(bad);
        }
        let secure = join.scheme() == "https" && socket.scheme() == "wss";
        let loopback = socket.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if !secure && !(loopback && join.scheme() == "http" && socket.scheme() == "ws") {
            return Err("Relay requires HTTPS/WSS; HTTP/WS is allowed only on loopback for tests.");
        }
        Ok(())
    }
}

// Reject duplicate keys at every depth before typed deserialization. serde_json's
// ordinary Value parser would overwrite them, hiding malformed relay messages.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON with unique keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Unique(values.into()))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut map = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if map.contains_key(&k) {
                        return Err(de::Error::custom("Duplicate JSON key"));
                    }
                    map.insert(k, a.next_value::<Unique>()?.0);
                }
                Ok(Unique(map.into()))
            }
        }
        d.deserialize_any(V)
    }
}
pub(crate) fn strict_json(bytes: &[u8]) -> Result<Value, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("Relay message exceeds size limit.");
    }
    serde_json::from_slice::<Unique>(bytes)
        .map(|v| v.0)
        .map_err(|_| "Malformed relay JSON.")
}
#[derive(Deserialize, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Payload {
    Reaction { kind: ReactionKind },
    Comment { body: String, handle: String },
}
impl Payload {
    fn event(&self) -> Result<AudienceEvent, &'static str> {
        match self {
            Self::Reaction { kind } => Ok(AudienceEvent::Reaction { kind: *kind }),
            Self::Comment { body, handle } => {
                let request = json!({"v":1,"type":"comment","request_id":"relay","body":body,"handle":handle});
                crate::audience_events::parse_event(&request.to_string())
                    .map(|(_, e)| e)
                    .ok_or("Invalid relay comment.")
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum Incoming {
    #[serde(rename = "presenter_welcome")]
    Welcome {
        #[serde(rename = "protocol")]
        _protocol: String,
        #[serde(rename = "version")]
        _version: String,
        presenter_epoch: String,
        display_epoch: String,
        audience_connections: u64,
    },
    #[serde(rename = "state")]
    State {
        #[serde(rename = "protocol")]
        _protocol: String,
        #[serde(rename = "version")]
        _version: String,
        revision: String,
        display_epoch: String,
        available: bool,
        audience_connections: u64,
    },
    #[serde(rename = "event")]
    Event {
        #[serde(rename = "protocol")]
        _protocol: String,
        #[serde(rename = "version")]
        _version: String,
        event_id: String,
        connection_id: String,
        request_id: String,
        presenter_epoch: String,
        display_epoch: String,
        offer_ttl_ms: u64,
        payload: Payload,
    },
}
fn incoming(text: &str) -> Result<Incoming, &'static str> {
    let value = strict_json(text.as_bytes())?;
    if value["protocol"] != "qp.presenter" || value["version"] != VERSION {
        return Err("Unsupported relay protocol.");
    }
    let message: Incoming = serde_json::from_value(value).map_err(|_| "Invalid relay message.")?;
    match &message {
        Incoming::Welcome {
            presenter_epoch,
            display_epoch,
            audience_connections,
            ..
        } => {
            if counter(presenter_epoch).is_none()
                || counter(display_epoch).is_none()
                || *audience_connections > 9_007_199_254_740_991
            {
                return Err("Invalid relay snapshot.");
            }
        }
        Incoming::State {
            revision,
            display_epoch,
            audience_connections,
            ..
        } => {
            if counter(revision).is_none()
                || counter(display_epoch).is_none()
                || *audience_connections > 9_007_199_254_740_991
            {
                return Err("Invalid relay state.");
            }
        }
        Incoming::Event {
            event_id,
            connection_id,
            request_id,
            presenter_epoch,
            display_epoch,
            offer_ttl_ms,
            payload,
            ..
        } => {
            if ![event_id, connection_id, request_id]
                .iter()
                .all(|id| identifier(id, 64))
                || counter(presenter_epoch).is_none()
                || counter(display_epoch).is_none()
                || !(1..=2000).contains(offer_ttl_ms)
            {
                return Err("Invalid relay offer.");
            }
            payload.event()?;
        }
    }
    Ok(message)
}
#[derive(Clone, Copy, Default)]
struct Display {
    generation: u64,
    paused: bool,
}
pub struct RemoteAudienceSession {
    status: Arc<Mutex<SessionStatus>>,
    connections: Arc<AtomicUsize>,
    engine: Arc<Mutex<AudienceEngine>>,
    stop: watch::Sender<bool>,
    display: watch::Sender<Display>,
    worker: Option<JoinHandle<()>>,
}
impl RemoteAudienceSession {
    pub fn start(path: PathBuf) -> std::io::Result<Self> {
        Self::start_worker(Some(path), None)
    }
    pub(crate) fn start_managed(
        settings: crate::audience_relay_http::ManagedRelay,
    ) -> std::io::Result<Self> {
        Self::start_worker(None, Some(settings))
    }
    fn start_worker(
        path: Option<PathBuf>,
        managed: Option<crate::audience_relay_http::ManagedRelay>,
    ) -> std::io::Result<Self> {
        let status = Arc::new(Mutex::new(SessionStatus::Starting));
        let connections = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(Mutex::new(AudienceEngine::new(Instant::now())));
        let (stop, mut stopped) = watch::channel(false);
        let (display, updates) = watch::channel(Display::default());
        let shared = (status.clone(), connections.clone(), engine.clone());
        let worker = thread::Builder::new().name("audience-relay".into()).spawn(move || {
            let (status, connections, engine) = shared;
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), &'static str> {
                let config = path.as_ref().map(RelayConfig::load).transpose()?;
                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|_| "Cannot start relay worker.")?;
                #[cfg(not(test))] { engine.lock().unwrap_or_else(|e| e.into_inner()).logger = Some(crate::audience_log::logger()); }
                runtime.block_on(async {
                    if *stopped.borrow() {
                        if let Some(settings) = managed.as_ref() {
                            if settings.has_attempt() {
                                let client = crate::audience_relay_http::client()?;
                                crate::audience_relay_http::cleanup(&client, settings).await?;
                            }
                        }
                        return Ok(());
                    }
                    if let Some(settings) = managed {
                        let client = crate::audience_relay_http::client()?;
                        let room = tokio::select! {
                            biased;
                            _ = stopped.changed() => None,
                            result = crate::audience_relay_http::create(&client, &settings) => Some(result?),
                        };
                        let outcome = if let Some(room) = room {
                            tokio::select! {
                                _ = stopped.changed() => Ok(()),
                                result = run(room, status.clone(), connections.clone(), engine.clone(), updates) => result,
                            }
                        } else { Ok(()) };
                        if *stopped.borrow() && settings.has_attempt() { crate::audience_relay_http::cleanup(&client, &settings).await?; }
                        outcome
                    } else {
                        tokio::select! {
                            _ = stopped.changed() => Ok(()),
                            result = run(config.ok_or("Missing relay configuration.")?, status.clone(), connections.clone(), engine.clone(), updates) => result,
                        }
                    }
                })
            }));
            connections.store(0, Ordering::Relaxed);
            engine.lock().unwrap_or_else(|e| e.into_inner()).stop();
            *status.lock().unwrap_or_else(|e| e.into_inner()) = match outcome {
                Ok(Ok(())) => SessionStatus::Stopped,
                Ok(Err(e)) => SessionStatus::Failed(e.into()),
                Err(_) => SessionStatus::Failed("Relay worker stopped unexpectedly.".into()),
            };
        })?;
        Ok(Self {
            status,
            connections,
            engine,
            stop,
            display,
            worker: Some(worker),
        })
    }
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            status: self
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            connections: self.connections.load(Ordering::Relaxed),
        }
    }
    pub fn drain_events(&self) -> Vec<AcceptedAudienceEvent> {
        self.engine
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(Instant::now())
    }
    pub fn set_paused(&self, paused: bool) -> u64 {
        let generation = self
            .engine
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .advance_generation();
        self.display.send_replace(Display { generation, paused });
        generation
    }
    pub fn comment_log_failed(&self) -> bool {
        self.engine
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .logger
            .as_ref()
            .is_some_and(|l| l.failed())
    }
    pub fn request_stop(&self) {
        self.engine.lock().unwrap_or_else(|e| e.into_inner()).stop();
        self.stop.send_replace(true);
    }
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub fn shutdown(&mut self) {
        self.request_stop();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
impl Drop for RemoteAudienceSession {
    fn drop(&mut self) {
        self.request_stop();
        if self.is_finished() {
            if let Some(w) = self.worker.take() {
                let _ = w.join();
            }
        }
    }
}
struct Cached {
    id: String,
    offer: Value,
    status: &'static str,
    received: Instant,
}
struct WireState {
    presenter: String,
    epoch: u128,
    generation: u64,
    paused: bool,
    confirmed: bool,
    revision: Option<u128>,
    cache: VecDeque<Cached>,
}
impl WireState {
    fn display(&mut self, display: Display) -> Result<Value, &'static str> {
        self.epoch += 1;
        if self.epoch.to_string().len() > 20 {
            return Err("Relay display epoch exhausted.");
        }
        self.generation = display.generation;
        self.paused = display.paused;
        self.confirmed = false;
        Ok(
            json!({"protocol":"qp.presenter","version":VERSION,"type":"display","presenter_epoch":self.presenter,"display_epoch":self.epoch.to_string(),"paused":self.paused}),
        )
    }
    fn process(
        &mut self,
        text: &str,
        engine: &mut AudienceEngine,
        connections: &AtomicUsize,
        received: Instant,
    ) -> Result<Option<Value>, &'static str> {
        match incoming(text)? {
            Incoming::State {
                revision,
                display_epoch,
                available,
                audience_connections,
                ..
            } => {
                let revision = counter(&revision).unwrap();
                if self.revision.is_none_or(|old| revision > old) {
                    self.revision = Some(revision);
                    self.confirmed = available && counter(&display_epoch) == Some(self.epoch);
                    connections.store(
                        usize::try_from(audience_connections).unwrap_or(usize::MAX),
                        Ordering::Relaxed,
                    );
                }
                Ok(None)
            }
            Incoming::Event {
                event_id,
                connection_id,
                presenter_epoch,
                display_epoch,
                offer_ttl_ms,
                payload,
                ..
            } => {
                let offer = strict_json(text.as_bytes())?;
                self.cache
                    .retain(|c| received.saturating_duration_since(c.received) < DEDUP_TTL);
                let status = if presenter_epoch != self.presenter {
                    "stopped"
                } else if let Some(old) = self.cache.iter().find(|c| c.id == event_id) {
                    if old.offer != offer {
                        return Err("Relay changed a duplicate event.");
                    }
                    old.status
                } else if self.cache.len() >= DEDUP_CAPACITY {
                    "busy"
                } else {
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    connection_id.hash(&mut hasher);
                    let generation = if self.confirmed
                        && !self.paused
                        && counter(&display_epoch) == Some(self.epoch)
                    {
                        self.generation
                    } else {
                        u64::MAX
                    };
                    let now = Instant::now();
                    let deadline = received + Duration::from_millis(offer_ttl_ms);
                    let admission = if now >= deadline {
                        Admission::Busy
                    } else {
                        engine.accept_relay(
                            payload.event()?,
                            hasher.finish(),
                            received,
                            deadline,
                            generation,
                        )
                    };
                    let status = match admission {
                        Admission::Accepted => "accepted",
                        Admission::Stopped => "stopped",
                        Admission::Busy | Admission::RateLimited => "busy",
                    };
                    self.cache.push_back(Cached {
                        id: event_id.clone(),
                        offer,
                        status,
                        received,
                    });
                    status
                };
                Ok(Some(
                    json!({"protocol":"qp.presenter","version":VERSION,"type":"receipt","event_id":event_id,"presenter_epoch":presenter_epoch,"status":status}),
                ))
            }
            _ => Err("Unexpected relay welcome."),
        }
    }
}
async fn run(
    config: RelayConfig,
    status: Arc<Mutex<SessionStatus>>,
    connections: Arc<AtomicUsize>,
    engine: Arc<Mutex<AudienceEngine>>,
    mut updates: watch::Receiver<Display>,
) -> Result<(), &'static str> {
    let socket_config = WebSocketConfig::default()
        .max_message_size(Some(MAX_BYTES))
        .max_frame_size(Some(MAX_BYTES));
    let (mut socket, _) = tokio::time::timeout(
        TIMEOUT,
        tokio_tungstenite::connect_async_with_config(
            &config.presenter_url,
            Some(socket_config),
            true,
        ),
    )
    .await
    .map_err(|_| "Relay connection timed out.")?
    .map_err(|_| "Cannot connect to relay.")?;
    let hello = json!({"protocol":"qp.presenter","version":VERSION,"type":"hello","token":config.presenter_token});
    tokio::time::timeout(
        SEND_TIMEOUT,
        socket.send(Message::Text(hello.to_string().into())),
    )
    .await
    .map_err(|_| "Relay send timed out.")?
    .map_err(|_| "Relay connection closed.")?;
    let welcome = tokio::time::timeout(TIMEOUT, async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => return incoming(&text),
                Some(Ok(Message::Ping(_))) => {
                    socket
                        .flush()
                        .await
                        .map_err(|_| "Relay connection closed.")?;
                }
                _ => return Err("Relay authentication failed."),
            }
        }
    })
    .await
    .map_err(|_| "Relay authentication timed out.")??;
    let Incoming::Welcome {
        presenter_epoch,
        display_epoch,
        audience_connections,
        ..
    } = welcome
    else {
        return Err("Relay did not send a presenter welcome.");
    };
    connections.store(
        usize::try_from(audience_connections).unwrap_or(usize::MAX),
        Ordering::Relaxed,
    );
    let mut state = WireState {
        presenter: presenter_epoch,
        epoch: counter(&display_epoch).unwrap(),
        generation: 0,
        paused: false,
        confirmed: false,
        revision: None,
        cache: VecDeque::new(),
    };
    let display = *updates.borrow_and_update();
    let initial = state.display(display)?;
    tokio::time::timeout(
        SEND_TIMEOUT,
        socket.send(Message::Text(initial.to_string().into())),
    )
    .await
    .map_err(|_| "Relay send timed out.")?
    .map_err(|_| "Relay connection closed.")?;
    *status.lock().unwrap_or_else(|e| e.into_inner()) = SessionStatus::Running {
        url: config.join_url,
        code: config.code,
    };
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    let mut last_received = Instant::now();
    loop {
        let outgoing = tokio::select! {
            _ = updates.changed() => { let display = *updates.borrow_and_update(); Some(Message::Text(state.display(display)?.to_string().into())) }
            _ = heartbeat.tick() => {
                if last_received.elapsed() >= Duration::from_secs(45) { return Err("Relay heartbeat timed out."); }
                Some(Message::Ping(Vec::new().into()))
            }
            message = socket.next() => {
                last_received = Instant::now();
                match message {
                    Some(Ok(Message::Text(text))) => state.process(&text, &mut engine.lock().unwrap_or_else(|e| e.into_inner()), &connections, last_received)?.map(|v| Message::Text(v.to_string().into())),
                    Some(Ok(Message::Ping(_))) => { tokio::time::timeout(SEND_TIMEOUT, socket.flush()).await.map_err(|_| "Relay send timed out.")?.map_err(|_| "Relay connection closed.")?; None }
                    Some(Ok(Message::Pong(_))) => None,
                    Some(Ok(Message::Close(_))) | None => return Err("Relay disconnected. Start again to resume; events are not replayed."),
                    _ => return Err("Invalid relay WebSocket message."),
                }
            }
        };
        if let Some(message) = outgoing {
            tokio::time::timeout(SEND_TIMEOUT, socket.send(message))
                .await
                .map_err(|_| "Relay send timed out.")?
                .map_err(|_| "Relay connection closed.")?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> WireState {
        WireState {
            presenter: "7".into(),
            epoch: 11,
            generation: 0,
            paused: false,
            confirmed: true,
            revision: None,
            cache: VecDeque::new(),
        }
    }
    fn offer(id: &str, epoch: &str, display: &str, comment: bool) -> String {
        json!({"protocol":"qp.presenter","version":VERSION,"type":"event","event_id":id,"connection_id":"a1","request_id":"r1","presenter_epoch":epoch,"display_epoch":display,"offer_ttl_ms":2000,
            "payload":if comment { json!({"type":"comment","body":"Hello\n世界","handle":" Anonymous "}) } else { json!({"type":"reaction","kind":"heart"}) }}).to_string()
    }
    fn snapshot(revision: &str, display: &str, count: u64) -> String {
        json!({"protocol":"qp.presenter","version":VERSION,"type":"state","revision":revision,"display_epoch":display,"available":true,"audience_connections":count}).to_string()
    }
    fn config(port: u16) -> Value {
        json!({"room_id":"test","code":"TEST","join_url":format!("http://127.0.0.1:{port}/join/test#k={}", "A".repeat(43)),"presenter_url":format!("ws://127.0.0.1:{port}/presenter"),"presenter_token":"B".repeat(43)})
    }

    #[test]
    fn strict_json_accepts_exact_byte_limit_and_rejects_escaped_duplicate_keys() {
        let exact = format!("\"{}\"", "x".repeat(MAX_BYTES - 2));
        assert_eq!(
            strict_json(exact.as_bytes()).unwrap(),
            json!("x".repeat(MAX_BYTES - 2))
        );
        assert_eq!(
            strict_json(format!("{exact} ").as_bytes()).err(),
            Some("Relay message exceeds size limit.")
        );
        for text in [
            r#"{"name":1,"\u006eame":2}"#,
            r#"[{"nested":{"x":1,"\u0078":2}}]"#,
            "{} {}",
        ] {
            assert_eq!(
                strict_json(text.as_bytes()).err(),
                Some("Malformed relay JSON.")
            );
        }
        assert_eq!(
            strict_json(br#"[null,true,false,1,-2,1.5,"text",{"x":1}]"#).unwrap(),
            json!([null, true, false, 1, -2, 1.5, "text", {"x":1}])
        );
    }

    #[test]
    fn relay_snapshot_counters_are_canonical_and_connections_are_safe_integers() {
        for kind in ["presenter_welcome", "state"] {
            let counter_key = if kind == "state" {
                "revision"
            } else {
                "presenter_epoch"
            };
            let mut base = json!({"protocol":"qp.presenter","version":VERSION,"type":kind,"display_epoch":"0","audience_connections":9_007_199_254_740_991u64});
            base[counter_key] = json!("99999999999999999999");
            if kind == "state" {
                base["available"] = json!(true);
            }
            assert!(incoming(&base.to_string()).is_ok());
            for value in [
                json!(""),
                json!("00"),
                json!("01"),
                json!("-1"),
                json!("1.0"),
                json!("100000000000000000000"),
                json!(1),
            ] {
                for key in [counter_key, "display_epoch"] {
                    let mut invalid = base.clone();
                    invalid[key] = value.clone();
                    assert!(
                        incoming(&invalid.to_string()).is_err(),
                        "{kind}: {key}={value}"
                    );
                }
            }
            base["audience_connections"] = json!(9_007_199_254_740_992u64);
            assert!(incoming(&base.to_string()).is_err());
        }
    }

    #[test]
    fn relay_offer_identifiers_payloads_and_ttl_have_strict_boundaries() {
        let base: Value = serde_json::from_str(&offer("e1", "7", "11", true)).unwrap();
        for key in ["event_id", "connection_id", "request_id"] {
            let mut valid = base.clone();
            valid[key] = json!("x".repeat(64));
            assert!(incoming(&valid.to_string()).is_ok());
            for id in [String::new(), "x".repeat(65), "é".into(), "a/b".into()] {
                let mut invalid = base.clone();
                invalid[key] = json!(id);
                assert!(incoming(&invalid.to_string()).is_err(), "{key}: {id:?}");
            }
        }
        for ttl in [1, 2000] {
            let mut valid = base.clone();
            valid["offer_ttl_ms"] = json!(ttl);
            assert!(incoming(&valid.to_string()).is_ok());
        }
        for payload in [
            json!({"type":"comment","body":"Hello"}),
            json!({"type":"comment","body":"Hello","handle":"","extra":true}),
            json!({"type":"comment","body":"Hello","handle":"x".repeat(33)}),
            json!({"type":"reaction","kind":"unknown"}),
        ] {
            let mut invalid = base.clone();
            invalid["payload"] = payload;
            assert!(incoming(&invalid.to_string()).is_err());
        }
    }

    #[test]
    fn busy_receipt_is_cached_after_budget_refills_without_enqueuing_again() {
        let now = Instant::now();
        let mut engine = AudienceEngine::new(now);
        let mut wire = state();
        let connections = AtomicUsize::new(0);
        for id in 0..30 {
            let receipt = wire
                .process(
                    &offer(&format!("e{id}"), "7", "11", true),
                    &mut engine,
                    &connections,
                    now,
                )
                .unwrap()
                .unwrap();
            assert_eq!(receipt["status"], "accepted");
        }
        let text = offer("limited", "7", "11", true);
        let receipt = wire
            .process(&text, &mut engine, &connections, now)
            .unwrap()
            .unwrap();
        assert_eq!(receipt["status"], "busy");
        assert_eq!(receipt["event_id"], "limited");
        assert_eq!(receipt["presenter_epoch"], "7");
        let later = now + Duration::from_secs(1);
        let events: Vec<_> = engine
            .drain(later)
            .into_iter()
            .chain(engine.drain(later))
            .collect();
        assert_eq!(events.len(), 30);
        assert_eq!(
            wire.process(&text, &mut engine, &connections, later)
                .unwrap(),
            Some(receipt)
        );
        assert!(engine.drain(later).is_empty());
        assert_eq!(
            wire.process(
                &offer("fresh", "7", "11", true),
                &mut engine,
                &connections,
                later
            )
            .unwrap()
            .unwrap()["status"],
            "accepted"
        );
        assert_eq!(engine.drain(later)[0].sequence, 31);
    }

    #[test]
    fn duplicate_cache_expires_at_exact_ttl_and_allows_new_admission() {
        let now = Instant::now();
        let mut engine = AudienceEngine::new(now);
        let mut wire = state();
        let connections = AtomicUsize::new(0);
        let text = offer("e1", "7", "11", true);
        wire.process(&text, &mut engine, &connections, now).unwrap();
        assert_eq!(engine.drain(now).len(), 1);
        let before = now + DEDUP_TTL - Duration::from_nanos(1);
        wire.process(&text, &mut engine, &connections, before)
            .unwrap();
        assert!(engine.drain(before).is_empty());
        let boundary = now + DEDUP_TTL;
        wire.process(&text, &mut engine, &connections, boundary)
            .unwrap();
        let events = engine.drain(boundary);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, 2);
        assert_eq!(events[0].received_at, boundary);
        assert_eq!(wire.cache.len(), 1);
    }

    #[test]
    fn unavailable_state_revokes_display_confirmation_and_equal_revision_is_ignored() {
        let now = Instant::now();
        let mut engine = AudienceEngine::new(now);
        let mut wire = state();
        let connections = AtomicUsize::new(0);
        wire.process(&snapshot("8", "11", 3), &mut engine, &connections, now)
            .unwrap();
        let mut unavailable: Value = serde_json::from_str(&snapshot("9", "11", 1)).unwrap();
        unavailable["available"] = json!(false);
        wire.process(&unavailable.to_string(), &mut engine, &connections, now)
            .unwrap();
        wire.process(&snapshot("9", "11", 99), &mut engine, &connections, now)
            .unwrap();
        assert_eq!(connections.load(Ordering::Relaxed), 1);
        wire.process(
            &offer("unavailable", "7", "11", true),
            &mut engine,
            &connections,
            now,
        )
        .unwrap();
        assert_eq!(engine.drain(now)[0].display_generation, u64::MAX);
        wire.process(&snapshot("10", "11", 2), &mut engine, &connections, now)
            .unwrap();
        wire.process(
            &offer("available", "7", "11", true),
            &mut engine,
            &connections,
            now,
        )
        .unwrap();
        assert_eq!(engine.drain(now)[0].display_generation, 0);
        assert_eq!(connections.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn display_epoch_exhaustion_fails_before_emitting_invalid_wire_counter() {
        let mut wire = state();
        wire.epoch = 99_999_999_999_999_999_998;
        let message = wire
            .display(Display {
                generation: 4,
                paused: true,
            })
            .unwrap();
        assert_eq!(message["display_epoch"], "99999999999999999999");
        assert_eq!(message["paused"], true);
        assert_eq!(wire.generation, 4);
        assert!(!wire.confirmed);
        assert_eq!(
            wire.display(Display::default()).err(),
            Some("Relay display epoch exhausted.")
        );
    }

    #[test]
    fn strict_wire_rejects_duplicates_invalid_bounds_and_versions() {
        for bad in [
            r#"{"x":1,"x":2}"#,
            r#"{"a":{"x":1,"x":2}}"#,
            r#"{"a":[{"x":1,"x":2}]}"#,
            r#"{"x":"\ud800"}"#,
        ] {
            assert!(strict_json(bad.as_bytes()).is_err());
        }
        assert!(strict_json(&vec![b' '; 4097]).is_err());
        let base: Value = serde_json::from_str(&offer("e1", "7", "11", true)).unwrap();
        for (key, value) in [
            ("version", json!("1")),
            ("protocol", json!("qp.audience")),
            ("display_epoch", json!("01")),
            ("presenter_epoch", json!(7)),
            ("event_id", json!("bad!")),
            ("offer_ttl_ms", json!(0)),
            ("offer_ttl_ms", json!(2001)),
            ("extra", json!(true)),
        ] {
            let mut v = base.clone();
            v[key] = value;
            assert!(incoming(&v.to_string()).is_err(), "{key}");
        }
        for body in [" ", "\u{7f}", "\u{85}", &"😀".repeat(201)] {
            let mut v = base.clone();
            v["payload"]["body"] = json!(body);
            assert!(incoming(&v.to_string()).is_err());
        }
        assert!(incoming(&base.to_string()).is_ok());
        assert_eq!(
            counter("99999999999999999999"),
            Some(99_999_999_999_999_999_999)
        );
        assert!(counter("100000000000000000000").is_none());
    }
    #[test]
    fn config_validates_origin_tokens_and_insecure_transport() {
        let base = config(1234);
        assert!(serde_json::from_value::<RelayConfig>(base.clone())
            .unwrap()
            .validate()
            .is_ok());
        for (key, value) in [
            ("presenter_token", json!("bad")),
            ("room_id", json!("other")),
            ("join_url", json!("http://127.0.0.1:1234/join/test#k=bad")),
            ("presenter_url", json!("ws://127.0.0.1:4321/presenter")),
            (
                "presenter_url",
                json!("ws://user:pass@127.0.0.1:1234/presenter"),
            ),
            (
                "presenter_url",
                json!("ws://127.0.0.1:1234/presenter?k=secret"),
            ),
        ] {
            let mut v = base.clone();
            v[key] = value;
            assert!(serde_json::from_value::<RelayConfig>(v)
                .unwrap()
                .validate()
                .is_err());
        }
        let mut lan = base.clone();
        lan["join_url"] = json!(format!(
            "http://192.168.1.2:1234/join/test#k={}",
            "A".repeat(43)
        ));
        lan["presenter_url"] = json!("ws://192.168.1.2:1234/presenter");
        assert!(serde_json::from_value::<RelayConfig>(lan)
            .unwrap()
            .validate()
            .is_err());
        let mut ipv6 = base.clone();
        ipv6["join_url"] = json!(format!("http://[::1]:1234/join/test#k={}", "A".repeat(43)));
        ipv6["presenter_url"] = json!("ws://[::1]:1234/presenter");
        assert!(serde_json::from_value::<RelayConfig>(ipv6)
            .unwrap()
            .validate()
            .is_ok());
        let mut secure = base;
        secure["join_url"] = json!(format!(
            "https://relay.example/join/test#k={}",
            "A".repeat(43)
        ));
        secure["presenter_url"] = json!("wss://relay.example/presenter");
        assert!(serde_json::from_value::<RelayConfig>(secure)
            .unwrap()
            .validate()
            .is_ok());
    }
    #[test]
    fn receipt_requires_admission_and_duplicates_do_not_repeat_history() {
        let now = Instant::now();
        let mut engine = AudienceEngine::new(now);
        let mut s = state();
        let count = AtomicUsize::new(0);
        let text = offer("e1", "7", "11", true);
        assert_eq!(
            s.process(&text, &mut engine, &count, now).unwrap().unwrap()["status"],
            "accepted"
        );
        assert_eq!(
            s.process(&text, &mut engine, &count, now).unwrap().unwrap()["status"],
            "accepted"
        );
        let events = engine.drain(now);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].event,
            AudienceEvent::Comment {
                body: "Hello 世界".into(),
                handle: " Anonymous ".into()
            }
        );
        let changed = text.replace("Hello", "Changed");
        assert!(s.process(&changed, &mut engine, &count, now).is_err());
        assert_eq!(
            s.process(&offer("e2", "6", "11", false), &mut engine, &count, now)
                .unwrap()
                .unwrap()["status"],
            "stopped"
        );
        engine.stop();
        assert_eq!(
            s.process(&offer("e3", "7", "11", false), &mut engine, &count, now)
                .unwrap()
                .unwrap()["status"],
            "stopped"
        );
    }
    #[test]
    fn pause_resume_ack_and_stale_state_fence_comment_overlay() {
        let now = Instant::now();
        let mut engine = AudienceEngine::new(now);
        let mut s = state();
        let count = AtomicUsize::new(0);
        s.display(Display {
            generation: 1,
            paused: true,
        })
        .unwrap();
        s.process(&offer("paused", "7", "12", true), &mut engine, &count, now)
            .unwrap();
        s.display(Display {
            generation: 2,
            paused: false,
        })
        .unwrap();
        s.process(
            &offer("unconfirmed", "7", "13", true),
            &mut engine,
            &count,
            now,
        )
        .unwrap();
        s.process(&snapshot("8", "13", 3), &mut engine, &count, now)
            .unwrap();
        s.process(&snapshot("7", "12", 1), &mut engine, &count, now)
            .unwrap();
        assert_eq!(count.load(Ordering::Relaxed), 3);
        s.process(&offer("old", "7", "12", true), &mut engine, &count, now)
            .unwrap();
        s.process(&offer("fresh", "7", "13", true), &mut engine, &count, now)
            .unwrap();
        let events = engine.drain(now);
        assert_eq!(
            events
                .iter()
                .map(|e| e.display_generation)
                .collect::<Vec<_>>(),
            vec![u64::MAX, u64::MAX, u64::MAX, 2]
        );
        let mut comments = crate::audience_comments::CommentEngine::default();
        comments.set_paused(false, 2);
        for e in events {
            comments.receive(&e, 100.0, now);
        }
        assert_eq!(comments.history().len(), 4);
        assert_eq!(comments.frames(now).len(), 1);
    }
    #[test]
    fn expired_offers_and_short_overlay_deadlines_are_not_renewed() {
        let now = Instant::now();
        let mut engine = AudienceEngine::new(now);
        let mut s = state();
        let count = AtomicUsize::new(0);
        assert_eq!(
            s.process(
                &offer("expired", "7", "11", false),
                &mut engine,
                &count,
                now - Duration::from_secs(3)
            )
            .unwrap()
            .unwrap()["status"],
            "busy"
        );
        let mut v: Value = serde_json::from_str(&offer("short", "7", "11", true)).unwrap();
        v["offer_ttl_ms"] = json!(100);
        s.process(&v.to_string(), &mut engine, &count, Instant::now())
            .unwrap();
        let events = engine.drain(Instant::now());
        assert_eq!(events.len(), 1);
        let mut comments = crate::audience_comments::CommentEngine::default();
        comments.receive(&events[0], 100.0, events[0].overlay_deadline);
        assert_eq!(comments.history().len(), 1);
        assert!(comments.frames(Instant::now()).is_empty());
        s.cache = (0..DEDUP_CAPACITY)
            .map(|i| Cached {
                id: i.to_string(),
                offer: Value::Null,
                status: "accepted",
                received: now,
            })
            .collect();
        assert_eq!(
            s.process(&offer("full", "7", "11", false), &mut engine, &count, now)
                .unwrap()
                .unwrap()["status"],
            "busy"
        );
        assert!(engine.drain(now).is_empty());
    }
    #[test]
    fn socket_path_authenticates_delivers_receipts_and_stops_without_replay() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("relay.json");
        let mut file = File::options()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .unwrap();
        }
        std::io::Write::write_all(&mut file, config(port).to_string().as_bytes()).unwrap();
        let (signal, ready) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            let hello: Value = serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(hello["token"], "B".repeat(43));
            ws.send(tungstenite::Message::Text(json!({"protocol":"qp.presenter","version":VERSION,"type":"presenter_welcome","presenter_epoch":"7","display_epoch":"10","audience_connections":2}).to_string().into())).unwrap();
            let display: Value =
                serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(display["display_epoch"], "11");
            ws.send(tungstenite::Message::Text(snapshot("1", "11", 2).into()))
                .unwrap();
            ws.send(tungstenite::Message::Text(
                offer("e1", "7", "11", true).into(),
            ))
            .unwrap();
            loop {
                let msg = ws.read().unwrap();
                if msg.is_text() {
                    let receipt: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
                    assert_eq!(receipt["status"], "accepted");
                    break;
                }
            }
            signal.send(()).unwrap();
            loop {
                match ws.read() {
                    Ok(m) if m.is_text() => {
                        let v: Value = serde_json::from_str(m.to_text().unwrap()).unwrap();
                        assert_eq!(v["type"], "display");
                        assert_eq!(v["paused"], true);
                        signal.send(()).unwrap();
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
        let mut session = RemoteAudienceSession::start(path).unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            session.snapshot().status,
            SessionStatus::Running { .. }
        ));
        assert_eq!(session.snapshot().connections, 2);
        assert_eq!(session.drain_events().len(), 1);
        assert_eq!(session.set_paused(true), 1);
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let start = Instant::now();
        session.shutdown();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(session.is_finished());
        assert_eq!(session.snapshot().connections, 0);
        assert!(session.drain_events().is_empty());
        server.join().unwrap();
    }
    #[test]
    fn stop_interrupts_authentication_wait() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay.json");
        std::fs::write(&path, config(port).to_string()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let (hello, ready) = std::sync::mpsc::channel();
        let (release, done) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            socket.read().unwrap();
            hello.send(()).unwrap();
            done.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let mut session = RemoteAudienceSession::start(path).unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let start = Instant::now();
        session.shutdown();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(session.snapshot().status, SessionStatus::Stopped);
        release.send(()).unwrap();
        server.join().unwrap();
    }
    #[test]
    fn invalid_config_fails_without_disclosing_credentials() {
        let mut session =
            RemoteAudienceSession::start(PathBuf::from("/does-not-exist/secret.json")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            matches!(session.snapshot().status,SessionStatus::Failed(ref message) if !message.contains("secret.json"))
        );
        session.shutdown();
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;
    #[test]
    #[ignore = "requires a running OSS relay runtime and QUICK_PRESENTER_TEST_CONFIG"]
    fn external_relay_room_creation_round_trip_and_termination() {
        let path = PathBuf::from(
            std::env::var_os("QUICK_PRESENTER_TEST_CONFIG")
                .expect("Set the relay fixture configuration path"),
        );
        let settings = crate::audience_relay_http::ManagedRelay::new(
            crate::app_config::load(Some(path))
                .unwrap()
                .expect("Remote TOML configuration required"),
        );
        let mut session = RemoteAudienceSession::start_managed(settings.clone()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(session.snapshot().status, SessionStatus::Running { .. })
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            session.snapshot().status,
            SessionStatus::Running { .. }
        ));
        let config = settings.attempt().unwrap().room.unwrap();
        let url = url::Url::parse(&config.join_url).unwrap();
        let endpoint = format!(
            "ws://{}/ws/audience/{}",
            url[url::Position::BeforeHost..url::Position::AfterPort].to_owned(),
            config.room_id
        );
        let (mut audience, _) = tungstenite::connect(endpoint).unwrap();
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = audience.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
        }
        audience.send(tungstenite::Message::Text(json!({"protocol":"qp.audience","version":VERSION,"type":"hello","token":url.fragment().unwrap().strip_prefix("k=").unwrap()}).to_string().into())).unwrap();
        fn result(
            ws: &mut tungstenite::WebSocket<
                tungstenite::stream::MaybeTlsStream<std::net::TcpStream>,
            >,
        ) -> Value {
            loop {
                let message = ws.read().unwrap();
                if let Ok(text) = message.to_text() {
                    let value: Value = serde_json::from_str(text).unwrap();
                    if value["type"] == "result" {
                        return value;
                    }
                }
            }
        }
        for (id, payload) in [
            ("r1", json!({"type":"reaction","kind":"heart"})),
            (
                "r2",
                json!({"type":"comment","body":"Hello 世界","handle":" Original "}),
            ),
        ] {
            let mut value = payload;
            value["protocol"] = json!("qp.audience");
            value["version"] = json!(VERSION);
            value["request_id"] = json!(id);
            audience
                .send(tungstenite::Message::Text(value.to_string().into()))
                .unwrap();
            assert_eq!(result(&mut audience)["status"], "accepted");
        }
        let events = session.drain_events();
        assert_eq!(events.len(), 2);
        assert_eq!(session.snapshot().connections, 1);
        let now = Instant::now();
        let mut overlay = crate::audience_overlay::OverlayEngine::default();
        overlay.push(&events[0], now);
        assert_eq!(overlay.frames(now).len(), 1);
        let mut comments = crate::audience_comments::CommentEngine::default();
        comments.receive(&events[1], 100.0, now);
        assert_eq!(comments.history().len(), 1);
        assert_eq!(comments.frames(now).len(), 1);
        assert_eq!(
            events[1].event,
            AudienceEvent::Comment {
                body: "Hello 世界".into(),
                handle: " Original ".into()
            }
        );
        session.shutdown();
        assert!(session.is_finished());
        assert!(settings.attempt.lock().unwrap().is_none());
        let mut resumed = RemoteAudienceSession::start_managed(settings.clone()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(resumed.snapshot().status, SessionStatus::Running { .. })
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            resumed.snapshot().status,
            SessionStatus::Running { .. }
        ));
        assert!(resumed.drain_events().is_empty());
        let next = settings.attempt().unwrap().room.unwrap();
        assert_ne!(config.room_id, next.room_id);
        assert_ne!(config.presenter_token, next.presenter_token);
        let endpoint = format!(
            "{}/ws/audience/{}",
            settings
                .endpoint
                .origin
                .as_str()
                .trim_end_matches('/')
                .replacen("http:", "ws:", 1),
            next.room_id
        );
        let (mut audience, _) = tungstenite::connect(endpoint).unwrap();
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = audience.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
        }
        let url = url::Url::parse(&next.join_url).unwrap();
        audience.send(tungstenite::Message::Text(json!({"protocol":"qp.audience","version":VERSION,"type":"hello","token":url.fragment().unwrap().strip_prefix("k=").unwrap()}).to_string().into())).unwrap();
        audience.send(tungstenite::Message::Text(json!({"protocol":"qp.audience","version":VERSION,"type":"reaction","request_id":"r3","kind":"applause"}).to_string().into())).unwrap();
        assert_eq!(result(&mut audience)["status"], "accepted");
        assert_eq!(resumed.drain_events().len(), 1);
        resumed.shutdown();
        let _ = audience.close(None);
    }
}

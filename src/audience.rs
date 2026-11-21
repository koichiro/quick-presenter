//! Optional local audience session. Network tasks never access Slint or PDF state.
use crate::audience_events::{
    parse_event, AcceptedAudienceEvent, Admission, AudienceEngine, RateBudget,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use std::{
    collections::VecDeque,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use tokio::sync::{watch, Semaphore};

const PROTOCOL: &str = "qp.audience";
const VERSION: &str = "1";
const REQUEST_CACHE: Duration = Duration::from_secs(30);
const MAX_REQUESTS: usize = 256;

const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const HEARTBEAT: Duration = Duration::from_secs(15);
const MAX_MESSAGE_BYTES: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SessionStatus {
    #[default]
    Starting,
    Running {
        url: String,
        code: String,
    },
    Stopped,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct SessionSnapshot {
    pub status: SessionStatus,
    pub connections: usize,
}

pub fn local_addresses() -> std::io::Result<Vec<Ipv4Addr>> {
    let mut addresses: Vec<_> = if_addrs::get_if_addrs()?
        .into_iter()
        .filter_map(|interface| match interface.ip() {
            IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
            _ => None,
        })
        .collect();
    addresses.sort();
    addresses.dedup();
    Ok(addresses)
}

pub struct LocalAudienceSession {
    status: Arc<Mutex<SessionStatus>>,
    connections: Arc<AtomicUsize>,
    stop: watch::Sender<bool>,
    worker: Option<JoinHandle<()>>,
    engine: Arc<Mutex<AudienceEngine>>,
}

impl LocalAudienceSession {
    pub fn start(address: Ipv4Addr) -> std::io::Result<Self> {
        let status = Arc::new(Mutex::new(SessionStatus::Starting));
        let connections = Arc::new(AtomicUsize::new(0));
        let (stop, receiver) = watch::channel(false);
        let engine = Arc::new(Mutex::new(AudienceEngine::new(Instant::now())));
        let worker_engine = engine.clone();
        let worker_status = status.clone();
        let worker_connections = connections.clone();
        let worker = thread::Builder::new()
            .name("audience-local".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?;
                    #[cfg(not(test))]
                    {
                        worker_engine
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .logger = Some(crate::audience_log::logger());
                    }
                    runtime.block_on(run(
                        address,
                        worker_status.clone(),
                        worker_connections,
                        receiver,
                        worker_engine,
                    ))
                });
                *worker_status.lock().unwrap_or_else(|e| e.into_inner()) = match result {
                    Ok(Ok(())) => SessionStatus::Stopped,
                    Ok(Err(error)) => SessionStatus::Failed(error.to_string()),
                    Err(_) => SessionStatus::Failed("Audience server stopped unexpectedly.".into()),
                };
            })?;
        Ok(Self {
            status,
            connections,
            stop,
            worker: Some(worker),
            engine,
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

    pub fn advance_comment_generation(&self) -> u64 {
        self.engine
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .advance_generation()
    }
    pub fn comment_log_failed(&self) -> bool {
        self.engine
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .logger
            .as_ref()
            .is_some_and(|logger| logger.failed())
    }

    pub fn request_stop(&self) {
        self.engine.lock().unwrap_or_else(|e| e.into_inner()).stop();
        self.stop.send_replace(true);
    }

    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Called only after the event loop exits; no synchronous work runs in tasks.
    pub fn shutdown(&mut self) {
        self.request_stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for LocalAudienceSession {
    fn drop(&mut self) {
        self.request_stop();
        if self.is_finished() {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

#[derive(Clone)]
struct ServerState {
    authority: String,
    code: String,
    secret: String,
    connections: Arc<AtomicUsize>,
    unauthenticated: Arc<Semaphore>,
    attempts: Arc<Mutex<HandshakeBudget>>,
    stop: watch::Receiver<bool>,
    engine: Arc<Mutex<AudienceEngine>>,
    next_participant: Arc<AtomicU64>,
}

fn credentials() -> std::io::Result<(String, String)> {
    let mut bytes = [0u8; 36];
    getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
    let encode = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok((
        encode(&bytes[..4]).to_uppercase(),
        URL_SAFE_NO_PAD.encode(&bytes[4..]),
    ))
}

async fn run(
    address: Ipv4Addr,
    status: Arc<Mutex<SessionStatus>>,
    connections: Arc<AtomicUsize>,
    mut stop: watch::Receiver<bool>,
    engine: Arc<Mutex<AudienceEngine>>,
) -> std::io::Result<()> {
    let (code, secret) = credentials()?;
    let listener = tokio::net::TcpListener::bind(SocketAddr::new(address.into(), 0)).await?;
    let authority = listener.local_addr()?.to_string();
    let url = format!("http://{authority}/join/{code}#k={secret}");
    let state = ServerState {
        authority,
        code: code.clone(),
        secret,
        connections,
        unauthenticated: Arc::new(Semaphore::new(32)),
        attempts: Arc::new(Mutex::new(HandshakeBudget::new(Instant::now()))),
        stop: stop.clone(),
        engine,
        next_participant: Arc::new(AtomicU64::new(1)),
    };
    let app = Router::new()
        .route("/join/{code}", get(join_page))
        .route("/ws/audience/{code}", get(upgrade))
        .route("/assets/audience.js", get(browser_script))
        .route("/assets/LICENSE", get(browser_license))
        .route("/assets/NOTICE", get(browser_notice))
        .with_state(state);
    *status.lock().unwrap_or_else(|e| e.into_inner()) = SessionStatus::Running { url, code };
    if *stop.borrow() {
        return Ok(());
    }
    tokio::select! {
        result = axum::serve(listener, app) => result,
        _ = stop.changed() => Ok(()),
    }
}

fn valid_headers(headers: &HeaderMap, authority: &str, websocket: bool) -> bool {
    headers.get(header::HOST).and_then(|h| h.to_str().ok()) == Some(authority)
        && (!websocket
            || headers.get(header::ORIGIN).and_then(|h| h.to_str().ok())
                == Some(format!("http://{authority}").as_str()))
}

async fn join_page(
    State(state): State<ServerState>,
    axum::extract::Path(code): axum::extract::Path<String>,
    headers: HeaderMap,
) -> Response {
    if !valid_headers(&headers, &state.authority, false) || code != state.code {
        return StatusCode::NOT_FOUND.into_response();
    }
    ([(header::CACHE_CONTROL, "no-store"), (header::REFERRER_POLICY, "no-referrer"),
        (header::CONTENT_SECURITY_POLICY, "default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'")],
        Html(include_str!("../assets/audience/index.html"))).into_response()
}

fn asset(
    headers: &HeaderMap,
    state: &ServerState,
    mime: &'static str,
    content: &'static str,
) -> Response {
    if !valid_headers(headers, &state.authority, false) {
        return StatusCode::NOT_FOUND.into_response();
    }
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        content,
    )
        .into_response()
}
async fn browser_script(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    asset(
        &headers,
        &state,
        "text/javascript; charset=utf-8",
        include_str!("../assets/audience/audience.js"),
    )
}
async fn browser_license(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    asset(
        &headers,
        &state,
        "text/plain; charset=utf-8",
        include_str!("../assets/audience/LICENSE"),
    )
}
async fn browser_notice(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    asset(
        &headers,
        &state,
        "text/plain; charset=utf-8",
        include_str!("../assets/audience/NOTICE"),
    )
}

async fn upgrade(
    State(state): State<ServerState>,
    axum::extract::Path(code): axum::extract::Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if code != state.code {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !valid_headers(&headers, &state.authority, true) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state
        .attempts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .allow(Instant::now())
    {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let Ok(permit) = state.unauthenticated.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move { connection(socket, state, permit).await })
        .into_response()
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Hello {
    protocol: String,
    version: String,
    r#type: String,
    token: String,
}

/// Global handshake budget bounds reconnect/invalid-token churn, not audience size.
struct HandshakeBudget {
    started: Instant,
    remaining: u32,
}
impl HandshakeBudget {
    fn new(now: Instant) -> Self {
        Self {
            started: now,
            remaining: 20,
        }
    }
    fn allow(&mut self, now: Instant) -> bool {
        if now.duration_since(self.started) >= Duration::from_secs(1) {
            *self = Self::new(now);
        }
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

fn authenticated(text: &str, secret: &str) -> bool {
    crate::audience_wire::strict_json(text.as_bytes())
        .ok()
        .and_then(|value| serde_json::from_value::<Hello>(value).ok())
        .is_some_and(|hello| {
            hello.protocol == PROTOCOL
                && hello.version == VERSION
                && hello.r#type == "hello"
                && hello.token == secret
        })
}

fn audience_input(
    text: &str,
) -> Option<(
    String,
    crate::audience_events::AudienceEvent,
    serde_json::Value,
)> {
    let mut value = crate::audience_wire::strict_json(text.as_bytes()).ok()?;
    let fingerprint = value.clone();
    let fields = value.as_object_mut()?;
    if fields.remove("protocol")? != PROTOCOL
        || fields.remove("version")? != VERSION
        || fields.contains_key("v")
    {
        return None;
    }
    fields.insert("v".into(), serde_json::json!(1));
    let (id, event) = parse_event(&value.to_string())?;
    Some((id, event, fingerprint))
}

struct CachedResult {
    id: String,
    fingerprint: serde_json::Value,
    finished: Instant,
    status: &'static str,
}
#[derive(Default)]
struct RequestCache(VecDeque<CachedResult>);
impl RequestCache {
    fn expire(&mut self, now: Instant) {
        self.0
            .retain(|entry| now.saturating_duration_since(entry.finished) < REQUEST_CACHE);
    }
    fn check(
        &mut self,
        id: &str,
        fingerprint: &serde_json::Value,
        now: Instant,
    ) -> Result<Option<&'static str>, u16> {
        self.expire(now);
        if let Some(entry) = self.0.iter().find(|entry| entry.id == id) {
            return if &entry.fingerprint == fingerprint {
                Ok(Some(entry.status))
            } else {
                Err(1002)
            };
        }
        if self.0.len() >= MAX_REQUESTS {
            return Err(1013);
        }
        Ok(None)
    }
    fn insert(
        &mut self,
        id: String,
        fingerprint: serde_json::Value,
        status: &'static str,
        now: Instant,
    ) {
        self.0.push_back(CachedResult {
            id,
            fingerprint,
            finished: now,
            status,
        });
    }
}
fn wire_status(admission: Admission) -> &'static str {
    match admission {
        Admission::Accepted => "accepted",
        Admission::Stopped => "stopped",
        Admission::Busy | Admission::RateLimited => "busy",
    }
}
async fn close(socket: &mut WebSocket, code: u16) {
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        socket.send(Message::Close(Some(CloseFrame {
            code,
            reason: "".into(),
        }))),
    )
    .await;
}

struct CountGuard(Arc<AtomicUsize>);
impl Drop for CountGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

async fn connection(
    mut socket: WebSocket,
    state: ServerState,
    permit: tokio::sync::OwnedSemaphorePermit,
) {
    let mut stop = state.stop.clone();
    let first = tokio::select! {
        first = tokio::time::timeout(AUTH_TIMEOUT, socket.recv()) => first,
        _ = stop.changed() => return,
    };
    let Ok(Some(Ok(Message::Text(text)))) = first else {
        close(&mut socket, 1008).await;
        return;
    };
    if !authenticated(&text, &state.secret) || *stop.borrow() {
        close(&mut socket, 1008).await;
        return;
    }
    drop(permit);
    state.connections.fetch_add(1, Ordering::Relaxed);
    let _count = CountGuard(state.connections.clone());
    let participant_id = state.next_participant.fetch_add(1, Ordering::Relaxed);
    if !send_json(
        &mut socket,
        serde_json::json!({"protocol":PROTOCOL,"version":VERSION,"type":"welcome","connection_id":participant_id.to_string(),"available":true,"capabilities":["reactions","comments"],"limits":{"max_message_bytes":MAX_MESSAGE_BYTES,"request_cache_ms":30000}}),
    )
    .await
    {
        return;
    }
    let mut requests = RequestCache::default();
    let mut budget = RateBudget::new(5, 2, Instant::now());
    let mut comment_budget = RateBudget::new(2, 1, Instant::now());
    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    heartbeat.tick().await;
    let mut waiting_for_pong = false;
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            _ = heartbeat.tick() => {
                requests.expire(Instant::now());
                if waiting_for_pong { break; }
                waiting_for_pong = true;
                if !matches!(tokio::time::timeout(Duration::from_secs(2), socket.send(Message::Ping(Vec::new().into()))).await, Ok(Ok(()))) { break; }
            }
            message = socket.recv() => match message {
                Some(Ok(Message::Pong(_))) => waiting_for_pong = false,
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(Message::Text(text))) => {
                    let Some((request_id, event, fingerprint)) = audience_input(&text) else { close(&mut socket, 1002).await; break; };
                    let now = Instant::now();
                    match requests.check(&request_id, &fingerprint, now) {
                        Ok(Some(status)) => {
                            if !send_json(&mut socket, serde_json::json!({"protocol":PROTOCOL,"version":VERSION,"type":"result","request_id":request_id,"status":status})).await { break; }
                            continue;
                        },
                        Err(code) => { close(&mut socket, code).await; break; },
                        Ok(None) => (),
                    }
                    let status = if budget.allow(now) && (!matches!(event, crate::audience_events::AudienceEvent::Comment { .. }) || comment_budget.allow(now)) {
                        state.engine.lock().unwrap_or_else(|e| e.into_inner()).accept_event(event, participant_id, now)
                    } else { Admission::RateLimited };
                    let status = wire_status(status);
                    requests.insert(request_id.clone(), fingerprint, status, now);
                    if !send_json(&mut socket, serde_json::json!({"protocol":PROTOCOL,"version":VERSION,"type":"result","request_id":request_id,"status":status})).await { break; }
                },
                Some(Ok(Message::Ping(_))) => (),
                _ => { close(&mut socket, 1002).await; break; },
            }
        }
    }
}

async fn send_json(socket: &mut WebSocket, value: serde_json::Value) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            socket.send(Message::Text(value.to_string().into()))
        )
        .await,
        Ok(Ok(()))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use tungstenite::client::IntoClientRequest;

    fn wait_until(mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !condition() {
            assert!(Instant::now() < deadline, "audience operation timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn ready(session: &LocalAudienceSession) -> (String, String) {
        wait_until(|| !matches!(session.snapshot().status, SessionStatus::Starting));
        let SessionStatus::Running { url, .. } = session.snapshot().status else {
            panic!("server did not start");
        };
        let (url, token) = url.split_once("#k=").unwrap();
        (url.to_owned(), token.to_owned())
    }

    fn connect(url: &str) -> tungstenite::WebSocket<std::net::TcpStream> {
        let authority = url
            .strip_prefix("http://")
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        let mut request = format!(
            "ws://{authority}/ws/audience/{}",
            url.rsplit('/').next().unwrap()
        )
        .into_client_request()
        .unwrap();
        request
            .headers_mut()
            .insert("Origin", format!("http://{authority}").parse().unwrap());
        let stream = std::net::TcpStream::connect(authority).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        tungstenite::client(request, stream).unwrap().0
    }

    #[test]
    fn session_serves_page_authenticates_counts_and_closes_on_stop() {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        let (url, token) = ready(&session);
        let authority = url
            .strip_prefix("http://")
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        let path = url.strip_prefix(&format!("http://{authority}")).unwrap();
        let mut http = std::net::TcpStream::connect(authority).unwrap();
        http.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        write!(
            http,
            "GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        http.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("Quick Presenter"));
        assert!(!response.contains(&token));
        let mut rejected = connect(&url);
        rejected
            .send(tungstenite::Message::Text(
                r#"{"protocol":"qp.audience","version":"1","type":"hello","token":"wrong"}"#.into(),
            ))
            .unwrap();
        assert!(matches!(
            rejected.read().unwrap(),
            tungstenite::Message::Close(_)
        ));
        assert_eq!(session.snapshot().connections, 0);
        let mut ws = connect(&url);
        ws.send(tungstenite::Message::Text(
            serde_json::json!({"protocol":"qp.audience","version":"1","type":"hello","token":token}).to_string().into(),
        ))
        .unwrap();
        let welcome = ws.read().unwrap().into_text().unwrap();
        assert!(welcome.contains("welcome"));
        assert_eq!(session.snapshot().connections, 1);
        ws.close(None).unwrap();
        wait_until(|| session.snapshot().connections == 0);
        let mut ws = connect(&url);
        ws.send(tungstenite::Message::Text(
            serde_json::json!({"protocol":"qp.audience","version":"1","type":"hello","token":token}).to_string().into(),
        ))
        .unwrap();
        ws.read().unwrap();
        session.shutdown();
        assert!(ws.read().is_err());
        assert_eq!(session.snapshot().connections, 0);
        assert_eq!(session.snapshot().status, SessionStatus::Stopped);
        let _rebound = std::net::TcpListener::bind(authority).unwrap();
        let mut next = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        let (_, next_token) = ready(&next);
        assert_ne!(token, next_token);
        next.shutdown();
    }

    #[test]
    fn authenticated_reactions_are_always_accepted_while_session_runs() {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        let (url, token) = ready(&session);
        let mut ws = connect(&url);
        ws.send(tungstenite::Message::Text(
            serde_json::json!({"protocol":"qp.audience","version":"1","type":"hello","token":token}).to_string().into(),
        ))
        .unwrap();
        let welcome: serde_json::Value =
            serde_json::from_str(&ws.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(
            welcome["capabilities"],
            serde_json::json!(["reactions", "comments"])
        );
        for id in 0..6 {
            ws.send(tungstenite::Message::Text(serde_json::json!({"protocol":"qp.audience","version":"1","type":"reaction","request_id":id.to_string(),"kind":"applause"}).to_string().into())).unwrap();
            let reply: serde_json::Value =
                serde_json::from_str(&ws.read().unwrap().into_text().unwrap()).unwrap();
            assert_eq!(reply["status"], if id < 5 { "accepted" } else { "busy" });
        }
        let events = session.drain_events();
        assert_eq!(events.len(), 5);
        assert!(events.iter().all(|e| e.event
            == crate::audience_events::AudienceEvent::Reaction {
                kind: crate::audience_events::ReactionKind::Applause
            }));
        let mut second = connect(&url);
        second
            .send(tungstenite::Message::Text(
                serde_json::json!({"protocol":"qp.audience","version":"1","type":"hello","token":token}).to_string().into(),
            ))
            .unwrap();
        second.read().unwrap();
        second
            .send(tungstenite::Message::Text(
                r#"{"protocol":"qp.audience","version":"1","type":"reaction","request_id":"second","kind":"heart"}"#.into(),
            ))
            .unwrap();
        assert!(second
            .read()
            .unwrap()
            .into_text()
            .unwrap()
            .contains("accepted"));
        session.request_stop();
        assert!(session.drain_events().is_empty());
        session.shutdown();
        let mut next = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        ready(&next);
        assert!(next.drain_events().is_empty());
        next.shutdown();
    }

    #[test]
    fn comments_authenticate_validate_rate_limit_and_capture_generation() {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        let (url, token) = ready(&session);
        let mut ws = connect(&url);
        ws.send(tungstenite::Message::Text(
            serde_json::json!({"protocol":"qp.audience","version":"1","type":"hello","token":token}).to_string().into(),
        ))
        .unwrap();
        assert!(ws.read().unwrap().into_text().unwrap().contains("comments"));
        for n in 0..3 {
            ws.send(tungstenite::Message::Text(serde_json::json!({"protocol":"qp.audience","version":"1","type":"comment","request_id":n.to_string(),"body":"Hello\n世界", "handle":" Anonymous "}).to_string().into())).unwrap();
            let response = ws.read().unwrap().into_text().unwrap();
            assert!(response.contains(if n < 2 { "accepted" } else { "busy" }));
        }
        assert_eq!(session.advance_comment_generation(), 1);
        let events = session.drain_events();
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.display_generation == 0));
        assert_eq!(
            events[0].event,
            crate::audience_events::AudienceEvent::Comment {
                body: "Hello 世界".into(),
                handle: " Anonymous ".into()
            }
        );
        // Reactions remain available when the independent comment budget is empty.
        ws.send(tungstenite::Message::Text(
            r#"{"protocol":"qp.audience","version":"1","type":"reaction","request_id":"r","kind":"heart"}"#.into(),
        ))
        .unwrap();
        assert!(ws.read().unwrap().into_text().unwrap().contains("accepted"));
        ws.send(tungstenite::Message::Text(
            r#"{"protocol":"qp.audience","version":"1","type":"comment","request_id":"bad","body":"  ","handle":""}"#.into(),
        ))
        .unwrap();
        assert!(matches!(ws.read().unwrap(), tungstenite::Message::Close(_)));
        session.shutdown();
    }
    #[test]
    fn unrecognized_application_message_closes_without_delivery() {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        let (url, token) = ready(&session);
        let mut ws = connect(&url);
        ws.send(tungstenite::Message::Text(
            serde_json::json!({"protocol":"qp.audience","version":"1","type":"hello","token":token}).to_string().into(),
        ))
        .unwrap();
        ws.read().unwrap();
        ws.send(tungstenite::Message::Text(
            r#"{"protocol":"qp.audience","version":"1","type":"reaction","request_id":"1","kind":"unknown"}"#.into(),
        ))
        .unwrap();
        assert!(matches!(ws.read().unwrap(), tungstenite::Message::Close(_)));
        assert!(session.drain_events().is_empty());
        session.shutdown();
    }

    #[test]
    fn many_authenticated_clients_deliver_bounded_events_and_shutdown_cleanly() {
        exercise_many_clients(1, Duration::ZERO);
    }

    #[test]
    #[ignore = "explicit local sustained-load validation; takes about 30 seconds"]
    fn sustained_local_load_remains_bounded_and_stops_cleanly() {
        exercise_many_clients(30, Duration::from_secs(1));
    }

    fn exercise_many_clients(rounds: usize, interval: Duration) {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        let (url, token) = ready(&session);
        let mut clients = Vec::new();
        // Pace joins to respect the anti-churn handshake budget, not an audience cap.
        for batch in 0..3 {
            if batch > 0 {
                thread::sleep(Duration::from_millis(1100));
            }
            for _ in 0..16 {
                let mut ws = connect(&url);
                ws.send(tungstenite::Message::Text(
                    serde_json::json!({"protocol":"qp.audience","version":"1","type":"hello","token":token}).to_string().into(),
                ))
                .unwrap();
                let welcome: serde_json::Value =
                    serde_json::from_str(&ws.read().unwrap().into_text().unwrap()).unwrap();
                assert_eq!(welcome["type"], "welcome");
                clients.push(ws);
            }
        }
        assert_eq!(session.snapshot().connections, 48);
        let started = Instant::now();
        let mut totals = [0usize; 2];
        let mut max_round = Duration::ZERO;
        for round in 0..rounds {
            let round_started = Instant::now();
            let workers: Vec<_> = clients
                .into_iter()
                .enumerate()
                .map(|(index, mut ws)| {
                    thread::spawn(move || {
                        let mut counts = [0usize; 2];
                        for request in 0..6 {
                            let id = format!("{round}-{index}-{request}");
                            ws.send(tungstenite::Message::Text(
                                serde_json::json!({"protocol":"qp.audience","version":"1","type":"reaction","request_id":id,"kind":"heart"})
                                    .to_string().into(),
                            )).unwrap();
                            let reply: serde_json::Value = loop {
                                match ws.read().unwrap() {
                                    tungstenite::Message::Text(text) => break serde_json::from_str(&text).unwrap(),
                                    tungstenite::Message::Ping(_) => ws.flush().unwrap(),
                                    tungstenite::Message::Pong(_) => (),
                                    message => panic!("unexpected load-test response: {message:?}"),
                                }
                            };
                            assert_eq!(reply["request_id"], id);
                            let outcome = match reply["status"].as_str() {
                                Some("accepted") => 0,
                                Some("busy") => 1,
                                status => panic!("unexpected admission status: {status:?}"),
                            };
                            counts[outcome] += 1;
                        }
                        (ws, counts)
                    })
                })
                .collect();
            clients = workers
                .into_iter()
                .map(|w| {
                    let (ws, counts) = w.join().unwrap();
                    for (total, count) in totals.iter_mut().zip(counts) {
                        *total += count;
                    }
                    ws
                })
                .collect();
            let events = session.drain_events();
            assert!(!events.is_empty());
            assert!(events.len() <= 16);
            assert!(events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence));
            assert_eq!(session.snapshot().connections, 48);
            max_round = max_round.max(round_started.elapsed());
            if round + 1 < rounds {
                thread::sleep(interval.saturating_sub(round_started.elapsed()));
            }
        }
        assert_eq!(totals.iter().sum::<usize>(), 48 * 6 * rounds);
        println!("Local load: clients=48 rounds={rounds} requests={} accepted={} busy={} elapsed_ms={} max_round_ms={}",
            48 * 6 * rounds, totals[0], totals[1], started.elapsed().as_millis(), max_round.as_millis());
        let authority = url
            .strip_prefix("http://")
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        session.shutdown();
        assert_eq!(session.snapshot().connections, 0);
        assert_eq!(session.snapshot().status, SessionStatus::Stopped);
        assert!(session.drain_events().is_empty());
        for ws in &mut clients {
            assert!(ws.read().is_err());
        }
        let _rebound = std::net::TcpListener::bind(authority).unwrap();
    }

    #[test]
    fn shutdown_closes_clients_waiting_for_authentication() {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        let (url, _) = ready(&session);
        let mut clients: Vec<_> = (0..8).map(|_| connect(&url)).collect();
        assert_eq!(session.snapshot().connections, 0);
        session.shutdown();
        assert!(session.is_finished());
        for ws in &mut clients {
            assert!(ws.read().is_err());
        }
    }

    #[test]
    fn handshake_budget_bounds_churn_and_recovers() {
        let now = Instant::now();
        let mut budget = HandshakeBudget::new(now);
        for _ in 0..20 {
            assert!(budget.allow(now));
        }
        assert!(!budget.allow(now));
        assert!(budget.allow(now + Duration::from_secs(1)));
    }
    #[test]
    fn authentication_requires_version_and_secret() {
        assert!(authenticated(
            r#"{"protocol":"qp.audience","version":"1","type":"hello","token":"secret"}"#,
            "secret"
        ));
        for text in [
            r#"{"protocol":"qp.audience","version":"2","type":"hello","token":"secret"}"#,
            r#"{"protocol":"qp.audience","version":"1-draft.1","type":"hello","token":"secret"}"#,
            r#"{"protocol":"qp.audience","version":"1","type":"hello","token":"wrong"}"#,
            r#"{"protocol":"qp.audience","version":"1","type":"hello","token":"secret","extra":true}"#,
            "{}",
            "invalid",
        ] {
            assert!(!authenticated(text, "secret"));
        }
    }
    #[test]
    fn host_and_origin_must_match() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "127.0.0.1:8000".parse().unwrap());
        assert!(valid_headers(&headers, "127.0.0.1:8000", false));
        assert!(!valid_headers(&headers, "127.0.0.1:8000", true));
        headers.insert(header::ORIGIN, "http://evil.example".parse().unwrap());
        assert!(!valid_headers(&headers, "127.0.0.1:8000", true));
        headers.insert(header::ORIGIN, "http://127.0.0.1:8000".parse().unwrap());
        assert!(valid_headers(&headers, "127.0.0.1:8000", true));
        assert!(!valid_headers(&headers, "127.0.0.1:9000", true));
    }
    #[test]
    fn credentials_have_independent_random_secrets() {
        let a = credentials().unwrap();
        let b = credentials().unwrap();
        assert_eq!(a.0.len(), 8);
        assert_eq!(a.1.len(), 43);
        assert_ne!(a.1, b.1);
    }
    #[test]
    fn stopping_during_startup_reaps_worker() {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        session.shutdown();
        assert_eq!(session.snapshot().status, SessionStatus::Stopped);
        assert_eq!(session.snapshot().connections, 0);
    }
    fn hello(token: &str) -> String {
        serde_json::json!({"protocol":PROTOCOL,"version":VERSION,"type":"hello","token":token})
            .to_string()
    }
    fn input(id: &str, body: &str) -> String {
        serde_json::json!({"protocol":PROTOCOL,"version":VERSION,"type":"comment","request_id":id,"body":body,"handle":"👩🏽‍💻"}).to_string()
    }
    fn authed(url: &str, token: &str) -> tungstenite::WebSocket<std::net::TcpStream> {
        let mut ws = connect(url);
        ws.send(tungstenite::Message::Text(hello(token).into()))
            .unwrap();
        let welcome: serde_json::Value =
            serde_json::from_str(&ws.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(welcome["protocol"], PROTOCOL);
        assert_eq!(welcome["version"], VERSION);
        assert_eq!(welcome["available"], true);
        assert!(welcome["connection_id"].as_str().unwrap().len() <= 64);
        assert_eq!(
            welcome["limits"],
            serde_json::json!({"max_message_bytes":4096,"request_cache_ms":30000})
        );
        ws
    }
    fn result(ws: &mut tungstenite::WebSocket<std::net::TcpStream>) -> serde_json::Value {
        let value: serde_json::Value =
            serde_json::from_str(&ws.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(value["protocol"], PROTOCOL);
        assert_eq!(value["version"], VERSION);
        assert_eq!(value["type"], "result");
        value
    }
    #[test]
    fn repeated_request_returns_cached_result_without_duplicate_delivery_or_budget_cost() {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        let (url, token) = ready(&session);
        let mut ws = authed(&url, &token);
        let request = input("same", "Hello\n世界");
        for _ in 0..6 {
            ws.send(tungstenite::Message::Text(request.clone().into()))
                .unwrap();
            assert_eq!(result(&mut ws)["status"], "accepted");
        }
        assert_eq!(session.drain_events().len(), 1);
        ws.send(tungstenite::Message::Text(
            input("same", "different").into(),
        ))
        .unwrap();
        let tungstenite::Message::Close(Some(frame)) = ws.read().unwrap() else {
            panic!("expected protocol close");
        };
        assert_eq!(u16::from(frame.code), 1002);
        assert!(session.drain_events().is_empty());
        session.shutdown();
    }
    #[test]
    fn shared_emoji_comments_survive_actual_local_websockets_and_reload() {
        for body in [
            "😀 👍 ❤️ 👏 😂 ❓".to_owned(),
            "👨‍👩‍👧‍👦".into(),
            "🇯🇵 🇺🇸".into(),
            "👍🏽 👩🏽‍💻".into(),
            "e\u{301} 🏳️‍🌈".into(),
            "😀".repeat(200),
        ] {
            let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
            let (url, token) = ready(&session);
            let mut ws = authed(&url, &token);
            ws.send(tungstenite::Message::Text(input("1", &body).into()))
                .unwrap();
            assert_eq!(result(&mut ws)["status"], "accepted");
            let events = session.drain_events();
            assert_eq!(events.len(), 1);
            assert_eq!(
                events[0].event,
                crate::audience_events::AudienceEvent::Comment {
                    body: body.clone(),
                    handle: "👩🏽‍💻".into()
                }
            );
            drop(ws);
            wait_until(|| session.snapshot().connections == 0);
            let mut reloaded = authed(&url, &token);
            reloaded
                .send(tungstenite::Message::Text(input("1", &body).into()))
                .unwrap();
            assert_eq!(result(&mut reloaded)["status"], "accepted");
            let fresh = session.drain_events();
            assert_eq!(fresh.len(), 1);
            assert_ne!(events[0].participant_id, fresh[0].participant_id);
            session.shutdown();
        }
    }
    #[test]
    fn invalid_local_frames_close_with_protocol_error_without_delivery() {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        let (url, token) = ready(&session);
        let raw = input("bad", "text");
        for message in [
            tungstenite::Message::Text(
                raw.replace(
                    "\"request_id\":\"bad\"",
                    "\"request_id\":\"bad\",\"request_id\":\"bad\"",
                )
                .into(),
            ),
            tungstenite::Message::Text(
                raw.replace("\"version\":\"1\"", "\"version\":\"1-draft.1\"")
                    .into(),
            ),
            tungstenite::Message::Text(raw.replace("qp.audience", "qp.presenter").into()),
            tungstenite::Message::Text(
                raw.replace("\"body\":\"text\"", "\"body\":\"text\",\"v\":1")
                    .into(),
            ),
            tungstenite::Message::Text(
                raw.replace("\"body\":\"text\"", "\"body\":\"\\ud800\"")
                    .into(),
            ),
            tungstenite::Message::Text(input("bad", &"😀".repeat(201)).into()),
            tungstenite::Message::Text("{".into()),
            tungstenite::Message::Binary(vec![123, 125].into()),
        ] {
            let mut ws = authed(&url, &token);
            ws.send(message).unwrap();
            let tungstenite::Message::Close(Some(frame)) = ws.read().unwrap() else {
                panic!("expected protocol close");
            };
            assert_eq!(u16::from(frame.code), 1002);
        }
        assert!(session.drain_events().is_empty());
        session.shutdown();
    }
    #[test]
    fn cache_expiry_conflict_and_capacity_are_bounded() {
        let now = Instant::now();
        let value = serde_json::json!({"body":"test"});
        let mut cache = RequestCache::default();
        cache.insert("x".into(), value.clone(), "accepted", now);
        assert_eq!(
            cache.check("x", &value, now + REQUEST_CACHE - Duration::from_nanos(1)),
            Ok(Some("accepted"))
        );
        assert_eq!(
            cache.check("x", &serde_json::json!({"body":"other"}), now),
            Err(1002)
        );
        assert_eq!(cache.check("x", &value, now + REQUEST_CACHE), Ok(None));
        for n in 0..MAX_REQUESTS {
            cache.insert(n.to_string(), value.clone(), "busy", now);
        }
        assert_eq!(cache.check("overflow", &value, now), Err(1013));
        assert_eq!(cache.check("0", &value, now), Ok(Some("busy")));
        assert_eq!(
            cache.check("overflow", &value, now + REQUEST_CACHE),
            Ok(None)
        );
        assert_eq!(wire_status(Admission::RateLimited), "busy");
        assert_eq!(wire_status(Admission::Busy), "busy");
        assert_eq!(wire_status(Admission::Stopped), "stopped");
    }
    #[test]
    fn strict_local_authentication_and_frame_limits() {
        let valid = hello("A");
        assert!(authenticated(&valid, "A"));
        for invalid in [
            valid.replace("\"token\":\"A\"", "\"token\":\"A\",\"token\":\"A\""),
            valid.replace("\"type\":\"hello\"", "\"type\":\"hello\",\"extra\":true"),
            valid.replace("qp.audience", "qp.presenter"),
            valid.replace("\"version\":\"1\"", "\"version\":\"1-draft.1\""),
            "{\"v\":1,\"token\":\"A\"}".into(),
        ] {
            assert!(!authenticated(&invalid, "A"));
        }
        assert!(audience_input(&format!(
            "{}{}",
            input("x", "test"),
            " ".repeat(4096 - input("x", "test").len())
        ))
        .is_some());
        assert!(audience_input(&format!(
            "{}{}",
            input("x", "test"),
            " ".repeat(4097 - input("x", "test").len())
        ))
        .is_none());
    }
    #[test]
    fn embedded_module_and_notices_are_served_with_scoped_security_headers() {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        let (url, token) = ready(&session);
        let authority = url
            .strip_prefix("http://")
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        for (path, body) in [
            (
                "/assets/audience.js",
                include_str!("../assets/audience/audience.js"),
            ),
            (
                "/assets/LICENSE",
                include_str!("../assets/audience/LICENSE"),
            ),
            ("/assets/NOTICE", include_str!("../assets/audience/NOTICE")),
        ] {
            let mut http = std::net::TcpStream::connect(authority).unwrap();
            http.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            write!(
                http,
                "GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            let mut response = String::new();
            http.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1 200"));
            assert!(response.ends_with(body));
            assert!(response.contains("x-content-type-options: nosniff"));
            assert!(response.contains("cache-control: no-store"));
            assert!(!response.contains(&token));
        }
        for (path, host, status) in [
            ("/assets/audience.js", "other.invalid", 404),
            ("/ws", "", 404),
            ("/ws/audience/other", "", 404),
        ] {
            let mut http = std::net::TcpStream::connect(authority).unwrap();
            http.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let host = if host.is_empty() { authority } else { host };
            write!(http,"GET {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: http://{authority}\r\nUpgrade: websocket\r\nConnection: Upgrade, close\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").unwrap();
            let mut response = String::new();
            http.read_to_string(&mut response).unwrap();
            assert!(
                response.starts_with(&format!("HTTP/1.1 {status}")),
                "{response}"
            );
        }
        session.shutdown();
    }
}

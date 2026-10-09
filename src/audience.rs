//! Optional local audience session. Network tasks never access Slint or PDF state.
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use tokio::sync::{watch, Semaphore};

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
}

impl LocalAudienceSession {
    pub fn start(address: Ipv4Addr) -> std::io::Result<Self> {
        let status = Arc::new(Mutex::new(SessionStatus::Starting));
        let connections = Arc::new(AtomicUsize::new(0));
        let (stop, receiver) = watch::channel(false);
        let worker_status = status.clone();
        let worker_connections = connections.clone();
        let worker = thread::Builder::new()
            .name("audience-local".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?;
                    runtime.block_on(run(
                        address,
                        worker_status.clone(),
                        worker_connections,
                        receiver,
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

    pub fn request_stop(&self) {
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
}

fn credentials() -> std::io::Result<(String, String)> {
    let mut bytes = [0u8; 36];
    getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
    let encode = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok((encode(&bytes[..4]).to_uppercase(), encode(&bytes[4..])))
}

async fn run(
    address: Ipv4Addr,
    status: Arc<Mutex<SessionStatus>>,
    connections: Arc<AtomicUsize>,
    mut stop: watch::Receiver<bool>,
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
    };
    let app = Router::new()
        .route("/join/{code}", get(join_page))
        .route("/ws", get(upgrade))
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
        (header::CONTENT_SECURITY_POLICY, "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'")],
        Html(include_str!("../assets/audience/index.html"))).into_response()
}

async fn upgrade(
    State(state): State<ServerState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
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
    v: u32,
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
    serde_json::from_str::<Hello>(text).is_ok_and(|hello| hello.v == 1 && hello.token == secret)
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
        return;
    };
    if !authenticated(&text, &state.secret) || *stop.borrow() {
        return;
    }
    drop(permit);
    state.connections.fetch_add(1, Ordering::Relaxed);
    let _count = CountGuard(state.connections.clone());
    if !matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            socket.send(Message::Text(
                r#"{"v":1,"type":"welcome","capabilities":[]}"#.into(),
            )),
        )
        .await,
        Ok(Ok(()))
    ) {
        return;
    }
    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    heartbeat.tick().await;
    let mut waiting_for_pong = false;
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            _ = heartbeat.tick() => {
                if waiting_for_pong { break; }
                waiting_for_pong = true;
                if !matches!(tokio::time::timeout(Duration::from_secs(2), socket.send(Message::Ping(Vec::new().into()))).await, Ok(Ok(()))) { break; }
            }
            message = socket.recv() => match message {
                Some(Ok(Message::Pong(_))) => waiting_for_pong = false,
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                // No event submission is supported in the session-only phase.
                _ => break,
            }
        }
    }
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
        let mut request = format!("ws://{authority}/ws")
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
                r#"{"v":1,"token":"wrong"}"#.into(),
            ))
            .unwrap();
        assert!(rejected.read().is_err());
        assert_eq!(session.snapshot().connections, 0);
        let mut ws = connect(&url);
        ws.send(tungstenite::Message::Text(
            serde_json::json!({"v":1,"token":token}).to_string().into(),
        ))
        .unwrap();
        let welcome = ws.read().unwrap().into_text().unwrap();
        assert!(welcome.contains("welcome"));
        assert_eq!(session.snapshot().connections, 1);
        ws.close(None).unwrap();
        wait_until(|| session.snapshot().connections == 0);
        let mut ws = connect(&url);
        ws.send(tungstenite::Message::Text(
            serde_json::json!({"v":1,"token":token}).to_string().into(),
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
        assert!(authenticated(r#"{"v":1,"token":"secret"}"#, "secret"));
        for text in [
            r#"{"v":2,"token":"secret"}"#,
            r#"{"v":1,"token":"wrong"}"#,
            r#"{"v":1,"token":"secret","extra":true}"#,
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
        assert_eq!(a.1.len(), 64);
        assert_ne!(a.1, b.1);
    }
    #[test]
    fn stopping_during_startup_reaps_worker() {
        let mut session = LocalAudienceSession::start(Ipv4Addr::LOCALHOST).unwrap();
        session.shutdown();
        assert_eq!(session.snapshot().status, SessionStatus::Stopped);
        assert_eq!(session.snapshot().connections, 0);
    }
}

//! Bounded room management on the remote session's existing network runtime.
use crate::{
    app_config::RelayEndpoint,
    audience_relay::{strict_json, RelayConfig},
};
use futures_util::StreamExt;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

pub(crate) const ROOM_EXPIRED: &str = "Relay room expired. Start again to create a new room.";

#[derive(Clone)]
pub struct ManagedRelay {
    pub endpoint: RelayEndpoint,
    pub attempt: Arc<Mutex<Option<Attempt>>>,
    pub end_requested: Arc<AtomicBool>,
    journal: Option<Arc<crate::audience_recovery::Journal>>,
}
#[derive(Clone)]
pub struct Attempt {
    pub key: String,
    pub room: Option<RelayConfig>,
    pub may_exist: bool,
}
impl ManagedRelay {
    pub fn new(endpoint: RelayEndpoint) -> Self {
        Self {
            endpoint,
            attempt: Arc::new(Mutex::new(None)),
            end_requested: Arc::new(AtomicBool::new(false)),
            journal: None,
        }
    }
    pub fn for_configuration(
        endpoint: RelayEndpoint,
        config: std::path::PathBuf,
    ) -> Result<Self, &'static str> {
        let (journal, record) = crate::audience_recovery::Journal::open(&config, &endpoint)?;
        let mut settings = Self::new(endpoint);
        if let Some(record) = record {
            *settings.attempt.lock().unwrap_or_else(|e| e.into_inner()) =
                record.key.map(|key| Attempt {
                    key,
                    room: None,
                    may_exist: record.may_exist,
                });
            settings
                .end_requested
                .store(record.end_requested, Ordering::Release);
        }
        settings.journal = Some(Arc::new(journal));
        Ok(settings)
    }
    fn save_locked(
        &self,
        attempt: &Option<Attempt>,
        end_requested: bool,
    ) -> Result<(), &'static str> {
        if let Some(journal) = &self.journal {
            journal.save(&crate::audience_recovery::Record {
                version: 1,
                binding: crate::audience_recovery::binding(&self.endpoint),
                key: attempt.as_ref().map(|a| a.key.clone()),
                may_exist: attempt.as_ref().is_some_and(|a| a.may_exist),
                end_requested,
            })?;
        }
        Ok(())
    }
    fn persist_intent(&self) -> Result<(), &'static str> {
        let slot = self.attempt.lock().unwrap_or_else(|e| e.into_inner());
        self.save_locked(&slot, self.end_requested.load(Ordering::Acquire))
    }
    fn finish_cleanup(&self) -> Result<(), &'static str> {
        let mut slot = self.attempt.lock().unwrap_or_else(|e| e.into_inner());
        // Commit the complete end as one record; partial HTTP success remains recoverable.
        self.save_locked(&None, false)?;
        *slot = None;
        self.end_requested.store(false, Ordering::Release);
        Ok(())
    }
    pub fn attempt(&self) -> Result<Attempt, &'static str> {
        let mut slot = self.attempt.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes).map_err(|_| "Cannot generate relay request identity.")?;
            let key = bytes.iter().map(|b| format!("{b:02x}")).collect();
            *slot = Some(Attempt {
                key,
                room: None,
                may_exist: false,
            });
        }
        self.save_locked(&slot, self.end_requested.load(Ordering::Acquire))?;
        Ok(slot.as_ref().unwrap().clone())
    }
    pub fn has_attempt(&self) -> bool {
        self.attempt
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }
    pub fn requires_recovery(&self) -> bool {
        self.end_requested.load(Ordering::Acquire)
            || self
                .attempt
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .is_some_and(|a| a.may_exist || a.room.is_some())
    }
    pub fn with_endpoint(&self, endpoint: RelayEndpoint) -> Self {
        Self {
            endpoint,
            attempt: self.attempt.clone(),
            end_requested: self.end_requested.clone(),
            journal: self.journal.clone(),
        }
    }
    pub fn mark_request_started(&self) -> Result<bool, &'static str> {
        let mut slot = self.attempt.lock().unwrap_or_else(|e| e.into_inner());
        let attempt = slot
            .as_mut()
            .expect("creation identity allocated before sending");
        let ambiguous = attempt.may_exist;
        attempt.may_exist = true;
        self.save_locked(&slot, self.end_requested.load(Ordering::Acquire))?;
        Ok(ambiguous)
    }
    pub fn clear(&self) -> Result<(), &'static str> {
        let mut slot = self.attempt.lock().unwrap_or_else(|e| e.into_inner());
        self.save_locked(&None, self.end_requested.load(Ordering::Acquire))?;
        *slot = None;
        Ok(())
    }
}

pub fn client() -> Result<reqwest::Client, &'static str> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| "Cannot initialize relay HTTP client.")
}

pub async fn create(
    client: &reqwest::Client,
    settings: &ManagedRelay,
) -> Result<RelayConfig, &'static str> {
    let attempt = settings.attempt()?;
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        for index in 0..3 {
            if index > 0 {
                tokio::time::sleep(Duration::from_millis(200 * index)).await;
            }
            let previously_ambiguous = settings.mark_request_started()?;
            let response = client
                .post(settings.endpoint.origin.join("api/relay/v1/rooms").unwrap())
                .bearer_auth(&settings.endpoint.create_token)
                .json(&serde_json::json!({"idempotency_key": attempt.key}))
                .send()
                .await;
            let response = match response {
                Ok(r) => r,
                Err(_) => continue,
            };
            match response.status().as_u16() {
                200 | 201 => {}
                status @ (401 | 403) => {
                    if !previously_ambiguous {
                        if let Some(attempt) = settings
                            .attempt
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .as_mut()
                        {
                            attempt.may_exist = false;
                        }
                    }
                    settings.persist_intent()?;
                    return Err(if status == 401 {
                        "Relay creation credential was rejected."
                    } else {
                        "Relay denied room creation."
                    });
                }
                410 => {
                    settings.clear()?;
                    return Err(ROOM_EXPIRED);
                }
                503 => continue,
                _ => return Err("Relay returned an unsupported creation response."),
            }
            let mut body = Vec::new();
            let mut chunks = response.bytes_stream();
            let mut lost = false;
            while let Some(chunk) = chunks.next().await {
                match chunk {
                    Ok(chunk) if body.len() + chunk.len() <= 4096 => body.extend_from_slice(&chunk),
                    Ok(_) => return Err("Relay creation response is too large."),
                    Err(_) => {
                        lost = true;
                        break;
                    }
                }
            }
            if lost {
                continue;
            }
            let room: RelayConfig = serde_json::from_value(strict_json(&body)?)
                .map_err(|_| "Invalid relay room response.")?;
            room.validate()?;
            let join =
                url::Url::parse(&room.join_url).map_err(|_| "Invalid relay room response.")?;
            if join.origin() != settings.endpoint.origin.origin() {
                return Err("Relay returned a different origin.");
            }
            if let Some(slot) = settings
                .attempt
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
            {
                slot.room = Some(room.clone());
            }
            return Ok(room);
        }
        Err("Relay room creation is unavailable. Retry to recover the same room.")
    })
    .await;
    result.unwrap_or(Err(
        "Relay room creation timed out. Retry to recover the same room.",
    ))
}

pub async fn end(
    client: &reqwest::Client,
    endpoint: &RelayEndpoint,
    room: &RelayConfig,
) -> Result<(), &'static str> {
    let response = client
        .post(
            endpoint
                .origin
                .join(&format!("api/relay/v1/rooms/{}/end", room.room_id))
                .unwrap(),
        )
        .bearer_auth(&room.presenter_token)
        .send()
        .await
        .map_err(|_| "Cannot confirm relay room termination.")?;
    match response.status().as_u16() {
        200 | 404 | 410 => Ok(()),
        _ => Err("Cannot confirm relay room termination."),
    }
}

pub async fn cleanup(
    client: &reqwest::Client,
    settings: &ManagedRelay,
) -> Result<(), &'static str> {
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        // Persist explicit end intent before either HTTP endpoint can commit.
        settings.persist_intent()?;
        let session_end = async {
            // Explicit OFF is independent of best-effort room transport cleanup.
            if settings.end_requested.load(Ordering::Acquire) {
                let response = client
                    .post(
                        settings
                            .endpoint
                            .origin
                            .join("api/relay/v1/session/end")
                            .unwrap(),
                    )
                    .bearer_auth(&settings.endpoint.create_token)
                    .send()
                    .await
                    .map_err(|_| {
                        "Cannot confirm session termination. Retry to check the same session."
                    })?;
                if !matches!(response.status().as_u16(), 200 | 404 | 410) {
                    return Err(
                        "Cannot confirm session termination. Retry to check the same session.",
                    );
                }
            }
            Ok(())
        };
        let room_end = async {
            // Recover only an existing identity; never allocate a fresh one here.
            if !settings.has_attempt() {
                return Ok(());
            }
            let cached = settings
                .attempt
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .and_then(|a| a.room.clone());
            let room = match cached {
                Some(room) => room,
                None => match create(client, settings).await {
                    Ok(room) => room,
                    Err(ROOM_EXPIRED) => return Ok(()),
                    Err(error) => return Err(error),
                },
            };
            end(client, &settings.endpoint, &room).await
        };
        // Neither endpoint can prevent the other from being attempted. Both
        // share the same deadline, including a stalled session-end response.
        let (session_result, room_result) = tokio::join!(session_end, room_end);
        session_result.and(room_result)
    })
    .await
    .unwrap_or(Err("Cannot confirm relay room termination."));
    if result.is_ok() {
        settings.finish_cleanup()?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Bytes,
        extract::{
            ws::{Message as WsMessage, WebSocketUpgrade},
            State,
        },
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::{get, post},
        Router,
    };
    use serde_json::{json, Value};
    use std::{
        collections::VecDeque,
        sync::atomic::{AtomicUsize, Ordering},
    };

    #[derive(Clone)]
    struct Peer {
        responses: Arc<Mutex<VecDeque<(u16, String)>>>,
        requests: Arc<Mutex<Vec<(String, String)>>>,
        ends: Arc<AtomicUsize>,
        session_ends: Arc<AtomicUsize>,
        session_results: Arc<Mutex<VecDeque<u16>>>,
        end_results: Arc<Mutex<VecDeque<u16>>>,
        session_gate: Arc<Mutex<Option<Arc<tokio::sync::Notify>>>>,
        handshakes: Arc<Mutex<VecDeque<u16>>>,
        creation_gate: Arc<Mutex<Option<Arc<tokio::sync::Notify>>>>,
        display_gate: Arc<Mutex<Option<Arc<tokio::sync::Notify>>>>,
        display_seen: Arc<AtomicBool>,
    }
    struct Server {
        settings: ManagedRelay,
        peer: Peer,
        task: tokio::task::JoinHandle<()>,
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    async fn server() -> Server {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let settings = ManagedRelay::new(RelayEndpoint {
            origin: url::Url::parse(&origin).unwrap(),
            create_token: "A".repeat(43),
        });
        let peer = Peer {
            responses: Arc::new(Mutex::new(VecDeque::new())),
            requests: Arc::new(Mutex::new(Vec::new())),
            ends: Arc::new(AtomicUsize::new(0)),
            session_ends: Arc::new(AtomicUsize::new(0)),
            session_results: Arc::new(Mutex::new(VecDeque::new())),
            end_results: Arc::new(Mutex::new(VecDeque::new())),
            session_gate: Arc::new(Mutex::new(None)),
            handshakes: Arc::new(Mutex::new(VecDeque::new())),
            creation_gate: Arc::new(Mutex::new(None)),
            display_gate: Arc::new(Mutex::new(None)),
            display_seen: Arc::new(AtomicBool::new(false)),
        };
        let app = Router::new()
            .route("/api/relay/v1/session/end", post(|State(peer): State<Peer>, headers: HeaderMap| async move {
                assert_eq!(headers["authorization"], format!("Bearer {}", "A".repeat(43)));
                peer.session_ends.fetch_add(1, Ordering::SeqCst);
                let gate = peer.session_gate.lock().unwrap().take();
                if let Some(gate) = gate { gate.notified().await; }
                StatusCode::from_u16(peer.session_results.lock().unwrap().pop_front().unwrap_or(200)).unwrap()
            }))
            .route(
                "/api/relay/v1/rooms",
                post(
                    |State(peer): State<Peer>, headers: HeaderMap, body: Bytes| async move {
                        let body: Value = serde_json::from_slice(&body).unwrap();
                        peer.requests.lock().unwrap().push((
                            headers["authorization"].to_str().unwrap().into(),
                            body["idempotency_key"].as_str().unwrap().into(),
                        ));
                        let gate = peer.creation_gate.lock().unwrap().take();
                        if let Some(gate) = gate {
                            gate.notified().await;
                        }
                        let (status, text) = peer
                            .responses
                            .lock()
                            .unwrap()
                            .pop_front()
                            .unwrap_or((503, "{}".into()));
                        (StatusCode::from_u16(status).unwrap(), text)
                    },
                ),
            )
            .route(
                "/api/relay/v1/rooms/test/end",
                post(|State(peer): State<Peer>, headers: HeaderMap| async move {
                    if headers["authorization"] != format!("Bearer {}", "B".repeat(43)) {
                        return StatusCode::UNAUTHORIZED;
                    }
                    peer.ends.fetch_add(1, Ordering::SeqCst);
                    StatusCode::from_u16(peer.end_results.lock().unwrap().pop_front().unwrap_or(200)).unwrap()
                }),
            )
            .route("/api/relay/{version}/rooms/fresh/end", post(|State(peer): State<Peer>, headers: HeaderMap| async move {
                    if headers["authorization"] != format!("Bearer {}", "B".repeat(43)) {
                        return StatusCode::UNAUTHORIZED;
                    }
                    peer.ends.fetch_add(1, Ordering::SeqCst);
                    StatusCode::OK
                }))
            .route("/presenter/test", get(|State(peer): State<Peer>, ws: WebSocketUpgrade| async move {
                let status = peer.handshakes.lock().unwrap().pop_front().unwrap_or(101);
                if status != 101 {
                    return StatusCode::from_u16(status).unwrap().into_response();
                }
                ws.on_upgrade(move |mut socket| async move {
                    let _ = socket.recv().await;
                    let welcome = json!({"protocol":"qp.presenter","version":crate::audience_relay::VERSION,"type":"presenter_welcome","presenter_epoch":"1","display_epoch":"0","audience_connections":0});
                    if socket.send(WsMessage::Text(welcome.to_string().into())).await.is_ok() {
                        let mut revision = 0u64;
                        while let Some(Ok(message)) = socket.recv().await {
                            let WsMessage::Text(text) = message else { continue; };
                            let message: Value = serde_json::from_str(&text).unwrap();
                            if message["type"] == "display" {
                                peer.display_seen.store(true, Ordering::SeqCst);
                                let gate = peer.display_gate.lock().unwrap().take();
                                if let Some(gate) = gate { gate.notified().await; }
                                revision += 1;
                                let state = json!({"protocol":"qp.presenter","version":"1","type":"state","revision":revision.to_string(),"display_epoch":message["display_epoch"],"available":true,"audience_connections":0});
                                if socket.send(WsMessage::Text(state.to_string().into())).await.is_err() { break; }
                            }
                        }
                    }
                }).into_response()
            }))
            .with_state(peer.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Server {
            settings,
            peer,
            task,
        }
    }
    fn room(settings: &ManagedRelay) -> Value {
        let origin = settings.endpoint.origin.as_str().trim_end_matches('/');
        json!({"room_id":"test","code":"TEST","join_url":format!("{origin}/join/test#k={}", "C".repeat(43)),"presenter_url":format!("{}/presenter/test",origin.replacen("http:","ws:",1)),"presenter_token":"B".repeat(43)})
    }

    #[tokio::test]
    async fn cold_restart_replays_the_original_key_instead_of_competing_for_an_active_room() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = RelayEndpoint {
            origin: url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap()))
                .unwrap(),
            create_token: "A".repeat(43),
        };
        let room_body = room(&ManagedRelay::new(endpoint.clone())).to_string();
        let active_key = Arc::new(Mutex::new(None::<String>));
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let created = Arc::new(AtomicUsize::new(0));
        let app = Router::new().route(
            "/api/relay/v1/rooms",
            post({
                let active_key = active_key.clone();
                let requests = requests.clone();
                let created = created.clone();
                let config = config.clone();
                move |body: Bytes| {
                    let active_key = active_key.clone();
                    let requests = requests.clone();
                    let created = created.clone();
                    let config = config.clone();
                    let room_body = room_body.clone();
                    async move {
                        let body: Value = serde_json::from_slice(&body).unwrap();
                        assert_eq!(body.as_object().unwrap().len(), 1); // No service clock or PDF data.
                        let key = body["idempotency_key"].as_str().unwrap().to_owned();
                        requests.lock().unwrap().push(key.clone());
                        let mut active = active_key.lock().unwrap();
                        if active.as_ref().is_some_and(|old| old != &key) {
                            return (StatusCode::FORBIDDEN, "{}".to_owned());
                        }
                        // Recovery identity must be durable before the server can commit.
                        let path = config.with_file_name("config.toml.audience-session.json");
                        let journal: Value =
                            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
                        assert_eq!(journal["key"], key);
                        assert_eq!(journal["may_exist"], true);
                        let status = if active.is_none() {
                            *active = Some(key);
                            created.fetch_add(1, Ordering::SeqCst);
                            StatusCode::CREATED
                        } else {
                            StatusCode::OK
                        };
                        (status, room_body)
                    }
                }
            }),
        );
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let first = ManagedRelay::for_configuration(endpoint.clone(), config.clone()).unwrap();
        let original = create(&client().unwrap(), &first).await.unwrap();
        let key = first.attempt().unwrap().key;
        drop(first); // No cleanup: reconstruct the client after process loss.
        let lost_identity = ManagedRelay::new(endpoint.clone());
        assert_eq!(
            create(&client().unwrap(), &lost_identity).await.err(),
            Some("Relay denied room creation.")
        );
        let restored = ManagedRelay::for_configuration(endpoint, config).unwrap();
        assert!(restored.requires_recovery());
        let recovered = create(&client().unwrap(), &restored).await.unwrap();
        assert_eq!(recovered.room_id, original.room_id);
        assert_eq!(recovered.presenter_token, original.presenter_token);
        assert_eq!(restored.attempt().unwrap().key, key);
        assert_eq!(created.load(Ordering::SeqCst), 1);
        let requests = requests.lock().unwrap();
        assert_eq!(requests[0], requests[2]);
        assert_ne!(requests[0], requests[1]);
        task.abort();
    }

    #[tokio::test]
    async fn journal_write_failure_prevents_any_room_creation_request() {
        let s = server().await;
        let dir = tempfile::tempdir().unwrap();
        let settings = ManagedRelay::for_configuration(
            s.settings.endpoint.clone(),
            dir.path().join("config.toml"),
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("config.toml.audience-session.json")).unwrap();
        assert!(create(&client().unwrap(), &settings).await.is_err());
        assert!(s.peer.requests.lock().unwrap().is_empty());
        assert!(settings.has_attempt());
    }

    #[tokio::test]
    async fn interrupted_explicit_end_is_restored_and_never_followed_by_a_fresh_creation() {
        for room_end_fails in [false, true] {
            let s = server().await;
            let dir = tempfile::tempdir().unwrap();
            let config = dir.path().join("config.toml");
            let first =
                ManagedRelay::for_configuration(s.settings.endpoint.clone(), config.clone())
                    .unwrap();
            push(&s, 201, room(&s.settings).to_string());
            create(&client().unwrap(), &first).await.unwrap();
            first.end_requested.store(true, Ordering::Release);
            if room_end_fails {
                s.peer.end_results.lock().unwrap().push_back(503);
            } else {
                s.peer.session_results.lock().unwrap().push_back(503);
            }
            assert!(cleanup(&client().unwrap(), &first).await.is_err());
            let key = first.attempt().unwrap().key;
            drop(first);
            let restored =
                ManagedRelay::for_configuration(s.settings.endpoint.clone(), config.clone())
                    .unwrap();
            assert!(restored.end_requested.load(Ordering::Acquire));
            assert_eq!(restored.attempt().unwrap().key, key);
            // The previous room end committed, while utilization end did not.
            if room_end_fails {
                push(&s, 200, room(&s.settings).to_string());
            } else {
                push(&s, 410, "{}".into());
            }
            let mut worker =
                crate::audience_relay::RemoteAudienceSession::start_managed(restored.clone())
                    .unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                while !worker.is_finished() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            worker.shutdown();
            assert!(matches!(
                worker.snapshot().status,
                crate::audience::SessionStatus::Stopped
            ));
            assert_eq!(s.peer.requests.lock().unwrap().len(), 2);
            assert_eq!(s.peer.session_ends.load(Ordering::SeqCst), 2);
            assert!(!restored.requires_recovery());
            drop(restored);
            drop(worker);
            let stopped =
                ManagedRelay::for_configuration(s.settings.endpoint.clone(), config).unwrap();
            assert!(!stopped.has_attempt());
            assert!(!stopped.end_requested.load(Ordering::Acquire));
        }
    }
    fn push(server: &Server, status: u16, body: String) {
        server
            .peer
            .responses
            .lock()
            .unwrap()
            .push_back((status, body));
    }

    #[test]
    fn cloned_settings_share_attempt_and_clear_cached_room_together() {
        let settings = ManagedRelay::new(RelayEndpoint {
            origin: url::Url::parse("http://127.0.0.1:1234").unwrap(),
            create_token: "A".repeat(43),
        });
        let clone = settings.clone();
        assert!(!settings.has_attempt());
        let attempt = settings.attempt().unwrap();
        assert_eq!(attempt.key.len(), 64);
        assert!(attempt.key.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(clone.attempt().unwrap().key, attempt.key);
        let room: RelayConfig = serde_json::from_value(room(&settings)).unwrap();
        room.validate().unwrap();
        settings.attempt.lock().unwrap().as_mut().unwrap().room = Some(room);
        assert_eq!(clone.attempt().unwrap().room.unwrap().room_id, "test");
        clone.clear().unwrap();
        assert!(!settings.has_attempt());
        assert_ne!(settings.attempt().unwrap().key, attempt.key);
        assert!(clone.attempt().unwrap().room.is_none());
    }

    #[tokio::test]
    async fn explicit_end_works_without_a_created_room_but_transport_cleanup_never_ends_session() {
        let s = server().await;
        let http = client().unwrap();
        cleanup(&http, &s.settings).await.unwrap();
        assert_eq!(s.peer.session_ends.load(Ordering::SeqCst), 0);
        s.settings.end_requested.store(true, Ordering::Release);
        cleanup(&http, &s.settings).await.unwrap();
        assert_eq!(s.peer.session_ends.load(Ordering::SeqCst), 1);
        assert!(!s.settings.end_requested.load(Ordering::Acquire));
        assert!(s.peer.requests.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn failed_explicit_end_preserves_intent_and_room_until_confirmed_retry() {
        let s = server().await;
        let http = client().unwrap();
        push(&s, 201, room(&s.settings).to_string());
        create(&http, &s.settings).await.unwrap();
        s.settings.end_requested.store(true, Ordering::Release);
        s.peer.session_results.lock().unwrap().extend([503, 200]);
        assert!(cleanup(&http, &s.settings).await.is_err());
        assert!(s.settings.end_requested.load(Ordering::Acquire));
        assert!(s.settings.has_attempt());
        assert_eq!(s.peer.ends.load(Ordering::SeqCst), 1);
        cleanup(&http, &s.settings).await.unwrap();
        assert!(!s.settings.end_requested.load(Ordering::Acquire));
        assert!(!s.settings.has_attempt());
        assert_eq!(s.peer.session_ends.load(Ordering::SeqCst), 2);
        assert_eq!(s.peer.ends.load(Ordering::SeqCst), 2);
        assert_eq!(s.peer.requests.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn explicit_end_is_backward_compatible_with_room_only_servers() {
        for status in [404, 410] {
            let s = server().await;
            s.peer.session_results.lock().unwrap().push_back(status);
            s.settings.end_requested.store(true, Ordering::Release);
            cleanup(&client().unwrap(), &s.settings).await.unwrap();
            assert!(!s.settings.end_requested.load(Ordering::Acquire));
        }
    }
    #[tokio::test]
    async fn exit_cleanup_does_not_allocate_a_fresh_key_after_a_terminal_room() {
        let s = server().await;
        let key = s.settings.attempt().unwrap().key;
        s.settings.mark_request_started().unwrap();
        push(&s, 410, "{}".into());
        push(&s, 201, room(&s.settings).to_string());
        let mut cleanup =
            crate::audience_relay::RemoteAudienceSession::start_cleanup(s.settings.clone())
                .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !cleanup.is_finished() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        cleanup.shutdown();
        assert_eq!(
            cleanup.snapshot().status,
            crate::audience::SessionStatus::Stopped
        );
        assert_eq!(
            s.peer.requests.lock().unwrap().as_slice(),
            &[(format!("Bearer {}", "A".repeat(43)), key)]
        );
        assert!(!s.settings.has_attempt());
    }

    #[tokio::test]
    async fn initial_rejection_allows_corrected_import_without_changing_identity() {
        for rejected in [401, 403] {
            let s = server().await;
            let http = client().unwrap();
            let key = s.settings.attempt().unwrap().key;
            push(&s, rejected, "{}".into());
            assert!(create(&http, &s.settings).await.is_err());
            assert!(!s.settings.requires_recovery());
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("download.toml");
            let destination = directory.path().join("config.toml");
            std::fs::write(
                &source,
                format!(
                    "[audience]\ntransport='remote'\nrelay='{}'\ncreate_token='{}'\n",
                    s.settings.endpoint.origin,
                    "D".repeat(43)
                ),
            )
            .unwrap();
            let endpoint = crate::app_config::import_file(&source, &destination).unwrap();
            let corrected = s.settings.with_endpoint(endpoint);
            push(&s, 201, room(&corrected).to_string());
            create(&http, &corrected).await.unwrap();
            let requests = s.peer.requests.lock().unwrap();
            assert_eq!(requests.len(), 2);
            assert!(requests.iter().all(|(_, request_key)| request_key == &key));
            assert_eq!(requests[1].0, format!("Bearer {}", "D".repeat(43)));
        }
    }

    #[tokio::test]
    async fn rejection_after_an_ambiguous_request_still_blocks_replacement() {
        let s = server().await;
        let http = client().unwrap();
        let key = s.settings.attempt().unwrap().key;
        assert!(create(&http, &s.settings).await.is_err());
        assert!(s.settings.requires_recovery());
        push(&s, 401, "{}".into());
        assert!(create(&http, &s.settings).await.is_err());
        assert!(s.settings.requires_recovery());
        assert_eq!(s.settings.attempt().unwrap().key, key);
    }

    #[tokio::test]
    async fn slow_session_end_cannot_skip_known_room_end_or_exceed_shared_deadline() {
        let s = server().await;
        let http = client().unwrap();
        push(&s, 201, room(&s.settings).to_string());
        create(&http, &s.settings).await.unwrap();
        s.settings.end_requested.store(true, Ordering::Release);
        *s.peer.session_gate.lock().unwrap() = Some(Arc::new(tokio::sync::Notify::new()));
        let settings = s.settings.clone();
        let task = tokio::spawn(async move { cleanup(&http, &settings).await });
        tokio::time::timeout(Duration::from_secs(1), async {
            while s.peer.ends.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("room end must not wait for session end");
        assert!(tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .is_err());
        assert!(s.settings.requires_recovery());
        assert!(s.settings.end_requested.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn join_metadata_waits_for_matching_display_acknowledgement() {
        for delayed in [false, true] {
            let s = server().await;
            let gate = Arc::new(tokio::sync::Notify::new());
            if delayed {
                *s.peer.display_gate.lock().unwrap() = Some(gate.clone());
            }
            push(&s, 201, room(&s.settings).to_string());
            let mut session =
                crate::audience_relay::RemoteAudienceSession::start_managed(s.settings.clone())
                    .unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                while !s.peer.display_seen.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            if delayed {
                assert_eq!(
                    session.snapshot().status,
                    crate::audience::SessionStatus::Starting
                );
                gate.notify_one();
            }
            tokio::time::timeout(Duration::from_secs(3), async {
                while !matches!(
                    session.snapshot().status,
                    crate::audience::SessionStatus::Running { .. }
                ) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            session.request_stop();
            tokio::time::timeout(Duration::from_secs(3), async {
                while !session.is_finished() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            session.shutdown();
        }
    }

    #[tokio::test]
    async fn denied_creation_keeps_the_attempt_for_a_later_explicit_retry() {
        let s = server().await;
        let http = client().unwrap();
        push(&s, 403, "untrusted server detail".into());
        assert_eq!(
            create(&http, &s.settings).await.err(),
            Some("Relay denied room creation.")
        );
        let attempt = s.settings.attempt().unwrap();
        assert!(attempt.room.is_none());
        push(&s, 201, room(&s.settings).to_string());
        assert_eq!(create(&http, &s.settings).await.unwrap().room_id, "test");
        let requests = s.peer.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|(_, key)| key == &attempt.key));
    }

    #[tokio::test]
    async fn stop_before_creation_is_polled_does_not_create_a_room_during_cleanup() {
        let s = server().await;
        let http = client().unwrap();
        push(&s, 201, room(&s.settings).to_string());
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        assert!(!*stopped.borrow());
        stop.send(true).unwrap();

        let created = tokio::select! {
            biased;
            _ = stopped.changed() => None,
            result = create(&http, &s.settings) => Some(result),
        };
        assert!(created.is_none());
        assert!(!s.settings.has_attempt());
        cleanup(&http, &s.settings).await.unwrap();
        assert!(!s.settings.has_attempt());
        assert!(s.peer.requests.lock().unwrap().is_empty());
        assert_eq!(s.peer.ends.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn one_on_replaces_confirmed_terminal_rooms_and_reaches_running() {
        use crate::{audience::SessionStatus, audience_relay::RemoteAudienceSession};
        for terminal_status in [0, 404, 410] {
            let s = server().await;
            let old_key = s.settings.attempt().unwrap().key;
            if terminal_status == 0 {
                push(&s, 410, "{}".into());
            } else {
                push(&s, 200, room(&s.settings).to_string());
                s.peer.handshakes.lock().unwrap().push_back(terminal_status);
            }
            let mut fresh = room(&s.settings);
            fresh["room_id"] = json!("fresh");
            fresh["code"] = json!("FRESH");
            fresh["join_url"] = json!(format!(
                "{}join/fresh#k={}",
                s.settings.endpoint.origin,
                "C".repeat(43)
            ));
            push(&s, 201, fresh.to_string());
            let mut session = RemoteAudienceSession::start_managed(s.settings.clone()).unwrap();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            loop {
                match session.snapshot().status {
                    SessionStatus::Running { code, url } => {
                        assert_eq!(code, "FRESH");
                        assert!(url.contains("/join/fresh#"));
                        break;
                    }
                    SessionStatus::Failed(error) => panic!("Recovery failed: {error}"),
                    _ => {}
                }
                assert!(tokio::time::Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let requests = s.peer.requests.lock().unwrap().clone();
            assert_eq!(requests.len(), 2);
            assert_eq!(requests[0].1, old_key);
            assert_ne!(requests[0].1, requests[1].1);
            session.request_stop();
            while !session.is_finished() {
                assert!(tokio::time::Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            session.shutdown();
            assert_eq!(s.peer.ends.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn stop_during_terminal_response_never_creates_a_replacement() {
        use crate::{audience::SessionStatus, audience_relay::RemoteAudienceSession};
        let s = server().await;
        let key = s.settings.attempt().unwrap().key;
        let gate = Arc::new(tokio::sync::Notify::new());
        *s.peer.creation_gate.lock().unwrap() = Some(gate.clone());
        push(&s, 410, "{}".into());
        push(&s, 410, "{}".into());
        let mut session = RemoteAudienceSession::start_managed(s.settings.clone()).unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while s.peer.requests.lock().unwrap().is_empty() {
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        session.request_stop();
        gate.notify_one();
        while !session.is_finished() {
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        session.shutdown();
        assert!(matches!(session.snapshot().status, SessionStatus::Stopped));
        let requests = s.peer.requests.lock().unwrap();
        assert!(requests.len() <= 2);
        assert!(requests.iter().all(|request| request.1 == key));
        assert!(!s.settings.has_attempt());
    }

    #[tokio::test]
    async fn terminal_replacement_is_bounded_and_ambiguous_failures_keep_the_key() {
        use crate::{audience::SessionStatus, audience_relay::RemoteAudienceSession};
        for statuses in [
            &[410, 410][..],
            &[401][..],
            &[403][..],
            &[404][..],
            &[503, 503, 503][..],
        ] {
            let s = server().await;
            let old_key = s.settings.attempt().unwrap().key;
            for status in statuses {
                push(&s, *status, "{}".into());
            }
            let mut session = RemoteAudienceSession::start_managed(s.settings.clone()).unwrap();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            while !session.is_finished() {
                assert!(tokio::time::Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            assert!(matches!(
                session.snapshot().status,
                SessionStatus::Failed(_)
            ));
            let requests = s.peer.requests.lock().unwrap().clone();
            assert_eq!(requests.len(), statuses.len());
            if statuses[0] == 410 {
                assert_ne!(requests[0].1, requests[1].1);
                assert!(!s.settings.has_attempt());
            } else {
                assert!(requests.iter().all(|request| request.1 == old_key));
                assert_eq!(s.settings.attempt().unwrap().key, old_key);
            }
            session.shutdown();
        }
    }

    #[tokio::test]
    async fn exact_response_size_is_accepted_and_overflow_retains_retry_identity() {
        let s = server().await;
        let http = client().unwrap();
        let body = room(&s.settings).to_string();
        let exact = format!("{body}{}", " ".repeat(4096 - body.len()));
        push(&s, 201, format!("{exact} "));
        let key = s.settings.attempt().unwrap().key;
        assert_eq!(
            create(&http, &s.settings).await.err(),
            Some("Relay creation response is too large.")
        );
        assert_eq!(s.settings.attempt().unwrap().key, key);
        assert!(s.settings.attempt().unwrap().room.is_none());
        push(&s, 201, exact);
        assert_eq!(create(&http, &s.settings).await.unwrap().room_id, "test");
        assert_eq!(s.peer.requests.lock().unwrap().len(), 2);
        assert_eq!(s.settings.attempt().unwrap().key, key);
    }

    #[tokio::test]
    async fn cleanup_of_a_confirmed_terminal_creation_succeeds_without_replacement() {
        let s = server().await;
        let key = s.settings.attempt().unwrap().key;
        push(&s, 410, "{}".into());
        cleanup(&client().unwrap(), &s.settings).await.unwrap();
        assert!(!s.settings.has_attempt());
        let requests = s.peer.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].1, key);
        assert_eq!(s.peer.ends.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn failed_cleanup_preserves_cached_room_for_retry_without_recreating_it() {
        let s = server().await;
        let http = client().unwrap();
        push(&s, 201, room(&s.settings).to_string());
        create(&http, &s.settings).await.unwrap();
        let key = s.settings.attempt().unwrap().key;
        s.settings
            .attempt
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .room
            .as_mut()
            .unwrap()
            .presenter_token = "C".repeat(43);
        assert_eq!(
            cleanup(&http, &s.settings).await.err(),
            Some("Cannot confirm relay room termination.")
        );
        assert!(s.settings.has_attempt());
        assert_eq!(s.settings.attempt().unwrap().key, key);
        assert_eq!(s.peer.requests.lock().unwrap().len(), 1);
        assert_eq!(s.peer.ends.load(Ordering::SeqCst), 0);
        s.settings
            .attempt
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .room
            .as_mut()
            .unwrap()
            .presenter_token = "B".repeat(43);
        cleanup(&http, &s.settings).await.unwrap();
        assert!(!s.settings.has_attempt());
        assert_eq!(s.peer.requests.lock().unwrap().len(), 1);
        assert_eq!(s.peer.ends.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_share_identity_cache_credentials_and_end_with_presenter_token() {
        let s = server().await;
        let http = client().unwrap();
        push(&s, 503, "{}".into());
        push(&s, 201, room(&s.settings).to_string());
        let created = create(&http, &s.settings).await.unwrap();
        assert_eq!(created.room_id, "test");
        push(&s, 200, room(&s.settings).to_string());
        let cached = create(&http, &s.settings).await.unwrap();
        assert_eq!(cached.presenter_token, "B".repeat(43));
        let requests = s.peer.requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0], requests[1]);
        assert_eq!(requests[0], requests[2]);
        assert_eq!(requests[0].0, format!("Bearer {}", "A".repeat(43)));
        assert_eq!(requests[0].1.len(), 64);
        cleanup(&http, &s.settings).await.unwrap();
        assert_eq!(s.peer.ends.load(Ordering::SeqCst), 1);
        assert!(s.settings.attempt.lock().unwrap().is_none());
        push(&s, 200, room(&s.settings).to_string());
        create(&http, &s.settings).await.unwrap();
        assert_ne!(s.peer.requests.lock().unwrap()[3].1, requests[0].1);
    }
    #[tokio::test]
    async fn failures_are_bounded_redacted_and_do_not_reset_ambiguous_keys() {
        let s = server().await;
        let http = client().unwrap();
        let key = s.settings.attempt().unwrap().key;
        for status in [401, 403, 302, 400] {
            push(&s, status, "secret-details".into());
            assert!(!create(&http, &s.settings)
                .await
                .err()
                .unwrap()
                .contains("secret-details"));
            assert_eq!(s.settings.attempt().unwrap().key, key);
        }
        push(&s, 410, "{}".into());
        assert!(create(&http, &s.settings).await.is_err());
        assert_ne!(s.settings.attempt().unwrap().key, key);
        let key = s.settings.attempt().unwrap().key;
        assert!(create(&http, &s.settings).await.is_err());
        assert_eq!(s.settings.attempt().unwrap().key, key);
        assert!(cleanup(&http, &s.settings).await.is_err());
        assert_eq!(s.settings.attempt().unwrap().key, key);
    }
    #[tokio::test]
    async fn responses_cannot_redirect_presenter_credentials_or_bypass_validation() {
        let s = server().await;
        let http = client().unwrap();
        let mut other = room(&s.settings);
        other["join_url"] = json!(format!("http://127.0.0.1:1/join/test#k={}", "C".repeat(43)));
        other["presenter_url"] = json!("ws://127.0.0.1:1/presenter/test");
        let mut bad = room(&s.settings);
        bad["presenter_token"] = json!("secret");
        let named = room(&s.settings);
        let positional = json!([
            named["room_id"],
            named["code"],
            named["join_url"],
            named["presenter_url"],
            named["presenter_token"]
        ]);
        for body in [
            positional.to_string(),
            other.to_string(),
            bad.to_string(),
            "{}".into(),
            "{".into(),
            "{\"x\":1,\"x\":2}".into(),
            " ".repeat(4097),
        ] {
            push(&s, 201, body);
            assert!(create(&http, &s.settings).await.is_err());
            assert!(s.settings.attempt().unwrap().room.is_none());
        }
        let room: RelayConfig = serde_json::from_value(room(&s.settings)).unwrap();
        let mut unauthorized = room.clone();
        unauthorized.presenter_token = "C".repeat(43);
        assert!(end(&http, &s.settings.endpoint, &unauthorized)
            .await
            .is_err());
        let mut missing = room.clone();
        missing.room_id = "missing".into();
        assert!(end(&http, &s.settings.endpoint, &missing).await.is_ok());
        let mut unavailable = s.settings.endpoint.clone();
        unavailable.origin = url::Url::parse("http://127.0.0.1:1").unwrap();
        assert!(end(&http, &unavailable, &room).await.is_err());
        let unavailable = ManagedRelay::new(unavailable);
        assert!(create(&http, &unavailable).await.is_err());
    }
}

#[cfg(test)]
mod fault_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Instant,
    };

    #[test]
    fn committed_but_truncated_response_recovers_with_the_same_key() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let settings = ManagedRelay::new(RelayEndpoint {
            origin: url::Url::parse(&origin).unwrap(),
            create_token: "A".repeat(43),
        });
        let body = serde_json::json!({"room_id":"room","code":"ROOM","join_url":format!("{origin}/join/room#k={}","C".repeat(43)),"presenter_url":format!("{}/ws/presenter/room",origin.replacen("http:","ws:",1)),"presenter_token":"B".repeat(43)}).to_string();
        let peer = thread::spawn(move || {
            let mut keys = Vec::new();
            for index in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut input = Vec::new();
                let (header_end, length) = loop {
                    let mut buffer = [0u8; 1024];
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    input.extend_from_slice(&buffer[..n]);
                    if let Some(end) = input.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&input[..end]).to_lowercase();
                        let length = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .unwrap()
                            .parse::<usize>()
                            .unwrap();
                        break (end + 4, length);
                    }
                };
                while input.len() < header_end + length {
                    let mut buffer = [0u8; 1024];
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    input.extend_from_slice(&buffer[..n]);
                }
                let request: serde_json::Value =
                    serde_json::from_slice(&input[header_end..header_end + length]).unwrap();
                keys.push(request["idempotency_key"].as_str().unwrap().to_owned());
                let response = if index == 0 {
                    "HTTP/1.1 201 Created\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{"
                        .to_owned()
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                };
                stream.write_all(response.as_bytes()).unwrap();
            }
            assert_eq!(keys[0], keys[1]);
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime
            .block_on(create(&client().unwrap(), &settings))
            .unwrap();
        assert_eq!(result.room_id, "room");
        peer.join().unwrap();
    }

    #[test]
    fn stop_during_creation_is_bounded_and_retains_unconfirmed_retry_identity() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let settings = ManagedRelay::new(RelayEndpoint {
            origin: url::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap(),
            create_token: "A".repeat(43),
        });
        let mut session =
            crate::audience_relay::RemoteAudienceSession::start_managed(settings.clone()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let first = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(_) => {
                    assert!(Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(5));
                }
            }
        };
        assert!(settings.has_attempt());
        let start = Instant::now();
        session.request_stop();
        while !session.is_finished() && start.elapsed() < Duration::from_secs(3) {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(session.is_finished());
        session.shutdown();
        assert!(settings.has_attempt());
        assert!(session.drain_events().is_empty());
        assert_eq!(session.snapshot().connections, 0);
        assert!(
            matches!(session.snapshot().status, crate::audience::SessionStatus::Failed(ref message) if message == "Cannot confirm relay room termination.")
        );
        drop(first);
    }
}

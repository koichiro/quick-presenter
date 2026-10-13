//! Bounded room management on the remote session's existing network runtime.
use crate::{
    app_config::RelayEndpoint,
    audience_relay::{strict_json, RelayConfig},
};
use futures_util::StreamExt;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone)]
pub struct ManagedRelay {
    pub endpoint: RelayEndpoint,
    pub attempt: Arc<Mutex<Option<Attempt>>>,
}
#[derive(Clone)]
pub struct Attempt {
    pub key: String,
    pub room: Option<RelayConfig>,
}
impl ManagedRelay {
    pub fn new(endpoint: RelayEndpoint) -> Self {
        Self {
            endpoint,
            attempt: Arc::new(Mutex::new(None)),
        }
    }
    pub fn attempt(&self) -> Result<Attempt, &'static str> {
        let mut slot = self.attempt.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes).map_err(|_| "Cannot generate relay request identity.")?;
            let key = bytes.iter().map(|b| format!("{b:02x}")).collect();
            *slot = Some(Attempt { key, room: None });
        }
        Ok(slot.as_ref().unwrap().clone())
    }
    pub fn has_attempt(&self) -> bool {
        self.attempt
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }
    pub fn clear(&self) {
        *self.attempt.lock().unwrap_or_else(|e| e.into_inner()) = None;
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
            let response = client
                .post(
                    settings
                        .endpoint
                        .origin
                        .join("api/relay/draft-1/rooms")
                        .unwrap(),
                )
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
                401 => return Err("Relay creation credential was rejected."),
                403 => return Err("Relay denied room creation."),
                410 => {
                    settings.clear();
                    return Err("Relay room expired. Start again to create a new room.");
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
                .join(&format!("api/relay/draft-1/rooms/{}/end", room.room_id))
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
        let cached = settings
            .attempt
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|a| a.room.clone());
        let room = match cached {
            Some(room) => room,
            None => create(client, settings).await?,
        };
        end(client, &settings.endpoint, &room).await
    })
    .await
    .unwrap_or(Err("Cannot confirm relay room termination."));
    settings.clear();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Bytes,
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::post,
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
        };
        let app = Router::new()
            .route(
                "/api/relay/draft-1/rooms",
                post(
                    |State(peer): State<Peer>, headers: HeaderMap, body: Bytes| async move {
                        let body: Value = serde_json::from_slice(&body).unwrap();
                        peer.requests.lock().unwrap().push((
                            headers["authorization"].to_str().unwrap().into(),
                            body["idempotency_key"].as_str().unwrap().into(),
                        ));
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
                "/api/relay/draft-1/rooms/test/end",
                post(|State(peer): State<Peer>, headers: HeaderMap| async move {
                    if headers["authorization"] != format!("Bearer {}", "B".repeat(43)) {
                        return StatusCode::UNAUTHORIZED;
                    }
                    peer.ends.fetch_add(1, Ordering::SeqCst);
                    StatusCode::OK
                }),
            )
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
        clone.clear();
        assert!(!settings.has_attempt());
        assert_ne!(settings.attempt().unwrap().key, attempt.key);
        assert!(clone.attempt().unwrap().room.is_none());
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
    async fn failed_cleanup_clears_cached_room_without_recreating_it() {
        let s = server().await;
        let http = client().unwrap();
        push(&s, 201, room(&s.settings).to_string());
        create(&http, &s.settings).await.unwrap();
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
        assert!(!s.settings.has_attempt());
        assert_eq!(s.peer.requests.lock().unwrap().len(), 1);
        assert_eq!(s.peer.ends.load(Ordering::SeqCst), 0);
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
        assert!(s.settings.attempt.lock().unwrap().is_none());
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
        for body in [
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
    fn stop_during_creation_is_bounded_and_clears_retry_identity() {
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
        assert!(!settings.has_attempt());
        assert!(session.drain_events().is_empty());
        assert_eq!(session.snapshot().connections, 0);
        assert!(
            matches!(session.snapshot().status, crate::audience::SessionStatus::Failed(ref message) if message == "Cannot confirm relay room termination.")
        );
        drop(first);
    }
}

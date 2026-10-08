#![cfg(any(unix, windows))]
use quick_presenter::control::{
    client,
    events::{self, EventHub},
    protocol::*,
    server::ControlServer,
};
use std::{path::PathBuf, sync::mpsc, thread, time::Duration};
struct Endpoint {
    path: PathBuf,
}
impl Endpoint {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        #[cfg(unix)]
        let path = {
            use std::os::unix::fs::DirBuilderExt;
            let root = std::env::temp_dir().join(format!("qp-events-{}-{id}", std::process::id()));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&root)
                .unwrap();
            root.join("control.sock")
        };
        #[cfg(windows)]
        let path = PathBuf::from(format!(r"\\.\pipe\qp-events-{}-{id}", std::process::id()));
        Self { path }
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        #[cfg(unix)]
        let _ = std::fs::remove_dir_all(self.path.parent().unwrap());
    }
}
fn status() -> Status {
    serde_json::from_str(r#"{"session_id":"test","document_revision":1,"document":"/slides.pdf","page":1,"pages":3,"fullscreen":false,"blackout":false,"timer":{"running":true,"elapsed_seconds":42},"opening":false,"render_state":"ready","notes_state":"ready"}"#).unwrap()
}
#[test]
fn watch_survives_idle_heartbeats_and_leaves_queries_available() {
    let endpoint = Endpoint::new();
    let (server, requests) = ControlServer::bind(&endpoint.path).unwrap();
    let (output, received) = mpsc::channel();
    let path = endpoint.path.clone();
    let watcher = thread::spawn(move || {
        client::watch_to(&path, &Request::new(7, Command::Watch(Empty {})), |event| {
            let end = matches!(event.event, Event::Closed {});
            output.send(event).unwrap();
            if end {
                Err(ControlError::new(ErrorCode::Cancelled, "Test complete"))
            } else {
                Ok(())
            }
        })
    });
    let mut pending = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut hub = EventHub::default();
    assert!(hub.subscribe(pending.watch.take().unwrap()));
    pending
        .response
        .send(Response::success(
            7,
            Reply::Watching {
                state: status(),
                sequence: hub.sequence(),
            },
        ))
        .unwrap();
    assert!(matches!(
        received.recv_timeout(Duration::from_secs(2)).unwrap().event,
        Event::Snapshot { .. }
    ));
    // Exceed the Windows client's per-read deadline; heartbeats must renew it.
    thread::sleep(Duration::from_secs(6));
    assert!(!watcher.is_finished());
    let path = endpoint.path.clone();
    let query = thread::spawn(move || {
        client::send_to(&path, &Request::new(8, Command::TimerElapsed(Empty {}))).unwrap()
    });
    let pending = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(pending.watch.is_none());
    pending
        .response
        .send(Response::success(
            8,
            Reply::TimerElapsed {
                session_id: "test".into(),
                document_revision: 1,
                timer: status().timer,
            },
        ))
        .unwrap();
    assert!(matches!(
        query.join().unwrap().outcome,
        Outcome::Result(Reply::TimerElapsed {
            timer: TimerStatus {
                elapsed_seconds: 42,
                ..
            },
            ..
        })
    ));
    hub.publish(&status(), Event::PageChanged { page: 2, pages: 3 });
    hub.publish(&status(), Event::Closed {});
    assert_eq!(
        received
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .sequence,
        1
    );
    assert_eq!(
        received
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .sequence,
        2
    );
    assert_eq!(
        watcher.join().unwrap().unwrap_err().code,
        ErrorCode::Cancelled
    );
    drop(server);
}
#[test]
fn lagged_watch_reports_a_typed_failure_and_reconnects_with_a_new_baseline() {
    let endpoint = Endpoint::new();
    let (_server, requests) = ControlServer::bind(&endpoint.path).unwrap();
    let path = endpoint.path.clone();
    let watcher = thread::spawn(move || {
        let mut output = Vec::new();
        let result = client::watch_to(&path, &Request::new(1, Command::Watch(Empty {})), |event| {
            output.push(event);
            Ok(())
        });
        (result, output)
    });
    let mut pending = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut hub = EventHub::default();
    assert!(hub.subscribe(pending.watch.take().unwrap()));
    for _ in 0..=events::EVENT_BUFFER {
        hub.publish(&status(), Event::PageChanged { page: 2, pages: 3 });
    }
    pending
        .response
        .send(Response::success(
            1,
            Reply::Watching {
                state: status(),
                sequence: 0,
            },
        ))
        .unwrap();
    let (result, output) = watcher.join().unwrap();
    assert_eq!(result.unwrap_err().code, ErrorCode::EventsLagged);
    assert_eq!(output.len(), 1);
    let path = endpoint.path.clone();
    let replacement = thread::spawn(move || {
        client::watch_to(&path, &Request::new(2, Command::Watch(Empty {})), |event| {
            assert_eq!(event.sequence, events::EVENT_BUFFER as u64 + 1);
            Err(ControlError::new(ErrorCode::Cancelled, "Snapshot received"))
        })
    });
    let mut pending = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(hub.subscribe(pending.watch.take().unwrap()));
    pending
        .response
        .send(Response::success(
            2,
            Reply::Watching {
                state: status(),
                sequence: hub.sequence(),
            },
        ))
        .unwrap();
    assert_eq!(
        replacement.join().unwrap().unwrap_err().code,
        ErrorCode::Cancelled
    );
}
#[test]
fn server_shutdown_finishes_watch_without_waiting_for_the_request_timeout() {
    let endpoint = Endpoint::new();
    let (server, requests) = ControlServer::bind(&endpoint.path).unwrap();
    let (ready, receiver) = mpsc::channel();
    let path = endpoint.path.clone();
    let watcher = thread::spawn(move || {
        client::watch_to(&path, &Request::new(1, Command::Watch(Empty {})), |event| {
            if matches!(event.event, Event::Snapshot { .. }) {
                ready.send(()).unwrap();
            }
            Ok(())
        })
    });
    let mut pending = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut hub = EventHub::default();
    hub.subscribe(pending.watch.take().unwrap());
    pending
        .response
        .send(Response::success(
            1,
            Reply::Watching {
                state: status(),
                sequence: 0,
            },
        ))
        .unwrap();
    receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    let started = std::time::Instant::now();
    drop(server);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(
        watcher.join().unwrap().unwrap_err().code,
        ErrorCode::Cancelled
    );
}

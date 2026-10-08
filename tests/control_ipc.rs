#![cfg(unix)]
use quick_presenter::control::{client, protocol::*, server::ControlServer};
use std::{
    fs,
    io::Write,
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::UnixStream,
    },
    path::PathBuf,
    process::Command as ProcessCommand,
    thread,
    time::Duration,
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "qp-ipc-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn socket(&self) -> PathBuf {
        self.0.join("control.sock")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn status() -> Status {
    Status {
        session_id: "test-session".into(),
        document_revision: 1,
        document: Some("/slides/demo.pdf".into()),
        page: Some(1),
        pages: 5,
        fullscreen: false,
        blackout: false,
        timer: TimerStatus {
            running: false,
            elapsed_seconds: 0,
        },
        opening: false,
        render_state: RenderState::Ready,
        notes_state: NotesState::Ready,
    }
}
#[test]
fn server_correlates_requests_and_keeps_one_owner() {
    let dir = Directory::new();
    let path = dir.socket();
    let (server, receiver) = ControlServer::bind(&path).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(ControlServer::bind(&path).is_err());
    let client = thread::spawn(move || {
        client::send_to(&path, &Request::new(42, Command::Status(Empty {}))).unwrap()
    });
    let pending = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(pending.request.id, 42);
    pending
        .response
        .send(Response::success(42, Reply::Status(status())))
        .unwrap();
    assert_eq!(
        client.join().unwrap(),
        Response::success(42, Reply::Status(status()))
    );
    drop(server);
    assert!(!dir.socket().exists());
    let (_replacement, _) = ControlServer::bind(&dir.socket()).unwrap();
}
#[test]
fn malformed_requests_do_not_enter_presentation_queue() {
    let dir = Directory::new();
    let (server, receiver) = ControlServer::bind(&dir.socket()).unwrap();
    for (request, code) in [
        (
            r#"{"id":7,"protocol_version":1,"method":"unknown","params":{}}"#,
            ErrorCode::UnknownMethod,
        ),
        (
            r#"{"id":7,"protocol_version":999,"method":"presentation.status","params":{}}"#,
            ErrorCode::ProtocolMismatch,
        ),
        ("{", ErrorCode::InvalidRequest),
    ] {
        let mut stream = UnixStream::connect(dir.socket()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .write_all(&(request.len() as u32).to_be_bytes())
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let response: Response = serde_json::from_slice(&read_frame(&mut stream).unwrap()).unwrap();
        assert!(matches!(response.outcome, Outcome::Error(e) if e.code == code));
        assert!(receiver.try_recv().is_err());
    }
    drop(server);
}
#[test]
fn stale_socket_recovers_but_unsafe_paths_are_preserved() {
    let dir = Directory::new();
    let path = dir.socket();
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    drop(listener);
    let (server, _) = ControlServer::bind(&path).unwrap();
    drop(server);
    fs::write(&path, "do not delete").unwrap();
    assert!(ControlServer::bind(&path).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "do not delete");
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(ControlServer::bind(&path).is_err());
}
#[test]
fn slow_or_disconnected_client_does_not_block_other_connections() {
    let dir = Directory::new();
    let path = dir.socket();
    let (server, receiver) = ControlServer::bind(&path).unwrap();
    let mut slow = UnixStream::connect(&path).unwrap();
    slow.write_all(&[0]).unwrap();
    let client = thread::spawn(move || {
        client::send_to(&path, &Request::new(1, Command::Status(Empty {}))).unwrap()
    });
    let pending = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    pending
        .response
        .send(Response::success(1, Reply::Status(status())))
        .unwrap();
    assert!(matches!(client.join().unwrap().outcome, Outcome::Result(_)));
    drop(slow);
    drop(server);
}
#[test]
fn qp_executable_keeps_json_stdout_clean_and_reports_exit_categories() {
    let root = Directory::new();
    let control_dir = root.0.join("quick-presenter");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&control_dir)
        .unwrap();
    let (server, receiver) = ControlServer::bind(&control_dir.join("control.sock")).unwrap();
    let root_path = root.0.clone();
    let child = thread::spawn(move || {
        ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
            .env("XDG_RUNTIME_DIR", root_path)
            .args(["status", "--json"])
            .output()
            .unwrap()
    });
    let pending = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    pending
        .response
        .send(Response::success(
            pending.request.id,
            Reply::Status(status()),
        ))
        .unwrap();
    let output = child.join().unwrap();
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["page"], 1);
    assert_eq!(json["protocol_version"], 1);
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
        .args(["goto", "0", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    let error: ControlError = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error.code, ErrorCode::InvalidPage);
    drop(server);
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
        .env("XDG_RUNTIME_DIR", &root.0)
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let error: ControlError = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error.code, ErrorCode::NotRunning);
}

#[test]
fn qp_context_and_slide_run_through_ipc_with_clean_json() {
    let root = Directory::new();
    let control_dir = root.0.join("quick-presenter");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&control_dir)
        .unwrap();
    let (_server, receiver) = ControlServer::bind(&control_dir.join("control.sock")).unwrap();
    for name in ["slide", "context"] {
        let path = root.0.clone();
        let child = thread::spawn(move || {
            ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
                .env("XDG_RUNTIME_DIR", path)
                .args([name, "--json"])
                .output()
                .unwrap()
        });
        let pending = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        let content = SlideText {
            page: 1,
            text: vec!["PDF source 日本語".into()],
            truncated: false,
        };
        let reply = match pending.request.command {
            Command::Slide(_) => Reply::Slide {
                session_id: "test-session".into(),
                document_revision: 1,
                pages: 5,
                content,
            },
            Command::Context(_) => Reply::Context {
                presentation: status(),
                current: SlideContext {
                    slide: content,
                    notes: "Original note".into(),
                },
                next: None,
            },
            _ => panic!("incorrect CLI command"),
        };
        pending
            .response
            .send(Response::success(pending.request.id, reply))
            .unwrap();
        let output = child.join().unwrap();
        assert!(
            output.status.success(),
            "CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["kind"], name);
        if name == "context" {
            assert_eq!(json["current"]["notes"], "Original note");
            assert!(json["next"].is_null());
        } else {
            assert_eq!(json["text"][0], "PDF source 日本語");
        }
    }
}

#[test]
fn qp_full_queries_reassemble_large_unicode_responses_before_json_output() {
    let root = Directory::new();
    let directory = root.0.join("quick-presenter");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let (_server, receiver) = ControlServer::bind(&directory.join("control.sock")).unwrap();
    for name in ["slide", "context"] {
        let path = root.0.clone();
        let child = thread::spawn(move || {
            ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
                .env("XDG_RUNTIME_DIR", path)
                .args([name, "--full", "--json"])
                .output()
                .unwrap()
        });
        let pending = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(pending.request.command.full_text());
        let source = "日本語".repeat(150_000);
        let slide = SlideText {
            page: 1,
            text: vec![source.clone()],
            truncated: false,
        };
        let result = if name == "slide" {
            Reply::Slide {
                session_id: "test-session".into(),
                document_revision: 1,
                pages: 5,
                content: slide,
            }
        } else {
            Reply::Context {
                presentation: status(),
                current: SlideContext {
                    slide,
                    notes: "original".into(),
                },
                next: None,
            }
        };
        pending
            .response
            .send(Response::success(pending.request.id, result))
            .unwrap();
        let output = child.join().unwrap();
        assert!(
            output.status.success(),
            "CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let content = if name == "slide" {
            &json
        } else {
            &json["current"]
        };
        assert_eq!(content["text"][0], source);
        assert_eq!(content["truncated"], false);
    }
}

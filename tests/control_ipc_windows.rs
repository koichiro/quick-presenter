#![cfg(windows)]

use quick_presenter::control::{client, protocol::*, server::ControlServer, transport};
use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    process::Command as ProcessCommand,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

fn unique_pipe() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    PathBuf::from(format!(
        r"\\.\pipe\quick-presenter-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn status() -> Status {
    Status {
        session_id: "test-session".into(),
        document_revision: 1,
        document: Some("C:\\slides\\demo.pdf".into()),
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
    let path = unique_pipe();
    let (server, receiver) = ControlServer::bind(&path).unwrap();

    let client = thread::spawn({
        let path = path.clone();
        move || client::send_to(&path, &Request::new(42, Command::Status(Empty {}))).unwrap()
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
    assert!(ControlServer::bind(&path).is_err());

    drop(server);
    let (_replacement, _) = ControlServer::bind(&path).unwrap();
}

#[test]
fn slow_client_does_not_block_other_connections() {
    let path = unique_pipe();
    let (server, receiver) = ControlServer::bind(&path).unwrap();
    let mut slow = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    slow.write_all(&[0]).unwrap();

    let client = thread::spawn({
        let path = path.clone();
        move || client::send_to(&path, &Request::new(1, Command::Status(Empty {}))).unwrap()
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
fn malformed_requests_do_not_enter_presentation_queue() {
    let path = unique_pipe();
    let (server, receiver) = ControlServer::bind(&path).unwrap();
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
        let mut stream = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
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
fn qp_executable_keeps_json_stdout_clean_and_reports_exit_categories() {
    let path = transport::endpoint().unwrap();
    let (server, receiver) = ControlServer::bind(&path).unwrap();
    let child = thread::spawn(move || {
        ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
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
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["page"], 1);
    assert_eq!(json["protocol_version"], 1);

    for name in ["slide", "context"] {
        let child = thread::spawn(move || {
            ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
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
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["kind"], name);
        if name == "context" {
            assert_eq!(json["current"]["notes"], "Original note");
        } else {
            assert_eq!(json["text"][0], "PDF source 日本語");
        }
    }

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
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let error: ControlError = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error.code, ErrorCode::NotRunning);
}

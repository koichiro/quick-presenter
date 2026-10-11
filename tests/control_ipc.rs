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
fn qp_export_outputs_all_full_contexts_and_leaves_stdout_empty_on_failure() {
    let root = Directory::new();
    let directory = root.0.join("quick-presenter");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let (_server, receiver) = ControlServer::bind(&directory.join("control.sock")).unwrap();
    for (include_next, fail_page) in [
        (false, None),
        (true, None),
        (false, Some(2)),
        (true, Some(2)),
    ] {
        let runtime = root.0.clone();
        let child = thread::spawn(move || {
            let mut command = ProcessCommand::new(env!("CARGO_BIN_EXE_qp"));
            command
                .env("XDG_RUNTIME_DIR", runtime)
                .args(["export", "--json"]);
            if include_next {
                command.arg("--include-next");
            }
            command.output().unwrap()
        });
        let mut requests = 0;
        loop {
            let pending = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
            requests += 1;
            let result = match &pending.request.command {
                Command::Status(_) => Reply::Status(status()),
                Command::ExportPage(params) => {
                    assert_eq!(params.session_id, "test-session");
                    assert_eq!(params.document_revision, 1);
                    assert!(pending.request.command.full_text());
                    if Some(params.page) == fail_page {
                        pending
                            .response
                            .send(Response::error(
                                Some(pending.request.id),
                                ErrorCode::Cancelled,
                                "PDF changed",
                            ))
                            .unwrap();
                        break;
                    }
                    Reply::Context {
                        presentation: status(),
                        current: SlideContext {
                            slide: SlideText {
                                page: params.page,
                                // Exercise full response transfers as well as clean JSON output.
                                text: if params.page == 3 {
                                    vec!["日本語".repeat(150_000)]
                                } else {
                                    vec![]
                                },
                                truncated: false,
                            },
                            notes: format!("Note {}\n日本語", params.page),
                        },
                        next: None,
                    }
                }
                _ => panic!("export must never navigate"),
            };
            pending
                .response
                .send(Response::success(pending.request.id, result))
                .unwrap();
            if requests == 7 {
                break;
            }
        }
        let output = child.join().unwrap();
        if fail_page.is_some() {
            assert_eq!(output.status.code(), Some(ErrorCode::Cancelled.exit_code()));
            assert!(output.stdout.is_empty());
            let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["code"], "CANCELLED");
        } else {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
            let array: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(array.len(), 5);
            for (index, item) in array.iter().enumerate() {
                assert_eq!(item["protocol_version"], 1);
                assert_eq!(item["kind"], "context");
                assert_eq!(item["current"]["page"], index + 1);
                assert_eq!(item["presentation"]["page"], 1);
                assert_eq!(
                    item["current"]["notes"],
                    format!("Note {}\n日本語", index + 1)
                );
                assert_eq!(
                    item["presentation"],
                    serde_json::to_value(status()).unwrap()
                );
                if include_next {
                    assert!(item.get("next").is_some());
                    if index < 4 {
                        assert_eq!(item["next"], array[index + 1]["current"]);
                    } else {
                        assert!(item["next"].is_null());
                    }
                } else {
                    assert!(item.get("next").is_none());
                }
            }
            assert_eq!(array[2]["current"]["text"][0], "日本語".repeat(150_000));
            assert!(array[0]["current"]["text"].as_array().unwrap().is_empty());
        }
    }
    for args in [&["export", "--JSON"][..], &["export", "--JSON", "--json"]] {
        let output = ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
            .env("XDG_RUNTIME_DIR", &root.0)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(ErrorCode::InvalidRequest.exit_code())
        );
        assert!(output.stdout.is_empty());
        if args.contains(&"--json") {
            let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["code"], "INVALID_REQUEST");
        } else {
            assert!(String::from_utf8_lossy(&output.stderr).starts_with("qp: InvalidRequest:"));
        }
        assert!(
            receiver.try_recv().is_err(),
            "Invalid flags must not reach IPC"
        );
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

#[test]
fn qp_watch_flushes_ndjson_and_keeps_terminal_errors_on_stderr() {
    use quick_presenter::control::events::{EventHub, EVENT_BUFFER};
    let root = Directory::new();
    let directory = root.0.join("quick-presenter");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let (_server, requests) = ControlServer::bind(&directory.join("control.sock")).unwrap();
    for lagged in [false, true] {
        let path = root.0.clone();
        let child = thread::spawn(move || {
            ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
                .env("XDG_RUNTIME_DIR", path)
                .args(["watch", "--json"])
                .output()
                .unwrap()
        });
        let mut pending = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut hub = EventHub::default();
        hub.subscribe(pending.watch.take().unwrap());
        if lagged {
            for _ in 0..=EVENT_BUFFER {
                hub.publish(&status(), Event::PageChanged { page: 2, pages: 5 });
            }
        } else {
            hub.publish(&status(), Event::PageChanged { page: 2, pages: 5 });
        }
        pending
            .response
            .send(Response::success(
                pending.request.id,
                Reply::Watching {
                    state: status(),
                    sequence: 0,
                },
            ))
            .unwrap();
        if !lagged {
            drop(hub);
        }
        let output = child.join().unwrap();
        assert_eq!(output.status.code(), Some(if lagged { 11 } else { 10 }));
        let error: ControlError = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(
            error.code,
            if lagged {
                ErrorCode::EventsLagged
            } else {
                ErrorCode::Cancelled
            }
        );
        let events: Vec<EventEnvelope> = std::str::from_utf8(&output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(matches!(events[0].event, Event::Snapshot { .. }));
        assert_eq!(events.len(), if lagged { 1 } else { 2 });
    }
    for args in [
        ["timer", "start", "--json"],
        ["timer", "stop", "--json"],
        ["timer", "reset", "--json"],
    ] {
        let output = ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
            .env("XDG_RUNTIME_DIR", &root.0)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(requests.try_recv().is_err());
    }
    let path = root.0.clone();
    let query = thread::spawn(move || {
        ProcessCommand::new(env!("CARGO_BIN_EXE_qp"))
            .env("XDG_RUNTIME_DIR", path)
            .args(["timer", "elapsed", "--json"])
            .output()
            .unwrap()
    });
    let pending = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    pending
        .response
        .send(Response::success(
            pending.request.id,
            Reply::TimerElapsed {
                session_id: "test-session".into(),
                document_revision: 1,
                timer: TimerStatus {
                    running: true,
                    elapsed_seconds: 42,
                },
            },
        ))
        .unwrap();
    let output = query.join().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["elapsed_seconds"], 42);
}

//! Exercise the shipped executable, not the test harness, over its pipe protocol.
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[cfg(debug_assertions)]
fn run_broker(mut command: Command) -> std::process::Output {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut reader = stdout;
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut reader = stderr;
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("broker deadline exceeded");
        }
        thread::sleep(Duration::from_millis(10));
    };
    std::process::Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    }
}

#[cfg(debug_assertions)]
fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/marp-speaker-notes.pdf")
}

#[test]
#[cfg(debug_assertions)]
fn broker_deadlines_cover_handshake_open_render_notes_and_graceful_shutdown() {
    for (fault, operation) in [
        ("hang-on-handshake", "Handshake"),
        ("hang-on-open", "Open"),
        ("wait-on-render", "VisibleRender"),
        ("wait-on-render", "AuxiliaryRender"),
        ("hang-on-notes", "Notes"),
        ("hang-on-shutdown", "Shutdown"),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_quick-presenter"));
        command
            .arg("--smoke-open-pdf")
            .arg(fixture())
            .env("QUICK_PRESENTER_HELPER_TEST_FAULT", fault)
            .env("QUICK_PRESENTER_HELPER_TEST_DEADLINE_MS", "1000");
        if operation == "AuxiliaryRender" {
            command.env("QUICK_PRESENTER_HELPER_TEST_AUXILIARY", "1");
        }
        let start = Instant::now();
        let output = run_broker(command);
        assert!(start.elapsed() < Duration::from_secs(8), "{fault}");
        if fault == "hang-on-shutdown" {
            assert!(output.status.success());
        } else {
            assert!(!output.status.success());
        }
        let error = String::from_utf8_lossy(&output.stderr);
        // Drop's shutdown failure is intentionally non-fatal and logged only
        // when diagnostics have been initialized (headless smoke does so).
        if fault != "hang-on-shutdown" {
            assert!(
                error.contains(operation) && error.contains("Timeout"),
                "{error}"
            );
        }
    }
}

#[test]
#[cfg(debug_assertions)]
fn restart_is_bounded_malformed_responses_are_not_retried_and_later_pdf_recovers() {
    let directory = std::env::temp_dir().join(format!(
        "quick-presenter-recovery-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("fault.pdf");
    std::fs::copy(fixture(), &path).unwrap();
    for (fault, attempts) in [
        ("abort-on-render", 1),
        ("wait-on-render", 1),
        ("truncated-on-render", 0),
        ("oversized-on-render", 0),
        ("malformed-on-render", 0),
        ("mismatch-on-render", 0),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_quick-presenter"));
        command
            .arg("--renderer-recovery-smoke")
            .arg(&path)
            .arg(fixture())
            .env("QUICK_PRESENTER_HELPER_TEST_FAULT", fault)
            .env("QUICK_PRESENTER_HELPER_TEST_FAULT_PAGE", "1")
            .env("QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE", "fault.pdf")
            .env("QUICK_PRESENTER_HELPER_TEST_DEADLINE_MS", "1000");
        let output = run_broker(command);
        assert!(
            output.status.success(),
            "{fault}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(
            text.contains(&format!(
                "recovery_result=false restart_attempts={attempts}"
            )),
            "{text}"
        );
        assert!(
            text.contains("later_valid_pdf=true") && text.contains("last_good_bytes=5760000"),
            "{text}"
        );
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!error.contains("SECRET_PDF_NOTE"));
        assert!(!error.contains(&directory.to_string_lossy().to_string()));
    }
    let marker = directory.join("once");
    std::fs::write(&marker, b"test-owned fault marker").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_quick-presenter"));
    command
        .arg("--renderer-recovery-smoke")
        .arg(&path)
        .arg(fixture())
        .env("QUICK_PRESENTER_HELPER_TEST_FAULT", "abort-on-render")
        .env("QUICK_PRESENTER_HELPER_TEST_FAULT_PAGE", "1")
        .env("QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE", "fault.pdf")
        .env("QUICK_PRESENTER_HELPER_TEST_FAULT_ONCE", &marker);
    let output = run_broker(command);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("recovery_result=true restart_attempts=1")
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(debug_assertions)]
fn excessive_allocation_attempt_is_rejected_without_losing_broker() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_quick-presenter"));
    command
        .arg("--smoke-open-pdf")
        .arg(fixture())
        .env("QUICK_PRESENTER_HELPER_TEST_FAULT", "allocate-on-render");
    let output = run_broker(command);
    assert!(!output.status.success() && output.status.code().is_some());
    assert!(String::from_utf8_lossy(&output.stderr).contains("helper allocation limit enforced"));
}

#[test]
fn killed_helper_is_reaped_and_another_helper_can_open() {
    let mut helper = Helper::new(None);
    helper.open();
    helper.child.kill().unwrap();
    assert!(!helper.wait_exit().success());
    assert!(helper.receive().is_err());
    let mut next = Helper::new(None);
    next.open();
    assert_eq!(
        next.request(7, json!({"kind":"Render","page_index":0,"width":320}))
            .0["kind"],
        "Pixels"
    );
}

#[test]
#[cfg(debug_assertions)]
fn scheduler_preserves_active_deck_on_candidate_timeout_or_crash_and_shutdown_interrupts_hang() {
    let directory = std::env::temp_dir().join(format!(
        "quick-presenter-candidate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let candidate = directory.join("fault.pdf");
    std::fs::copy(fixture(), &candidate).unwrap();
    for fault in [
        "hang-on-open",
        "wait-on-render",
        "abort-on-render",
        "malformed-on-render",
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_quick-presenter"));
        command
            .arg("--renderer-scheduler-smoke")
            .arg(&candidate)
            .arg(fixture())
            .env("QUICK_PRESENTER_HELPER_TEST_FAULT", fault)
            .env("QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE", "fault.pdf")
            .env("QUICK_PRESENTER_HELPER_TEST_DEADLINE_MS", "1000");
        let output = run_broker(command);
        assert!(
            output.status.success(),
            "{fault}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout)
            .contains("active_preserved=true shutdown_reaped=true"));
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_quick-presenter"));
    command
        .arg("--renderer-scheduler-smoke")
        .arg(&candidate)
        .arg(fixture())
        .env("QUICK_PRESENTER_HELPER_TEST_FAULT", "wait-on-render")
        .env("QUICK_PRESENTER_HELPER_TEST_FAULT_PAGE", "1")
        .env("QUICK_PRESENTER_HELPER_TEST_SHUTDOWN", "1");
    let output = run_broker(command);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("shutdown_reaped=true"));
    std::fs::remove_dir_all(directory).unwrap();
}

struct Helper {
    child: Child,
    input: Option<ChildStdin>,
    output: mpsc::Receiver<Result<(Value, Vec<u8>), String>>,
    reader: Option<thread::JoinHandle<()>>,
    next: u64,
}
impl Helper {
    fn new(fault: Option<&str>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_quick-presenter"));
        command
            .arg("--renderer-helper")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(fault) = fault {
            command.env("QUICK_PRESENTER_HELPER_TEST_FAULT", fault);
        }
        let mut child = command.spawn().unwrap();
        let input = child.stdin.take();
        let mut stdout = child.stdout.take().unwrap();
        let (sender, output) = mpsc::channel();
        let reader = thread::spawn(move || loop {
            let frame = (|| {
                let mut header = [0; 14];
                stdout.read_exact(&mut header).map_err(|e| e.to_string())?;
                if &header[..4] != b"QPRP" || header[4..6] != 2u16.to_le_bytes() {
                    return Err("protocol mismatch".into());
                }
                let len = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
                let pixels = u32::from_le_bytes(header[10..14].try_into().unwrap()) as usize;
                if len > 1024 * 1024 || pixels > 64 * 1024 * 1024 {
                    return Err("oversized frame".into());
                }
                let mut body = vec![0; len];
                stdout.read_exact(&mut body).map_err(|e| e.to_string())?;
                let body = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
                let mut pixels = vec![0; pixels];
                stdout.read_exact(&mut pixels).map_err(|e| e.to_string())?;
                Ok((body, pixels))
            })();
            let failed = frame.is_err();
            if sender.send(frame).is_err() || failed {
                break;
            }
        });
        Self {
            child,
            input,
            output,
            reader: Some(reader),
            next: 0,
        }
    }
    fn send(&mut self, session: u64, message: Value) -> u64 {
        self.next += 1;
        let control = serde_json::to_vec(
            &json!({"request_id":self.next,"session_id":session,"message":message}),
        )
        .unwrap();
        let input = self.input.as_mut().unwrap();
        input.write_all(b"QPRP").unwrap();
        input.write_all(&2u16.to_le_bytes()).unwrap();
        input
            .write_all(&(control.len() as u32).to_le_bytes())
            .unwrap();
        input.write_all(&0u32.to_le_bytes()).unwrap();
        input.write_all(&control).unwrap();
        input.flush().unwrap();
        self.next
    }
    fn receive(&self) -> Result<(Value, Vec<u8>), String> {
        self.output
            .recv_timeout(Duration::from_secs(15))
            .expect("helper response deadline exceeded")
    }
    fn request(&mut self, session: u64, message: Value) -> (Value, Vec<u8>) {
        let id = self.send(session, message);
        let (body, pixels) = self.receive().unwrap();
        assert_eq!(body["request_id"], id);
        assert_eq!(body["session_id"], session);
        (body["message"].clone(), pixels)
    }
    fn open(&mut self) {
        assert_eq!(self.request(0, json!({"kind":"Hello"})).0["kind"], "Ready");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/marp-speaker-notes.pdf");
        #[cfg(unix)]
        let native = {
            use std::os::unix::ffi::OsStrExt;
            json!({"encoding":"Unix","bytes":path.as_os_str().as_bytes()})
        };
        #[cfg(windows)]
        let native = {
            use std::os::windows::ffi::OsStrExt;
            let bytes: Vec<_> = path
                .as_os_str()
                .encode_wide()
                .flat_map(u16::to_le_bytes)
                .collect();
            json!({"encoding":"Windows","bytes":bytes})
        };
        let (opened, _) = self.request(7, json!({"kind":"Open","path":native}));
        assert_eq!(opened["kind"], "Opened");
        assert_eq!(opened["page_count"], 3);
    }
    fn wait_exit(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "helper survived pipe closure");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.input.take();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn helper_opens_renders_notes_closes_and_shuts_down() {
    let mut helper = Helper::new(None);
    helper.open();
    assert_ne!(helper.child.id(), std::process::id());
    for index in 0..3 {
        let (render, pixels) =
            helper.request(7, json!({"kind":"Render","page_index":index,"width":320}));
        assert_eq!(render["kind"], "Pixels");
        assert_eq!(render["width"], 320);
        assert_eq!(render["format"], "Rgba8");
        assert_eq!(
            pixels.len(),
            320 * render["height"].as_u64().unwrap() as usize * 4
        );
        let (notes, _) = helper.request(7, json!({"kind":"NotesPage","page_index":index}));
        assert_eq!(notes["kind"], "NotesLoaded");
        assert!(notes["pages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["page_number"] == index + 1));
    }
    assert_eq!(
        helper.request(7, json!({"kind":"Close"})).0["kind"],
        "Closed"
    );
    assert_eq!(
        helper.request(0, json!({"kind":"Shutdown"})).0["kind"],
        "Stopped"
    );
    assert!(helper.wait_exit().success());
}

#[test]
#[cfg(debug_assertions)]
fn candidate_abort_or_eof_does_not_interrupt_active_helper() {
    let mut active = Helper::new(None);
    active.open();
    for fault in ["abort-on-render", "eof-on-render", "mismatch-on-render"] {
        let mut candidate = Helper::new(Some(fault));
        candidate.open();
        candidate.send(7, json!({"kind":"Render","page_index":0,"width":320}));
        assert!(candidate.receive().is_err());
        if fault == "abort-on-render" {
            assert!(!candidate.wait_exit().success());
        }
        let (page, _) = active.request(7, json!({"kind":"Render","page_index":1,"width":320}));
        assert_eq!(page["kind"], "Pixels");
    }
}

#[test]
#[cfg(debug_assertions)]
fn input_eof_terminates_helper_even_during_native_work() {
    let mut helper = Helper::new(Some("wait-on-render"));
    helper.open();
    helper.send(7, json!({"kind":"Render","page_index":0,"width":320}));
    thread::sleep(Duration::from_millis(100));
    helper.input.take();
    assert!(helper.wait_exit().success());
}

#[test]
fn helper_rejects_protocol_version_before_opening_pdf() {
    let mut helper = Helper::new(None);
    helper
        .input
        .as_mut()
        .unwrap()
        .write_all(b"QPRP\x01\x00\x01\x00\x00\x00\x00\x00\x00\x00x")
        .unwrap();
    assert!(helper.receive().is_err());
    assert!(!helper.wait_exit().success());
}

#[test]
fn headless_smoke_uses_a_distinct_renderer_process() {
    let output = Command::new(env!("CARGO_BIN_EXE_quick-presenter"))
        .arg("--smoke-open-pdf")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/marp-speaker-notes.pdf"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("through renderer helper"));
    assert!(text.contains("helper_pid="));
}

#[test]
#[cfg(debug_assertions)]
fn helper_faults_do_not_abort_the_broker_executable() {
    for fault in ["abort-on-render", "eof-on-render", "mismatch-on-render"] {
        let output = Command::new(env!("CARGO_BIN_EXE_quick-presenter"))
            .arg("--smoke-open-pdf")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/marp-speaker-notes.pdf"
            ))
            .env("QUICK_PRESENTER_HELPER_TEST_FAULT", fault)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            output.status.code().is_some(),
            "broker terminated abnormally: {fault}"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("Renderer helper failed"));
    }
}

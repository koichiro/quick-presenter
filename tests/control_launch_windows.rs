//! Exercise the shipped CLI's cold-start path over the real Windows Named Pipe.
#![cfg(windows)]

use quick_presenter::control::{client, protocol::*, server::ControlServer};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    },
};

const ROOT_ENV: &str = "QP_WINDOWS_STARTUP_FIXTURE";

struct Installation {
    root: tempfile::TempDir,
}

impl Installation {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("program files 日本語");
        fs::create_dir(&bin).unwrap();
        fs::create_dir(root.path().join("working directory")).unwrap();
        // The test executable re-enters the fixture branch below when launched
        // without arguments as the CLI's companion. No Python, GUI or PDFium.
        fs::copy(
            std::env::current_exe().unwrap(),
            bin.join("quick-presenter.exe"),
        )
        .unwrap();
        let cli = std::env::var_os("QP_STARTUP_TEST_CLI")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_qp")));
        fs::copy(cli, bin.join("qp.exe")).unwrap();
        Self { root }
    }

    fn run(&self, mode: &str, name: &str) -> Output {
        let stdout = self.root.path().join(format!("{name}.stdout"));
        let stderr = self.root.path().join(format!("{name}.stderr"));
        // A Windows descendant can retain inherited pipe handles. File-backed
        // output lets us wait for qp itself, independently of the companion.
        let mut child = ProcessCommand::new(self.root.path().join("program files 日本語/qp.exe"))
            .args(["open", "requested slides 日本語.pdf", "--json"])
            .current_dir(self.root.path().join("working directory"))
            .env(ROOT_ENV, self.root.path())
            .env("QP_WINDOWS_STARTUP_MODE", mode)
            .env("LOCALAPPDATA", self.root.path())
            .stdout(File::create(&stdout).unwrap())
            .stderr(File::create(&stderr).unwrap())
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("CLI did not terminate within 30 seconds in {mode}");
            }
            thread::sleep(Duration::from_millis(20));
        };
        Output {
            status,
            stdout: fs::read(stdout).unwrap(),
            stderr: fs::read(stderr).unwrap(),
        }
    }

    fn requests(&self) -> Vec<Value> {
        fs::read_to_string(self.root.path().join("requests"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

impl Drop for Installation {
    fn drop(&mut self) {
        let Ok(pid) = fs::read_to_string(self.root.path().join("gui.pid")) else {
            return;
        };
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
                0,
                pid.parse().unwrap(),
            )
        };
        if handle.is_null() {
            return;
        }
        // Retain the handle and verify the image before terminating only our
        // disposable companion, even if the recorded PID has been recycled.
        let mut image = vec![0u16; 32768];
        let mut length = image.len() as u32;
        let queried =
            unsafe { QueryFullProcessImageNameW(handle, 0, image.as_mut_ptr(), &mut length) };
        let expected = self
            .root
            .path()
            .join("program files 日本語/quick-presenter.exe");
        if queried != 0
            && PathBuf::from(String::from_utf16_lossy(&image[..length as usize])) == expected
        {
            unsafe {
                TerminateProcess(handle, 0);
                WaitForSingleObject(handle, 5000);
            }
        }
        unsafe { CloseHandle(handle) };
    }
}

fn fixture(root: &Path) {
    fs::write(root.join("gui.pid"), std::process::id().to_string()).unwrap();
    let mut launches = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("launches"))
        .unwrap();
    writeln!(launches, "started").unwrap();
    drop(launches);
    let mode = std::env::var("QP_WINDOWS_STARTUP_MODE").unwrap();
    let (_server, receiver) = ControlServer::start().unwrap();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/control-v1.json")).unwrap();
    let mut state: Status =
        serde_json::from_value(fixture["responses"][0]["result"].clone()).unwrap();
    let mut polls = 0;
    loop {
        let pending = receiver.recv_timeout(Duration::from_secs(60)).unwrap();
        let mut requests = OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("requests"))
            .unwrap();
        serde_json::to_writer(&mut requests, &pending.request).unwrap();
        writeln!(requests).unwrap();
        drop(requests);
        let id = pending.request.id;
        let response = match pending.request.command {
            Command::Status(_) => {
                polls += 1;
                if mode == "late_ready" {
                    thread::sleep(Duration::from_secs(16));
                }
                state.opening = mode == "restoring_forever" || (mode == "restoring" && polls <= 3);
                Response::success(id, Reply::Status(state.clone()))
            }
            Command::Open(params) => {
                if mode == "disconnect" {
                    // Lose the response after receiving the mutation: its
                    // outcome is uncertain and must never trigger a replay.
                    std::process::exit(0);
                }
                if state.opening || mode == "dialog_busy" {
                    Response::error(Some(id), ErrorCode::Busy, "Open already in progress")
                } else {
                    state.document = Some(params.file);
                    Response::success(
                        id,
                        Reply::Mutation {
                            changed: true,
                            state: state.clone(),
                        },
                    )
                }
            }
            _ => panic!("unexpected startup command"),
        };
        let _ = pending.response.send(response);
    }
}

#[test]
fn cold_start_waits_for_restoration_and_never_replays_open() {
    if let Some(root) = std::env::var_os(ROOT_ENV) {
        fixture(Path::new(&root));
        return;
    }
    // Windows deliberately has one fixed machine-wide endpoint. Refuse to
    // interfere with an app already running outside this disposable fixture.
    let error = client::send(&Request::new(1, Command::Status(Empty {}))).unwrap_err();
    assert_eq!(
        error.code,
        ErrorCode::NotRunning,
        "Close Quick Presenter before this test"
    );

    for (mode, exit, polls, opens) in [
        ("restoring", 0, 4, 1),
        ("ready", 0, 1, 1),
        ("restoring_forever", 9, 0, 0),
        ("dialog_busy", 8, 1, 1),
        ("disconnect", 7, 1, 1),
        ("late_ready", 9, 1, 0),
    ] {
        let installation = Installation::new();
        let started = Instant::now();
        let output = installation.run(mode, "first");
        assert_eq!(
            output.status.code(),
            Some(exit),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let requests = installation.requests();
        let status_count = requests
            .iter()
            .filter(|r| r["method"] == "presentation.status")
            .count();
        if mode == "restoring_forever" {
            assert!(status_count > 1);
            assert!(started.elapsed() < Duration::from_secs(25));
        } else {
            assert_eq!(status_count, polls, "{mode}");
        }
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["method"] == "presentation.open")
                .count(),
            opens,
            "{mode}"
        );
        if opens == 1 {
            assert_eq!(requests.last().unwrap()["method"], "presentation.open");
            assert!(requests[..requests.len() - 1]
                .iter()
                .all(|request| request["method"] == "presentation.status"));
        }
        if exit == 0 {
            assert!(output.stderr.is_empty());
            let response: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                Path::new(response["state"]["document"].as_str().unwrap()),
                installation
                    .root
                    .path()
                    .join("working directory/requested slides 日本語.pdf")
            );
            let second = installation.run(mode, "second");
            assert!(
                second.status.success(),
                "{}",
                String::from_utf8_lossy(&second.stderr)
            );
            assert_eq!(installation.requests().len(), requests.len() + 1);
            assert_eq!(
                fs::read_to_string(installation.root.path().join("launches"))
                    .unwrap()
                    .lines()
                    .count(),
                1
            );
        } else {
            assert!(output.stdout.is_empty());
            let response: Value = serde_json::from_slice(&output.stderr).unwrap();
            let code = match exit {
                8 => "BUSY",
                9 => "TIMEOUT",
                _ => "IPC_FAILURE",
            };
            assert_eq!(response["code"], code, "{mode}");
            if mode == "dialog_busy" {
                let second = installation.run(mode, "second");
                assert_eq!(second.status.code(), Some(8));
                assert_eq!(installation.requests().len(), requests.len() + 1);
            }
        }
        println!("Windows startup scenario passed: {mode}");
        // Closing our verified helper process must release the fixed endpoint
        // before the next scenario starts.
        drop(installation);
    }
}

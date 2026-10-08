//! CLI process tests with a fake companion GUI; no window or PDFium required.
#![cfg(unix)]
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Installation {
    root: PathBuf,
}
impl Installation {
    fn new() -> Self {
        let root = PathBuf::from(format!(
            "/tmp/qp-launch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(root.join("bin")).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_qp"), root.join("bin/qp")).unwrap();
        fs::write(
            root.join("bin/quick-presenter"),
            "#!/bin/sh\nexec python3 \"$QP_LAUNCH_FIXTURE\"\n",
        )
        .unwrap();
        fs::set_permissions(
            root.join("bin/quick-presenter"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        Self { root }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(self.root.join("bin/qp"));
        command
            .args(args)
            .current_dir(&self.root)
            .env("XDG_RUNTIME_DIR", &self.root)
            .env(
                "QP_LAUNCH_FIXTURE",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/control-launch-gui.py"
                ),
            )
            .env(
                "QP_CONTRACT_FIXTURE",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/control-v1.json"
                ),
            );
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn requests(&self) -> Vec<Value> {
        fs::read_to_string(self.root.join("requests"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}
impl Drop for Installation {
    fn drop(&mut self) {
        if let Ok(pid) = fs::read_to_string(self.root.join("gui.pid")) {
            unsafe {
                libc::kill(pid.parse().unwrap(), libc::SIGTERM);
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn open_starts_companion_then_sends_once_and_reuses_it() {
    let installation = Installation::new();
    let response = success(installation.run(&["open", "slides with spaces 日本語.pdf", "--json"]));
    assert_eq!(
        response["state"]["document"],
        installation
            .root
            .join("slides with spaces 日本語.pdf")
            .to_str()
            .unwrap()
    );
    success(installation.run(&["open", "second.pdf", "--json"]));
    assert_eq!(
        fs::read_to_string(installation.root.join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let requests = installation.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["method"] == "presentation.open")
            .count(),
        2
    );
    assert_eq!(requests[0]["method"], "presentation.status");
}
#[test]
fn simultaneous_open_calls_launch_only_one_gui() {
    let installation = Installation::new();
    let first = installation
        .command(&["open", "first.pdf", "--json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let second = installation
        .command(&["open", "second.pdf", "--json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    success(first.wait_with_output().unwrap());
    success(second.wait_with_output().unwrap());
    assert_eq!(
        fs::read_to_string(installation.root.join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        installation
            .requests()
            .iter()
            .filter(|r| r["method"] == "presentation.open")
            .count(),
        2
    );
}
#[test]
fn other_commands_never_launch_gui() {
    let installation = Installation::new();
    for command in ["status", "next", "close", "context"] {
        let output = installation.run(&[command, "--json"]);
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
            "NOT_RUNNING"
        );
    }
    assert!(!installation.root.join("gui.pid").exists());
}
#[test]
fn unsafe_endpoint_does_not_launch_gui_or_replace_the_endpoint() {
    let installation = Installation::new();
    let directory = installation.root.join("quick-presenter");
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(directory.join("control.sock"), "keep this file").unwrap();
    let output = installation.run(&["open", "slides.pdf", "--json"]);
    assert_eq!(output.status.code(), Some(7));
    assert!(output.stdout.is_empty());
    assert!(!installation.root.join("gui.pid").exists());
    assert_eq!(
        fs::read_to_string(directory.join("control.sock")).unwrap(),
        "keep this file"
    );
}
#[test]
fn failed_startup_is_reported_without_relaunching_or_opening() {
    let installation = Installation::new();
    let output = installation
        .command(&["open", "slides.pdf", "--json"])
        .env("QP_LAUNCH_MODE", "exit")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
        "NOT_RUNNING"
    );
    assert_eq!(
        fs::read_to_string(installation.root.join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(!installation.root.join("requests").exists());
    fs::remove_file(installation.root.join("gui.pid")).unwrap();
}
#[test]
fn startup_timeout_never_submits_open() {
    let installation = Installation::new();
    let started = std::time::Instant::now();
    let output = installation
        .command(&["open", "slides.pdf", "--json"])
        .env("QP_LAUNCH_MODE", "timeout")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(9));
    assert!(output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
        "TIMEOUT"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(25));
    assert!(!installation.root.join("requests").exists());
}

#[test]
fn server_open_failure_is_returned_without_command_replay_or_relaunch() {
    let installation = Installation::new();
    let output = installation
        .command(&["open", "missing.pdf", "--json"])
        .env("QP_LAUNCH_MODE", "reject_open")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(10));
    assert!(output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
        "OPEN_FAILED"
    );
    assert_eq!(
        installation
            .requests()
            .iter()
            .filter(|r| r["method"] == "presentation.open")
            .count(),
        1
    );
    assert_eq!(
        fs::read_to_string(installation.root.join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

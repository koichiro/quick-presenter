#![cfg(unix)]
// Keep this fork test in its own binary: a paused child also inherits unrelated
// sockets when fork runs alongside other IPC tests. The stale-socket fixture
// also needs isolation from CLI subprocess launches and this fork test.
use quick_presenter::control::server::ControlServer;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    sync::Mutex,
};

static TEST_LOCK: Mutex<()> = Mutex::new(());

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("qp-lock-{}", std::process::id()));
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

#[test]
fn server_releases_lock_while_forked_child_retains_descriptor() {
    let _guard = TEST_LOCK.lock().unwrap();
    struct Child(libc::pid_t);
    impl Drop for Child {
        fn drop(&mut self) {
            // Reap the child even if an assertion fails in the parent.
            unsafe {
                libc::kill(self.0, libc::SIGKILL);
                while libc::waitpid(self.0, std::ptr::null_mut(), 0) == -1
                    && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
                {
                }
            }
        }
    }

    let dir = Directory::new();
    let path = dir.socket();
    let (server, _) = ControlServer::bind(&path).unwrap();
    // Model the fork-to-exec window of another thread launching qp. The child
    // must only call async-signal-safe functions in this multithreaded test.
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork failed: {}", std::io::Error::last_os_error());
    if pid == 0 {
        loop {
            unsafe { libc::pause() };
        }
    }
    let child = Child(pid);
    assert_eq!(
        unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) },
        0
    );
    assert!(ControlServer::bind(&path).is_err());
    drop(server);
    assert!(!path.exists());
    let (_replacement, _) = ControlServer::bind(&path).unwrap();
    // Closing the inherited old descriptor must not release the new owner's lock.
    drop(child);
    assert!(ControlServer::bind(&path).is_err());
}

#[test]
fn stale_socket_recovers_but_unsafe_paths_are_preserved() {
    let _guard = TEST_LOCK.lock().unwrap();
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

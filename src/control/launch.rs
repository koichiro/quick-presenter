//! CLI-only application startup. Presentation operations still use the protocol.
use super::{client, protocol::*, transport};
use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    time::{Duration, Instant},
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Only an open whose initial connection failed may start the companion GUI.
/// Never replay an open after sending it to a reachable server.
pub fn send(request: &Request) -> Result<Response, ControlError> {
    match client::send(request) {
        Err(error)
            if error.code == ErrorCode::NotRunning
                && matches!(request.command, Command::Open(_)) =>
        {
            let endpoint = transport::endpoint().map_err(startup_error)?;
            let gui = companion_gui(&std::env::current_exe().map_err(startup_error)?)?;
            let deadline = Instant::now() + STARTUP_TIMEOUT;
            let status = Request::new(request.id, Command::Status(Empty {}));
            let mut lock = None;
            let mut child = None;
            loop {
                // Recheck after obtaining the startup lock: another qp may have launched it.
                match client::send_to(&endpoint, &status) {
                    Ok(response) => {
                        if let Outcome::Error(error) = response.outcome {
                            return Err(error);
                        }
                        return client::send_to(&endpoint, request);
                    }
                    Err(error) if error.code == ErrorCode::NotRunning => {}
                    Err(error) => return Err(error),
                }
                if Instant::now() >= deadline {
                    return Err(ControlError::new(ErrorCode::Timeout,
                        "Quick Presenter did not expose local control within 15 seconds. Check the GUI diagnostic log."));
                }
                if lock.is_none() {
                    if let Some(acquired) = try_startup_lock(&endpoint).map_err(startup_error)? {
                        lock = Some(acquired);
                        // Probe once more while holding the lock before spawning.
                        continue;
                    }
                } else if child.is_none() {
                    child = Some(spawn_gui(&gui).map_err(startup_error)?);
                }
                if let Some(child) = child.as_mut() {
                    if let Some(exit) = child.try_wait().map_err(startup_error)? {
                        return Err(ControlError::new(ErrorCode::NotRunning,
                            format!("Quick Presenter exited during startup ({exit}). Check the GUI diagnostic log.")));
                    }
                }
                std::thread::sleep(POLL_INTERVAL);
            }
        }
        result => result,
    }
}

fn companion_gui(cli: &Path) -> Result<PathBuf, ControlError> {
    let file = if cfg!(windows) {
        "quick-presenter.exe"
    } else {
        "quick-presenter"
    };
    let gui = cli
        .parent()
        .ok_or_else(|| {
            ControlError::new(
                ErrorCode::NotRunning,
                "Cannot locate the CLI installation directory.",
            )
        })?
        .join(file);
    if !gui.is_file() {
        return Err(ControlError::new(
            ErrorCode::NotRunning,
            format!(
                "Companion GUI not found at {}. Install the complete Quick Presenter package.",
                gui.display()
            ),
        ));
    }
    Ok(gui)
}

fn spawn_gui(path: &Path) -> io::Result<Child> {
    let mut command = std::process::Command::new(path);
    // Inherit the endpoint environment. Open the PDF once, through IPC after readiness.
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000008 | 0x00000200); // Detached process and new process group.
    }
    command.spawn()
}

fn startup_error(error: io::Error) -> ControlError {
    ControlError::new(
        ErrorCode::IpcFailure,
        format!("Could not start Quick Presenter: {error}"),
    )
}

#[cfg(unix)]
fn try_startup_lock(endpoint: &Path) -> io::Result<Option<File>> {
    use std::os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    };
    transport::validate_directory(
        endpoint.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Missing endpoint parent")
        })?,
    )?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(endpoint.with_extension("launch.lock"))?;
    let metadata = lock.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Unsafe GUI startup lock",
        ));
    }
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(Some(lock));
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock {
        Ok(None)
    } else {
        Err(error)
    }
}

#[cfg(windows)]
fn try_startup_lock(_endpoint: &Path) -> io::Result<Option<File>> {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "LOCALAPPDATA is required for GUI startup",
        )
    })?)
    .join("Quick Presenter");
    std::fs::create_dir_all(&directory)?;
    match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(directory.join("control-launch.lock"))
    {
        Ok(lock) => Ok(Some(lock)),
        Err(error) if error.raw_os_error() == Some(32) => Ok(None), // Sharing violation: another qp is launching.
        Err(error) => Err(error),
    }
}

#[cfg(not(any(unix, windows)))]
fn try_startup_lock(_endpoint: &Path) -> io::Result<Option<File>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "GUI startup is unsupported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn companion_is_resolved_from_cli_location_not_working_directory_or_path() {
        let directory = std::env::temp_dir().join(format!("qp-launch-path-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let name = if cfg!(windows) {
            "quick-presenter.exe"
        } else {
            "quick-presenter"
        };
        let gui = directory.join(name);
        std::fs::write(&gui, b"fixture").unwrap();
        assert_eq!(companion_gui(&directory.join("qp")).unwrap(), gui);
        std::fs::remove_file(&gui).unwrap();
        assert_eq!(
            companion_gui(&directory.join("qp")).unwrap_err().code,
            ErrorCode::NotRunning
        );
        std::fs::remove_dir(&directory).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn startup_lock_is_exclusive_reusable_and_rejects_symlinks() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let directory = std::env::temp_dir().join(format!("qp-launch-lock-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        let endpoint = directory.join("control.sock");
        let first = try_startup_lock(&endpoint).unwrap().unwrap();
        assert!(try_startup_lock(&endpoint).unwrap().is_none());
        drop(first);
        assert!(try_startup_lock(&endpoint).unwrap().is_some());
        std::fs::remove_file(endpoint.with_extension("launch.lock")).unwrap();
        symlink(
            directory.join("victim"),
            endpoint.with_extension("launch.lock"),
        )
        .unwrap();
        assert!(try_startup_lock(&endpoint).is_err());
        assert!(!directory.join("victim").exists());
        std::fs::remove_dir_all(&directory).unwrap();
    }
}

use std::{io, path::PathBuf};

#[cfg(unix)]
pub fn endpoint() -> io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let uid = unsafe { libc::geteuid() };
    let base = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(path) => {
            let path = PathBuf::from(path);
            let metadata = std::fs::symlink_metadata(&path)?;
            if !path.is_absolute()
                || !metadata.is_dir()
                || metadata.uid() != uid
                || metadata.mode() & 0o077 != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Unsafe XDG_RUNTIME_DIR",
                ));
            }
            path.join("quick-presenter")
        }
        None => PathBuf::from(format!("/tmp/quick-presenter-{uid}")),
    };
    match std::fs::DirBuilder::new().mode(0o700).create(&base) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    validate_directory(&base)?;
    Ok(base.join("control.sock"))
}
#[cfg(unix)]
pub fn validate_directory(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Control directory must be owned by this user with mode 0700",
        ));
    }
    Ok(())
}
#[cfg(unix)]
pub fn validate_socket(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    validate_directory(
        path.parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing socket parent"))?,
    )?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Unsafe control socket",
        ));
    }
    Ok(())
}
#[cfg(windows)]
pub fn endpoint() -> io::Result<PathBuf> {
    Ok(PathBuf::from(r"\\.\pipe\quick-presenter"))
}

#[cfg(windows)]
pub(crate) mod windows {
    use std::{
        io::{self, Read, Write},
        os::windows::ffi::OsStrExt,
        path::Path,
        ptr, thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
            ERROR_PIPE_LISTENING, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
        },
        Security::{EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
        Storage::FileSystem::{
            CreateFileW, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING,
            PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
        },
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, PeekNamedPipe,
            WaitNamedPipeW, PIPE_NOWAIT, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
        },
        System::Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };

    const POLL_INTERVAL: Duration = Duration::from_millis(10);
    const PIPE_BUFFER_BYTES: u32 = (super::super::protocol::MAX_FRAME_BYTES + 4) as u32;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn open_process_token(process: HANDLE) -> io::Result<OwnedHandle> {
        let mut token = ptr::null_mut();
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(OwnedHandle(token))
        }
    }

    fn token_user(token: HANDLE) -> io::Result<Vec<usize>> {
        let mut required = 0;
        unsafe {
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut required);
        }
        if required == 0 {
            return Err(io::Error::last_os_error());
        }
        let words = (required as usize).div_ceil(std::mem::size_of::<usize>());
        let mut buffer = vec![0usize; words];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            )
        } == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(buffer)
        }
    }

    fn authenticate_server(pipe: HANDLE) -> io::Result<()> {
        let mut server_pid = 0;
        if unsafe { GetNamedPipeServerProcessId(pipe, &mut server_pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let server_process =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, server_pid) };
        if server_process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let server_process = OwnedHandle(server_process);
        let server_token = open_process_token(server_process.0)?;
        let current_token = open_process_token(unsafe { GetCurrentProcess() })?;
        let server_user = token_user(server_token.0)?;
        let current_user = token_user(current_token.0)?;
        let server_sid = unsafe { (*(server_user.as_ptr().cast::<TOKEN_USER>())).User.Sid };
        let current_sid = unsafe { (*(current_user.as_ptr().cast::<TOKEN_USER>())).User.Sid };
        if unsafe { EqualSid(server_sid, current_sid) } == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Control pipe server belongs to another user",
            ));
        }
        Ok(())
    }

    pub struct Pipe {
        handle: HANDLE,
        read_deadline: Instant,
        write_deadline: Instant,
    }

    unsafe impl Send for Pipe {}

    impl Pipe {
        fn new(handle: HANDLE, read_timeout: Duration, write_timeout: Duration) -> Self {
            Self {
                handle,
                read_deadline: Instant::now() + read_timeout,
                write_deadline: Instant::now() + write_timeout,
            }
        }

        pub fn set_deadlines(&mut self, read_timeout: Duration, write_timeout: Duration) {
            self.read_deadline = Instant::now() + read_timeout;
            self.write_deadline = Instant::now() + write_timeout;
        }
    }

    impl Drop for Pipe {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }

    impl Read for Pipe {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if bytes.is_empty() {
                return Ok(0);
            }
            loop {
                let mut available = 0;
                if unsafe {
                    PeekNamedPipe(
                        self.handle,
                        ptr::null_mut(),
                        0,
                        ptr::null_mut(),
                        &mut available,
                        ptr::null_mut(),
                    )
                } == 0
                {
                    return Err(io::Error::last_os_error());
                }
                if available == 0 {
                    if Instant::now() >= self.read_deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "Control read timed out",
                        ));
                    }
                    thread::sleep(POLL_INTERVAL);
                    continue;
                }
                let mut read = 0;
                let ok = unsafe {
                    ReadFile(
                        self.handle,
                        bytes.as_mut_ptr(),
                        bytes.len().min(available as usize).min(u32::MAX as usize) as u32,
                        &mut read,
                        ptr::null_mut(),
                    )
                };
                if ok != 0 {
                    return Ok(read as usize);
                }
                let error = unsafe { GetLastError() };
                if error != ERROR_NO_DATA {
                    return Err(io::Error::from_raw_os_error(error as i32));
                }
                if Instant::now() >= self.read_deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Control read timed out",
                    ));
                }
                thread::sleep(POLL_INTERVAL);
            }
        }
    }

    impl Write for Pipe {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.is_empty() {
                return Ok(0);
            }
            loop {
                let mut written = 0;
                let ok = unsafe {
                    WriteFile(
                        self.handle,
                        bytes.as_ptr(),
                        bytes.len().min(u32::MAX as usize) as u32,
                        &mut written,
                        ptr::null_mut(),
                    )
                };
                if ok != 0 && written != 0 {
                    return Ok(written as usize);
                }
                // PIPE_NOWAIT can succeed with zero bytes when its buffer is
                // full. Retry within the deadline rather than returning a
                // WriteZero error that truncates large response transfers.
                if ok == 0 {
                    let error = unsafe { GetLastError() };
                    if error != ERROR_NO_DATA {
                        return Err(io::Error::from_raw_os_error(error as i32));
                    }
                }
                if Instant::now() >= self.write_deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Control write timed out",
                    ));
                }
                thread::sleep(POLL_INTERVAL);
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    pub fn create_server(path: &Path, first_instance: bool) -> io::Result<Pipe> {
        let name = wide(path);
        let open_mode = PIPE_ACCESS_DUPLEX
            | if first_instance {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            };
        let pipe_mode =
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS;
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                open_mode,
                pipe_mode,
                PIPE_UNLIMITED_INSTANCES,
                PIPE_BUFFER_BYTES,
                PIPE_BUFFER_BYTES,
                0,
                // The Windows default pipe DACL grants write access to the creator,
                // administrators, and LocalSystem, while other users receive read-only access.
                ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Pipe::new(
                handle,
                Duration::from_secs(1),
                Duration::from_secs(1),
            ))
        }
    }

    pub fn accept(pipe: &Pipe, stop: &std::sync::atomic::AtomicBool) -> io::Result<bool> {
        use std::sync::atomic::Ordering;
        while !stop.load(Ordering::Relaxed) {
            if unsafe { ConnectNamedPipe(pipe.handle, ptr::null_mut()) } != 0 {
                return Ok(true);
            }
            match unsafe { GetLastError() } {
                ERROR_PIPE_CONNECTED => return Ok(true),
                ERROR_PIPE_LISTENING => thread::sleep(POLL_INTERVAL),
                ERROR_NO_DATA => return Ok(false),
                error => return Err(io::Error::from_raw_os_error(error as i32)),
            }
        }
        Ok(false)
    }

    pub fn connect(path: &Path, timeout: Duration) -> io::Result<Pipe> {
        let name = wide(path);
        let deadline = Instant::now() + timeout;
        loop {
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    ptr::null(),
                    OPEN_EXISTING,
                    SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                if let Err(error) = authenticate_server(handle) {
                    unsafe {
                        CloseHandle(handle);
                    }
                    return Err(io::Error::new(
                        error.kind(),
                        format!("Failed to authenticate the control pipe server: {error}"),
                    ));
                }
                return Ok(Pipe::new(handle, timeout, timeout));
            }
            let error = unsafe { GetLastError() };
            if error != ERROR_PIPE_BUSY {
                let source = io::Error::from_raw_os_error(error as i32);
                return Err(io::Error::new(
                    source.kind(),
                    format!("Failed to open the control pipe: {source}"),
                ));
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Timed out waiting for an available control pipe",
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let wait_ms = remaining.min(Duration::from_millis(50)).as_millis() as u32;
            unsafe {
                WaitNamedPipeW(name.as_ptr(), wait_ms);
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
pub fn endpoint() -> io::Result<PathBuf> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Local control is not implemented for this platform",
    ))
}

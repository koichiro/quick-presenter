use super::protocol::{Request, Response};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender},
        Arc,
    },
    time::Instant,
};

pub struct PendingRequest {
    pub request: Request,
    pub deadline: Instant,
    pub cancelled: Arc<AtomicBool>,
    pub response: SyncSender<Response>,
    pub watch: Option<super::events::Subscriber>,
}
impl PendingRequest {
    pub fn is_expired(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed) || Instant::now() >= self.deadline
    }
}

// Heartbeats detect disconnected idle clients and keep watches alive beyond request deadlines.
fn stream_events(
    receiver: &super::events::StreamReceiver,
    stop: &AtomicBool,
    id: u64,
    mut write: impl FnMut(&Response) -> std::io::Result<()>,
) -> std::io::Result<()> {
    use super::protocol::{ErrorCode, Reply};
    use std::{sync::mpsc::RecvTimeoutError, time::Duration};
    let mut heartbeat_at = Instant::now() + Duration::from_secs(2);
    loop {
        if receiver.lagged.load(Ordering::Relaxed) {
            return write(&Response::error(
                Some(id),
                ErrorCode::EventsLagged,
                "Event subscriber fell behind; reconnect for a fresh snapshot.",
            ));
        }
        if stop.load(Ordering::Relaxed) {
            return write(&Response::error(
                Some(id),
                ErrorCode::Cancelled,
                "Control server is stopping.",
            ));
        }
        match receiver.events.recv_timeout(Duration::from_millis(50)) {
            Ok(envelope) => write(&Response::success(id, Reply::Event { envelope }))?,
            Err(RecvTimeoutError::Disconnected) => {
                return write(&Response::error(
                    Some(id),
                    if receiver.lagged.load(Ordering::Relaxed) {
                        ErrorCode::EventsLagged
                    } else {
                        ErrorCode::Cancelled
                    },
                    "Event subscription ended; reconnect.",
                ))
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if Instant::now() >= heartbeat_at {
            write(&Response::success(id, Reply::Heartbeat {}))?;
            heartbeat_at = Instant::now() + Duration::from_secs(2);
        }
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use crate::control::{
        protocol::{self, ErrorCode},
        transport,
    };
    use std::sync::mpsc;
    use std::{
        fs::{self, File, OpenOptions},
        io::{self, Read, Write},
        os::{
            fd::AsRawFd,
            unix::{
                fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
                net::{UnixListener, UnixStream},
            },
        },
        path::{Path, PathBuf},
        thread::{self, JoinHandle},
        time::Duration,
    };
    const MAX_CLIENTS: usize = 8;
    const IO_TIMEOUT: Duration = Duration::from_secs(1);

    pub struct ControlServer {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
        path: PathBuf,
        identity: (u64, u64),
        lock: File,
    }
    impl ControlServer {
        pub fn start() -> io::Result<(Self, Receiver<PendingRequest>)> {
            Self::bind(&transport::endpoint()?)
        }
        pub fn bind(path: &Path) -> io::Result<(Self, Receiver<PendingRequest>)> {
            transport::validate_directory(path.parent().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Missing endpoint parent")
            })?)?;
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path.with_extension("lock"))?;
            let meta = lock.metadata()?;
            if !meta.is_file()
                || meta.uid() != unsafe { libc::geteuid() }
                || meta.mode() & 0o077 != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Unsafe control lock",
                ));
            }
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "Another instance owns local control",
                ));
            }
            if fs::symlink_metadata(path).is_ok() {
                transport::validate_socket(path)?;
                // Preserve endpoints held by older servers that do not use this lock.
                match UnixStream::connect(path) {
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AddrInUse,
                            "Control endpoint is active",
                        ))
                    }
                    Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                        fs::remove_file(path)?
                    }
                    Err(e) => return Err(e),
                }
            }
            let listener = UnixListener::bind(path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            listener.set_nonblocking(true)?;
            let meta = fs::symlink_metadata(path)?;
            let identity = (meta.dev(), meta.ino());
            let stop = Arc::new(AtomicBool::new(false));
            let stop_thread = stop.clone();
            let (sender, receiver) = mpsc::sync_channel(MAX_CLIENTS);
            let thread = thread::Builder::new()
                .name("presentation-control".into())
                .spawn(move || {
                    let mut clients: Vec<JoinHandle<()>> = Vec::new();
                    while !stop_thread.load(Ordering::Relaxed) {
                        let mut index = 0;
                        while index < clients.len() {
                            if clients[index].is_finished() {
                                let _ = clients.swap_remove(index).join();
                            } else {
                                index += 1;
                            }
                        }
                        match listener.accept() {
                            Ok((stream, _)) => {
                                if clients.len() >= MAX_CLIENTS {
                                    // Drain the accept backlog without allocating another worker.
                                    drop(stream);
                                    thread::sleep(Duration::from_millis(10));
                                    continue;
                                }
                                let sender = sender.clone();
                                let stop = stop_thread.clone();
                                if let Ok(client) = thread::Builder::new()
                                    .name("control-client".into())
                                    .spawn(move || {
                                        let _ = serve(stream, sender, stop);
                                    })
                                {
                                    clients.push(client);
                                }
                            }
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                thread::sleep(Duration::from_millis(10))
                            }
                            Err(_) => break,
                        }
                    }
                    for client in clients {
                        let _ = client.join();
                    }
                })?;
            Ok((
                Self {
                    stop,
                    thread: Some(thread),
                    path: path.to_owned(),
                    identity,
                    lock,
                },
                receiver,
            ))
        }
    }
    impl Drop for ControlServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            if let Ok(meta) = fs::symlink_metadata(&self.path) {
                if (meta.dev(), meta.ino()) == self.identity {
                    let _ = fs::remove_file(&self.path);
                }
            }
            // A forked child can retain this file until exec even with O_CLOEXEC.
            // Release ownership explicitly, after all workers and the endpoint stop.
            unsafe { libc::flock(self.lock.as_raw_fd(), libc::LOCK_UN) };
        }
    }
    struct DeadlineReader<'a> {
        stream: &'a mut UnixStream,
        deadline: Instant,
    }
    impl Read for DeadlineReader<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let remaining = self
                .deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "Control read timed out"))?;
            self.stream.set_read_timeout(Some(remaining))?;
            self.stream.read(bytes)
        }
    }
    struct DeadlineWriter<'a> {
        stream: &'a mut UnixStream,
        deadline: Instant,
    }
    impl Write for DeadlineWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let remaining = self
                .deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::TimedOut, "Control response timed out")
                })?;
            self.stream.set_write_timeout(Some(remaining))?;
            self.stream.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.stream.flush()
        }
    }
    fn serve(
        mut stream: UnixStream,
        sender: SyncSender<PendingRequest>,
        stop: Arc<AtomicBool>,
    ) -> io::Result<()> {
        // Accepted sockets inherit O_NONBLOCK on macOS. Use blocking I/O with deadlines.
        stream.set_nonblocking(false)?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        // One request per connection. Further clients can reuse the protocol through new connections.
        let bytes = protocol::read_frame(&mut DeadlineReader {
            stream: &mut stream,
            deadline: Instant::now() + IO_TIMEOUT,
        })?;
        let request = match protocol::decode_request(&bytes) {
            Ok(request) => request,
            Err(response) => return protocol::write_frame(&mut stream, &response),
        };
        let id = request.id;
        let (watch, event_receiver) = if matches!(request.command, protocol::Command::Watch(_)) {
            let (subscriber, receiver) = crate::control::events::channel();
            (Some(subscriber), Some(receiver))
        } else {
            (None, None)
        };
        let cancelled = watch
            .as_ref()
            .map(|subscriber| subscriber.closed.clone())
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        let deadline = Instant::now() + protocol::REQUEST_TIMEOUT;
        let (response_sender, receiver) = mpsc::sync_channel(1);
        let pending = PendingRequest {
            request,
            deadline,
            cancelled: cancelled.clone(),
            response: response_sender,
            watch,
        };
        if sender.try_send(pending).is_err() {
            return protocol::write_frame(
                &mut stream,
                &Response::error(
                    Some(id),
                    ErrorCode::Busy,
                    "Control queue is full or unavailable.",
                ),
            );
        }
        loop {
            if stop.load(Ordering::Relaxed) || Instant::now() >= deadline {
                cancelled.store(true, Ordering::Relaxed);
                return protocol::write_frame(
                    &mut stream,
                    &Response::error(
                        Some(id),
                        ErrorCode::Timeout,
                        "Control request timed out; query status before retrying a mutation.",
                    ),
                );
            }
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(response) => {
                    protocol::write_response(
                        &mut DeadlineWriter {
                            stream: &mut stream,
                            deadline,
                        },
                        &response,
                    )?;
                    if matches!(
                        response.outcome,
                        protocol::Outcome::Result(protocol::Reply::Watching { .. })
                    ) {
                        let receiver = event_receiver.as_ref().ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "Missing event subscription")
                        })?;
                        return super::stream_events(receiver, &stop, id, |response| {
                            protocol::write_frame(
                                &mut DeadlineWriter {
                                    stream: &mut stream,
                                    deadline: Instant::now() + IO_TIMEOUT,
                                },
                                response,
                            )
                        });
                    }
                    return Ok(());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return protocol::write_frame(
                        &mut stream,
                        &Response::error(
                            Some(id),
                            ErrorCode::Cancelled,
                            "Presentation request was cancelled.",
                        ),
                    )
                }
            }
        }
    }
}
#[cfg(unix)]
pub use unix::ControlServer;

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::control::{
        protocol::{self, ErrorCode},
        transport::{self, windows as pipe},
    };
    use std::{
        io,
        path::Path,
        sync::mpsc,
        thread::{self, JoinHandle},
        time::Duration,
    };

    const MAX_CLIENTS: usize = 8;
    const IO_TIMEOUT: Duration = Duration::from_secs(1);

    pub struct ControlServer {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl ControlServer {
        pub fn start() -> io::Result<(Self, Receiver<PendingRequest>)> {
            Self::bind(&transport::endpoint()?)
        }

        pub fn bind(path: &Path) -> io::Result<(Self, Receiver<PendingRequest>)> {
            // Creating the first instance synchronously makes endpoint ownership atomic and
            // reports a second GUI instance before its listener thread is started.
            let first = pipe::create_server(path, true)?;
            let stop = Arc::new(AtomicBool::new(false));
            let stop_thread = stop.clone();
            let path = path.to_owned();
            let (sender, receiver) = mpsc::sync_channel(MAX_CLIENTS);
            let thread = thread::Builder::new()
                .name("presentation-control".into())
                .spawn(move || {
                    let mut waiting = Some(first);
                    let mut clients: Vec<JoinHandle<()>> = Vec::new();
                    while !stop_thread.load(Ordering::Relaxed) {
                        let mut index = 0;
                        while index < clients.len() {
                            if clients[index].is_finished() {
                                let _ = clients.swap_remove(index).join();
                            } else {
                                index += 1;
                            }
                        }
                        let Some(pipe) = waiting.take() else {
                            break;
                        };
                        match pipe::accept(&pipe, &stop_thread) {
                            Ok(true) if clients.len() < MAX_CLIENTS => {
                                let sender = sender.clone();
                                let stop = stop_thread.clone();
                                if let Ok(client) = thread::Builder::new()
                                    .name("control-client".into())
                                    .spawn(move || {
                                        let _ = serve(pipe, sender, stop);
                                    })
                                {
                                    clients.push(client);
                                }
                            }
                            Ok(true) | Ok(false) => drop(pipe),
                            Err(_) => break,
                        }
                        if !stop_thread.load(Ordering::Relaxed) {
                            match pipe::create_server(&path, false) {
                                Ok(next) => waiting = Some(next),
                                Err(_) => break,
                            }
                        }
                    }
                    drop(waiting);
                    for client in clients {
                        let _ = client.join();
                    }
                })?;
            Ok((
                Self {
                    stop,
                    thread: Some(thread),
                },
                receiver,
            ))
        }
    }

    impl Drop for ControlServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn serve(
        mut stream: pipe::Pipe,
        sender: SyncSender<PendingRequest>,
        stop: Arc<AtomicBool>,
    ) -> io::Result<()> {
        stream.set_deadlines(IO_TIMEOUT, IO_TIMEOUT);
        // One request per connection. Further clients use new pipe instances.
        let bytes = protocol::read_frame(&mut stream)?;
        let request = match protocol::decode_request(&bytes) {
            Ok(request) => request,
            Err(response) => return protocol::write_frame(&mut stream, &response),
        };
        let id = request.id;
        let (watch, event_receiver) = if matches!(request.command, protocol::Command::Watch(_)) {
            let (subscriber, receiver) = crate::control::events::channel();
            (Some(subscriber), Some(receiver))
        } else {
            (None, None)
        };
        let cancelled = watch
            .as_ref()
            .map(|subscriber| subscriber.closed.clone())
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        let deadline = Instant::now() + protocol::REQUEST_TIMEOUT;
        let (response_sender, receiver) = mpsc::sync_channel(1);
        let pending = PendingRequest {
            request,
            deadline,
            cancelled: cancelled.clone(),
            response: response_sender,
            watch,
        };
        if sender.try_send(pending).is_err() {
            return protocol::write_frame(
                &mut stream,
                &Response::error(
                    Some(id),
                    ErrorCode::Busy,
                    "Control queue is full or unavailable.",
                ),
            );
        }
        loop {
            if stop.load(Ordering::Relaxed) || Instant::now() >= deadline {
                cancelled.store(true, Ordering::Relaxed);
                return protocol::write_frame(
                    &mut stream,
                    &Response::error(
                        Some(id),
                        ErrorCode::Timeout,
                        "Control request timed out; query status before retrying a mutation.",
                    ),
                );
            }
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(response) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    stream.set_deadlines(remaining, remaining);
                    protocol::write_response(&mut stream, &response)?;
                    if matches!(
                        response.outcome,
                        protocol::Outcome::Result(protocol::Reply::Watching { .. })
                    ) {
                        let receiver = event_receiver.as_ref().ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "Missing event subscription")
                        })?;
                        return super::stream_events(receiver, &stop, id, |response| {
                            stream.set_deadlines(IO_TIMEOUT, IO_TIMEOUT);
                            protocol::write_frame(&mut stream, response)
                        });
                    }
                    return Ok(());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return protocol::write_frame(
                        &mut stream,
                        &Response::error(
                            Some(id),
                            ErrorCode::Cancelled,
                            "Presentation request was cancelled.",
                        ),
                    )
                }
            }
        }
    }
}

#[cfg(windows)]
pub use windows::ControlServer;

#[cfg(not(any(unix, windows)))]
pub struct ControlServer;
#[cfg(not(any(unix, windows)))]
impl ControlServer {
    pub fn start() -> std::io::Result<(Self, Receiver<PendingRequest>)> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Local control is not implemented for this platform",
        ))
    }
}

use quick_presenter::control::{protocol, transport};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, CString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const SERVER_ID: &str = "app.quickpresenter.ipc-probe.server";
const CLIENT_ID: &str = "app.quickpresenter.ipc-probe.client";
const IO_BUDGET: Duration = Duration::from_secs(1);
const FRAME_LIMIT: usize = 4096;

#[repr(C)]
#[derive(Clone)]
struct Context {
    path: [i8; 1024],
    team: [i8; 64],
    group: [i8; 128],
}
#[repr(C)]
#[derive(Default, PartialEq, Eq)]
struct Peer {
    token: [u32; 8],
}
#[link(name = "store_ipc_probe", kind = "static")]
extern "C" {
    fn qp_probe_context(context: *mut Context, status: *mut i32) -> i32;
    fn qp_probe_peer(
        fd: i32,
        identifier: *const i8,
        context: *const Context,
        peer: *mut Peer,
        status: *mut i32,
    ) -> i32;
}
#[link(name = "Foundation", kind = "framework")]
extern "C" {}
#[link(name = "bsm")]
extern "C" {}

fn native_result(stage: i32, status: i32) -> io::Result<()> {
    if stage == 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("native identity/container check failed (stage={stage}, status={status})"),
        ))
    }
}

fn context() -> io::Result<Context> {
    let mut context = Context {
        path: [0; 1024],
        team: [0; 64],
        group: [0; 128],
    };
    let mut status = 0;
    // The bridge owns all CF objects and writes only within fixed-size buffers.
    let stage = unsafe { qp_probe_context(&mut context, &mut status) };
    native_result(stage, status)?;
    Ok(context)
}

fn authenticate(
    stream: &UnixStream,
    context: &Context,
    identifier: &str,
    previous: Option<&Peer>,
) -> io::Result<Peer> {
    let identifier = CString::new(identifier).expect("constant signing identifier");
    let mut peer = Peer::default();
    let mut status = 0;
    let stage = unsafe {
        qp_probe_peer(
            stream.as_raw_fd(),
            identifier.as_ptr(),
            context,
            &mut peer,
            &mut status,
        )
    };
    native_result(stage, status)?;
    if previous.is_some_and(|previous| previous != &peer) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "peer token changed",
        ));
    }
    Ok(peer)
}

fn endpoint(context: &Context) -> io::Result<PathBuf> {
    let root = unsafe { CStr::from_ptr(context.path.as_ptr()) };
    let root = Path::new(std::ffi::OsStr::from_bytes(root.to_bytes()));
    let dir = root.join("c");
    let path = dir.join("s");
    validate_length(&path)?;
    match fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error),
    }
    transport::validate_directory(&dir)?;
    Ok(path)
}

fn validate_length(path: &Path) -> io::Result<()> {
    // Darwin sockaddr_un has a 104-byte sun_path, including the trailing NUL.
    if !path.is_absolute()
        || path.as_os_str().as_bytes().contains(&0)
        || path.as_os_str().as_bytes().len() >= 104
    {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid or overlong App Group socket path",
        ))
    } else {
        Ok(())
    }
}

struct EndpointOwner {
    path: PathBuf,
    identity: (u64, u64),
    _lock: File,
}
impl EndpointOwner {
    fn bind(path: &Path) -> io::Result<(Self, UnixListener)> {
        transport::validate_directory(path.parent().expect("endpoint parent"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path.with_extension("lock"))?;
        let meta = lock.metadata()?;
        if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unsafe probe lock",
            ));
        }
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error());
        }
        match fs::symlink_metadata(path) {
            Ok(_) => {
                transport::validate_socket(path)?;
                match UnixStream::connect(path) {
                    Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                        fs::remove_file(path)?
                    }
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AddrInUse,
                            "live probe endpoint",
                        ))
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        let listener = UnixListener::bind(path)?;
        let meta = fs::symlink_metadata(path)?;
        let owner = Self {
            path: path.to_owned(),
            identity: (meta.dev(), meta.ino()),
            _lock: lock,
        };
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        Ok((owner, listener))
    }
}
impl Drop for EndpointOwner {
    fn drop(&mut self) {
        if let Ok(meta) = fs::symlink_metadata(&self.path) {
            if meta.file_type().is_socket() && (meta.dev(), meta.ino()) == self.identity {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

// Limit total frame duration, rather than resetting a timeout for every byte.
struct DeadlineIo<'a> {
    stream: &'a mut UnixStream,
    deadline: Instant,
}
impl DeadlineIo<'_> {
    fn remaining(&self) -> io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|value| !value.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "probe frame deadline"))
    }
}
fn wait_for(stream: &UnixStream, events: i16, deadline: Instant) -> io::Result<()> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|value| !value.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "probe IO deadline"))?;
        let millis = remaining
            .as_millis()
            .saturating_add(1)
            .min(i32::MAX as u128) as i32;
        let mut fd = libc::pollfd {
            fd: stream.as_raw_fd(),
            events,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut fd, 1, millis) };
        if result > 0 {
            return Ok(());
        }
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}
impl Read for DeadlineIo<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            self.remaining()?;
            match self.stream.read(bytes) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait_for(self.stream, libc::POLLIN, self.deadline)?
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                result => return result,
            }
        }
    }
}
impl Write for DeadlineIo<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            self.remaining()?;
            match self.stream.write(bytes) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait_for(self.stream, libc::POLLOUT, self.deadline)?
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Message {
    probe: u32,
    sequence: u32,
    count: u32,
}
impl Message {
    fn validate(&self) -> io::Result<()> {
        if self.probe != 1 || self.sequence != 0 || !(1..=30).contains(&self.count) {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid probe request",
            ))
        } else {
            Ok(())
        }
    }
}
fn read(stream: &mut UnixStream) -> io::Result<Message> {
    stream.set_nonblocking(true)?;
    let mut reader = DeadlineIo {
        stream,
        deadline: Instant::now() + IO_BUDGET,
    };
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > FRAME_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid probe frame length",
        ));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
fn write(stream: &mut UnixStream, message: &Message) -> io::Result<()> {
    stream.set_nonblocking(true)?;
    protocol::write_frame(
        &mut DeadlineIo {
            stream,
            deadline: Instant::now() + IO_BUDGET,
        },
        message,
    )
}

fn serve(
    mut stream: UnixStream,
    context: Context,
    stop_at: Instant,
    watchers: Arc<AtomicUsize>,
) -> io::Result<()> {
    stream.set_nonblocking(true)?;
    let peer = authenticate(&stream, &context, CLIENT_ID, None)?;
    let request = read(&mut stream)?;
    request.validate()?;
    authenticate(&stream, &context, CLIENT_ID, Some(&peer))?;
    let _watch = if request.count > 1 {
        Some(WatchSlot::acquire(watchers)?)
    } else {
        None
    };
    for sequence in 0..request.count {
        if Instant::now() >= stop_at {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "probe server stopping",
            ));
        }
        authenticate(&stream, &context, CLIENT_ID, Some(&peer))?;
        let response = Message {
            probe: 1,
            sequence,
            count: request.count,
        };
        write(&mut stream, &response)?;
        // Keep the peer alive while the client validates the complete response.
        let acknowledgement = read(&mut stream)?;
        authenticate(&stream, &context, CLIENT_ID, Some(&peer))?;
        if acknowledgement != response {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid probe acknowledgement",
            ));
        }
        if sequence + 1 < request.count {
            thread::sleep(Duration::from_secs(2));
        }
    }
    Ok(())
}
struct WatchSlot(Arc<AtomicUsize>);
impl WatchSlot {
    fn acquire(count: Arc<AtomicUsize>) -> io::Result<Self> {
        let mut current = count.load(Ordering::Relaxed);
        loop {
            if current >= 4 {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "probe watcher limit",
                ));
            }
            match count.compare_exchange_weak(
                current,
                current + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(value) => current = value,
            }
        }
        Ok(Self(count))
    }
}
impl Drop for WatchSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn server(context: Context, seconds: u32) -> io::Result<()> {
    let path = endpoint(&context)?;
    let (_owner, listener) = EndpointOwner::bind(&path)?;
    let stop_at = Instant::now() + Duration::from_secs(seconds.into());
    let watchers = Arc::new(AtomicUsize::new(0));
    let mut workers: Vec<thread::JoinHandle<io::Result<()>>> = Vec::new();
    println!(
        "{}",
        serde_json::json!({"probe":1,"event":"listening","seconds":seconds})
    );
    while Instant::now() < stop_at {
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                report(workers.swap_remove(index).join());
            } else {
                index += 1;
            }
        }
        match listener.accept() {
            Ok((stream, _)) if workers.len() < 8 => {
                let context = context.clone();
                let watchers = watchers.clone();
                workers.push(
                    thread::Builder::new()
                        .name("store-ipc-probe".into())
                        .spawn(move || serve(stream, context, stop_at, watchers))?,
                );
            }
            Ok(_) => eprintln!("{}", serde_json::json!({"probe":1,"error":"client limit"})),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => return Err(error),
        }
    }
    drop(listener);
    for worker in workers {
        report(worker.join());
    }
    Ok(())
}
fn report(result: thread::Result<io::Result<()>>) {
    match result {
        Ok(Ok(())) => eprintln!("{}", serde_json::json!({"probe":1,"event":"completed"})),
        Ok(Err(error)) => eprintln!(
            "{}",
            serde_json::json!({"probe":1,"error":error.to_string()})
        ),
        Err(_) => eprintln!(
            "{}",
            serde_json::json!({"probe":1,"error":"worker panicked"})
        ),
    }
}
fn client(context: Context, count: u32) -> io::Result<()> {
    let path = endpoint(&context)?;
    transport::validate_socket(&path)?;
    let mut stream = UnixStream::connect(path)?;
    let peer = authenticate(&stream, &context, SERVER_ID, None)?;
    write(
        &mut stream,
        &Message {
            probe: 1,
            sequence: 0,
            count,
        },
    )
    .map_err(|error| io::Error::new(error.kind(), format!("request write: {error}")))?;
    for sequence in 0..count {
        // Heartbeats are two seconds apart; frame arrival still has a bounded total budget.
        wait_for(
            &stream,
            libc::POLLIN,
            Instant::now() + Duration::from_secs(3),
        )?;
        let response = read(&mut stream)
            .map_err(|error| io::Error::new(error.kind(), format!("response read: {error}")))?;
        authenticate(&stream, &context, SERVER_ID, Some(&peer))?;
        if response
            != (Message {
                probe: 1,
                sequence,
                count,
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected probe response",
            ));
        }
        write(&mut stream, &response)?;
        println!("{}", serde_json::to_string(&response)?);
        io::stdout().flush()?;
    }
    Ok(())
}
fn options(args: &[String]) -> io::Result<(&str, u32)> {
    if args.len() == 2 && matches!(args[0].as_str(), "server" | "client") {
        if let Ok(value) = args[1].parse::<u32>() {
            let max = if args[0] == "server" { 300 } else { 30 };
            if (1..=max).contains(&value) {
                return Ok((&args[0], value));
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: store_ipc_probe server <1..300 seconds> | client <1..30 replies>",
    ))
}
pub fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let (role, value) = options(&args)?;
    let context = context()?;
    if role == "server" {
        server(context, value)
    } else {
        client(context, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "qp-ipc-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn endpoint_ownership_recovers_stale_sockets_but_preserves_unexpected_files() {
        let dir = TestDirectory::new();
        let path = dir.0.join("s");
        let listener = UnixListener::bind(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        drop(listener);
        let (owner, listener) = EndpointOwner::bind(&path).unwrap();
        assert!(EndpointOwner::bind(&path).is_err());
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"replacement").unwrap();
        drop(listener);
        drop(owner);
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        assert!(EndpointOwner::bind(&path).is_err());
    }
    #[test]
    fn invalid_frames_fail_before_allocating_the_declared_body() {
        for size in [0_u32, FRAME_LIMIT as u32 + 1, u32::MAX] {
            let (mut sender, mut receiver) = UnixStream::pair().unwrap();
            sender.write_all(&size.to_be_bytes()).unwrap();
            assert_eq!(
                read(&mut receiver).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
    #[test]
    fn incomplete_frames_have_a_total_deadline() {
        let (mut sender, mut receiver) = UnixStream::pair().unwrap();
        sender.write_all(&16_u32.to_be_bytes()).unwrap();
        sender.write_all(b"{").unwrap();
        let start = Instant::now();
        let error = read(&mut receiver).unwrap_err();
        assert!(matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ));
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn paths_use_byte_limits_and_reject_relative_or_nul_paths() {
        assert!(validate_length(Path::new(&format!("/{}", "x".repeat(102)))).is_ok());
        assert!(validate_length(Path::new(&format!("/{}", "x".repeat(103)))).is_err());
        assert!(validate_length(Path::new(&format!("/{}", "界".repeat(35)))).is_err());
        assert!(validate_length(Path::new("relative")).is_err());
        assert!(validate_length(Path::new("/a\0b")).is_err());
    }
    #[test]
    fn requests_bound_watch_duration_and_reject_other_versions() {
        for (probe, sequence, count) in [(0, 0, 1), (1, 1, 1), (1, 0, 0), (1, 0, 31)] {
            assert!(Message {
                probe,
                sequence,
                count
            }
            .validate()
            .is_err());
        }
        assert!(Message {
            probe: 1,
            sequence: 0,
            count: 30
        }
        .validate()
        .is_ok());
    }
    #[test]
    fn watcher_slots_are_released_on_failure_or_completion() {
        let count = Arc::new(AtomicUsize::new(0));
        let mut slots: Vec<_> = (0..4)
            .map(|_| WatchSlot::acquire(count.clone()).unwrap())
            .collect();
        assert!(WatchSlot::acquire(count.clone()).is_err());
        slots.pop();
        assert!(WatchSlot::acquire(count.clone()).is_ok());
        drop(slots);
        assert_eq!(count.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn invalid_options_cannot_disable_checks_or_run_unbounded() {
        for args in [
            vec!["server", "0"],
            vec!["server", "301"],
            vec!["client", "31"],
            vec!["client", "1", "--unsigned"],
        ] {
            assert!(options(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
        }
    }
}

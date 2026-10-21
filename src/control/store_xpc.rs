//! Mac App Store control transport. The native layer authenticates every XPC
//! message; Rust retains Protocol v1, dispatch, and presentation ownership.
use super::{
    events, protocol,
    protocol::{Command, ErrorCode, Outcome, Reply, Response},
    server::{stream_events, PendingRequest},
};
use std::{
    ffi::c_void,
    io::{self, Read, Write},
    panic::{catch_unwind, AssertUnwindSafe},
    ptr,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};

const MAX_CLIENTS: usize = 8;

unsafe extern "C" {
    fn qp_control_broker_main() -> i32;
    fn qp_control_gui_start(
        request: extern "C" fn(*mut c_void, *const c_void, usize, *mut c_void) -> *mut c_void,
        cancel: extern "C" fn(*mut c_void),
        context: *mut c_void,
    ) -> *mut c_void;
    fn qp_control_gui_stop(handle: *mut c_void);
    fn qp_control_peer_send(peer: *mut c_void, bytes: *const c_void, length: usize) -> i32;
    fn qp_control_peer_retain(peer: *mut c_void);
    fn qp_control_peer_release(peer: *mut c_void);
    fn qp_control_client_connect(error: *mut i32) -> *mut c_void;
    fn qp_control_client_send(handle: *mut c_void, bytes: *const c_void, length: usize) -> i32;
    fn qp_control_client_next(
        handle: *mut c_void,
        buffer: *mut c_void,
        capacity: usize,
        timeout_ms: u32,
    ) -> isize;
    fn qp_control_client_close(handle: *mut c_void);
    fn qp_control_launch_gui(app_path: *const std::ffi::c_char) -> i32;
}

pub fn launch_gui(app: &std::path::Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(app.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid app path"))?;
    let error = unsafe { qp_control_launch_gui(path.as_ptr()) };
    if error == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(error))
    }
}

pub fn broker_main() -> i32 {
    unsafe { qp_control_broker_main() }
}

struct ServerState {
    sender: SyncSender<PendingRequest>,
    stop: Arc<AtomicBool>,
    active: AtomicUsize,
}
static SERVER: OnceLock<ServerState> = OnceLock::new();

pub struct StoreServer {
    handle: usize,
    stop: Arc<AtomicBool>,
}

impl StoreServer {
    pub fn start() -> io::Result<(Self, Receiver<PendingRequest>)> {
        let (sender, receiver) = mpsc::sync_channel(MAX_CLIENTS);
        let stop = Arc::new(AtomicBool::new(false));
        SERVER
            .set(ServerState {
                sender,
                stop: stop.clone(),
                active: AtomicUsize::new(0),
            })
            .map_err(|_| io::Error::new(io::ErrorKind::AlreadyExists, "Store control started"))?;
        let context = SERVER.get().unwrap() as *const ServerState as *mut c_void;
        let handle = unsafe { qp_control_gui_start(on_request, on_cancel, context) };
        if handle.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Store control registration or signing identity failed",
            ));
        }
        Ok((
            Self {
                handle: handle as usize,
                stop,
            },
            receiver,
        ))
    }
}

impl Drop for StoreServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        unsafe { qp_control_gui_stop(self.handle as *mut c_void) };
    }
}

struct Session {
    cancelled: Arc<AtomicBool>,
}

extern "C" fn on_request(
    peer: *mut c_void,
    data: *const c_void,
    length: usize,
    context: *mut c_void,
) -> *mut c_void {
    catch_unwind(AssertUnwindSafe(|| {
        if peer.is_null()
            || data.is_null()
            || context.is_null()
            || length > protocol::MAX_FRAME_BYTES
        {
            return ptr::null_mut();
        }
        let state = unsafe { &*(context as *const ServerState) };
        if state.stop.load(Ordering::Relaxed) {
            return ptr::null_mut();
        }
        if state
            .active
            .try_update(Ordering::AcqRel, Ordering::Relaxed, |active| {
                (active < MAX_CLIENTS).then_some(active + 1)
            })
            .is_err()
        {
            return ptr::null_mut();
        }
        let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length) }.to_vec();
        let session = Arc::new(Session {
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        let worker_session = session.clone();
        let native_session = Arc::into_raw(session) as *mut c_void;
        let peer_value = peer as usize;
        unsafe { qp_control_peer_retain(peer) };
        let spawned = std::thread::Builder::new()
            .name("store-control-client".into())
            .spawn(move || {
                let peer = peer_value as *mut c_void;
                let _ = serve(peer, &bytes, state, &worker_session);
                unsafe { qp_control_peer_release(peer) };
                state.active.fetch_sub(1, Ordering::AcqRel);
            });
        if spawned.is_err() {
            unsafe {
                qp_control_peer_release(peer);
                drop(Arc::from_raw(native_session as *const Session));
            }
            state.active.fetch_sub(1, Ordering::AcqRel);
            return ptr::null_mut();
        }
        native_session
    }))
    .unwrap_or(ptr::null_mut())
}

extern "C" fn on_cancel(session: *mut c_void) {
    if !session.is_null() {
        let session = unsafe { Arc::from_raw(session as *const Session) };
        session.cancelled.store(true, Ordering::Relaxed);
    }
}

fn serve(
    peer: *mut c_void,
    bytes: &[u8],
    state: &ServerState,
    session: &Session,
) -> io::Result<()> {
    let mut writer = XpcWriter::new(peer);
    let request = match protocol::decode_request(bytes) {
        Ok(request) => request,
        Err(response) => return protocol::write_frame(&mut writer, &response),
    };
    let id = request.id;
    let (watch, event_receiver) = if matches!(request.command, Command::Watch(_)) {
        let (subscriber, receiver) = events::channel();
        (Some(subscriber), Some(receiver))
    } else {
        (None, None)
    };
    let cancelled = watch
        .as_ref()
        .map(|subscriber| subscriber.closed.clone())
        .unwrap_or_else(|| session.cancelled.clone());
    let deadline = Instant::now()
        + if matches!(request.command, Command::Open(_)) {
            protocol::STORE_OPEN_TIMEOUT
        } else {
            protocol::REQUEST_TIMEOUT
        };
    let (response_sender, receiver) = mpsc::sync_channel(1);
    let pending = PendingRequest {
        request,
        deadline,
        cancelled: cancelled.clone(),
        response: response_sender,
        watch,
    };
    if state.sender.try_send(pending).is_err() {
        return protocol::write_frame(
            &mut writer,
            &Response::error(
                Some(id),
                ErrorCode::Busy,
                "Control queue is full or unavailable.",
            ),
        );
    }
    loop {
        if state.stop.load(Ordering::Relaxed)
            || session.cancelled.load(Ordering::Relaxed)
            || Instant::now() >= deadline
        {
            cancelled.store(true, Ordering::Relaxed);
            return protocol::write_frame(
                &mut writer,
                &Response::error(
                    Some(id),
                    ErrorCode::Timeout,
                    "Control request timed out; query status before retrying a mutation.",
                ),
            );
        }
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(response) => {
                protocol::write_response(&mut writer, &response)?;
                if matches!(response.outcome, Outcome::Result(Reply::Watching { .. })) {
                    let receiver = event_receiver.as_ref().ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "Missing event subscription")
                    })?;
                    return stream_events(receiver, &state.stop, id, |response| {
                        if session.cancelled.load(Ordering::Relaxed) {
                            return Err(io::Error::new(
                                io::ErrorKind::BrokenPipe,
                                "Store CLI disconnected",
                            ));
                        }
                        protocol::write_frame(&mut writer, response)
                    });
                }
                return Ok(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return protocol::write_frame(
                    &mut writer,
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

struct FrameEncoder {
    pending: Vec<u8>,
}

impl FrameEncoder {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    fn push(
        &mut self,
        bytes: &[u8],
        mut send: impl FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        self.pending.extend_from_slice(bytes);
        loop {
            if self.pending.len() < 4 {
                return Ok(());
            }
            let length = u32::from_be_bytes(self.pending[..4].try_into().unwrap()) as usize;
            if length == 0 || length > protocol::MAX_FRAME_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid XPC control frame length",
                ));
            }
            if self.pending.len() < length + 4 {
                return Ok(());
            }
            send(&self.pending[4..length + 4])?;
            self.pending.drain(..length + 4);
        }
    }
}

struct XpcWriter {
    peer: usize,
    frames: FrameEncoder,
}

impl XpcWriter {
    fn new(peer: *mut c_void) -> Self {
        Self {
            peer: peer as usize,
            frames: FrameEncoder::new(),
        }
    }
}

impl Write for XpcWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let peer = self.peer as *mut c_void;
        self.frames.push(bytes, |frame| {
            let code = unsafe { qp_control_peer_send(peer, frame.as_ptr().cast(), frame.len()) };
            if code == 0 {
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(-code))
            }
        })?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct XpcStream {
    handle: usize,
    outgoing: FrameEncoder,
    incoming: Vec<u8>,
    position: usize,
    read_timeout: Duration,
}

impl XpcStream {
    pub fn connect() -> io::Result<Self> {
        let mut error = libc::ENOENT;
        let handle = unsafe { qp_control_client_connect(&mut error) };
        if handle.is_null() {
            return Err(io::Error::from_raw_os_error(error));
        }
        Ok(Self {
            handle: handle as usize,
            outgoing: FrameEncoder::new(),
            incoming: Vec::new(),
            position: 0,
            read_timeout: protocol::REQUEST_TIMEOUT + Duration::from_secs(2),
        })
    }

    pub fn set_read_timeout(&mut self, timeout: Duration) {
        self.read_timeout = timeout;
    }
}

impl Drop for XpcStream {
    fn drop(&mut self) {
        unsafe { qp_control_client_close(self.handle as *mut c_void) };
    }
}

impl Write for XpcStream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let handle = self.handle as *mut c_void;
        self.outgoing.push(bytes, |frame| {
            let code =
                unsafe { qp_control_client_send(handle, frame.as_ptr().cast(), frame.len()) };
            if code == 0 {
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(-code))
            }
        })?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for XpcStream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.position >= self.incoming.len() {
            let mut frame = vec![0u8; protocol::MAX_FRAME_BYTES];
            let timeout_ms = self.read_timeout.as_millis().min(u32::MAX as u128) as u32;
            let result = unsafe {
                qp_control_client_next(
                    self.handle as *mut c_void,
                    frame.as_mut_ptr().cast(),
                    frame.len(),
                    timeout_ms,
                )
            };
            if result < 0 {
                return Err(io::Error::from_raw_os_error(-result as i32));
            }
            let length = result as usize;
            frame.truncate(length);
            self.incoming = Vec::with_capacity(length + 4);
            self.incoming
                .extend_from_slice(&(length as u32).to_be_bytes());
            self.incoming.extend_from_slice(&frame);
            self.position = 0;
        }
        let available = &self.incoming[self.position..];
        let length = available.len().min(bytes.len());
        bytes[..length].copy_from_slice(&available[..length]);
        self.position += length;
        Ok(length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_rejects_oversize_before_sending() {
        let mut encoder = FrameEncoder::new();
        let header = ((protocol::MAX_FRAME_BYTES + 1) as u32).to_be_bytes();
        assert_eq!(
            encoder.push(&header, |_| Ok(())).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn framing_waits_for_complete_frame_and_preserves_order() {
        let mut encoder = FrameEncoder::new();
        let mut sent = Vec::new();
        encoder
            .push(&[0, 0, 0, 3, b'a'], |frame| {
                sent.push(frame.to_vec());
                Ok(())
            })
            .unwrap();
        assert!(sent.is_empty());
        encoder
            .push(&[b'b', b'c', 0, 0, 0, 1, b'd'], |frame| {
                sent.push(frame.to_vec());
                Ok(())
            })
            .unwrap();
        assert_eq!(sent, [b"abc".to_vec(), b"d".to_vec()]);
    }
}

//! Child-process PDFium boundary. The broker never owns native PDF objects.

use crate::{
    notes::SpeakerNotes,
    pdf::PdfDocumentState,
    render_scheduler::{RenderCommand, RenderEvent, RenderJobId, RenderPriority, RenderSessionId},
    renderer_limits::{Operation, RestartBudget, RESTART_BACKOFF},
    renderer_process::{Child, ReadPipe, WritePipe},
    renderer_protocol::{
        Broker, Envelope, FailureCode, Frame, Framed, Message, PageNote, PixelFormat,
        MAX_DOCUMENT_NOTE_BYTES, MAX_NOTE_BYTES, MAX_TEXT_BYTES,
    },
    renderer_resources::ResourceJob,
    renderer_supervision::{terminate, FailureKind, HelperFailure, Watchdog},
    rendering::{RenderRequest, RenderedPagePixels},
};
use anyhow::{bail, ensure, Context, Result};
use std::{
    cell::{Cell, RefCell},
    io,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex, Weak},
};

pub const HELPER_ARGUMENT: &str = "--renderer-helper";

#[derive(Default)]
pub struct ProcessGroup {
    state: Mutex<GroupState>,
}
#[derive(Default)]
struct GroupState {
    stopped: bool,
    children: Vec<Weak<Mutex<Child>>>,
    visible_request: Option<(RenderSessionId, RenderRequest)>,
}
impl ProcessGroup {
    pub(crate) fn set_visible(&self, session: RenderSessionId, request: RenderRequest) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .visible_request = Some((session, request));
    }
    pub(crate) fn visible_request(&self, session: RenderSessionId) -> Option<RenderRequest> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .visible_request
            .filter(|(id, _)| *id == session)
            .map(|(_, request)| request)
    }
    fn is_stopped(&self) -> bool {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).stopped
    }
    fn register(&self, child: &Arc<Mutex<Child>>) -> Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped {
            terminate(child);
            bail!("renderer shutdown already requested");
        }
        state.children.retain(|child| child.strong_count() > 0);
        state.children.push(Arc::downgrade(child));
        Ok(())
    }
    pub fn shutdown(&self) {
        let children = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.stopped = true;
            std::mem::take(&mut state.children)
        };
        for child in children {
            if let Some(child) = child.upgrade() {
                terminate(&child);
            }
        }
    }
}
pub struct HelperClient {
    child: Arc<Mutex<Child>>,
    writer: Framed<WritePipe>,
    reader: Framed<ReadPipe>,
    broker: Broker,
    failed: bool,
    failure_message: Option<String>,
    failure_kind: Option<FailureKind>,
    watchdog: Watchdog,
    _resource_job: ResourceJob,
}
impl HelperClient {
    pub fn spawn(group: &ProcessGroup) -> Result<Self> {
        Self::spawn_command(
            Command::new(std::env::current_exe().context("cannot locate renderer executable")?),
            group,
        )
    }
    pub fn spawn_for_document(group: &ProcessGroup, path: &std::path::Path) -> Result<Self> {
        Self::spawn_command_with_document(Command::new(std::env::current_exe()?), group, Some(path))
    }
    pub fn spawn_command(command: Command, group: &ProcessGroup) -> Result<Self> {
        Self::spawn_command_with_document(command, group, None)
    }
    fn spawn_command_with_document(
        mut command: Command,
        group: &ProcessGroup,
        path: Option<&std::path::Path>,
    ) -> Result<Self> {
        command
            .arg(HELPER_ARGUMENT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let (mut child, resource_job) =
            crate::renderer_process::spawn(command, path).map_err(|_| HelperFailure {
                kind: FailureKind::Spawn,
                operation: Operation::Handshake,
            })?;
        let stdin = child.stdin.take().context("missing helper stdin")?;
        let stdout = child.stdout.take().context("missing helper stdout")?;
        let child = Arc::new(Mutex::new(child));
        group.register(&child)?;
        let watchdog = Watchdog::new(Arc::clone(&child));
        let mut client = Self {
            child,
            writer: Framed::new(stdin),
            reader: Framed::new(stdout),
            broker: Broker::new(RenderSessionId(1))?,
            failed: false,
            failure_message: None,
            failure_kind: None,
            watchdog,
            _resource_job: resource_job,
        };
        let hello = client.broker.hello()?;
        client.exchange(hello, Operation::Handshake)?;
        Ok(client)
    }
    fn exchange(&mut self, request: Frame, operation: Operation) -> Result<Option<RenderEvent>> {
        ensure!(!self.failed, "renderer helper is no longer available");
        self.watchdog.arm(operation);
        let result = (|| {
            self.writer.send(&request)?;
            let response = self
                .broker
                .read_response(&mut self.reader)?
                .ok_or(HelperFailure {
                    kind: FailureKind::Crash,
                    operation,
                })?;
            self.broker.accept(response)
        })();
        let timed_failure = self.watchdog.finish();
        if result.is_err() || timed_failure.is_some() {
            let kind = timed_failure.unwrap_or_else(|| {
                result
                    .as_ref()
                    .err()
                    .and_then(|error| error.downcast_ref::<HelperFailure>())
                    .map(|error| error.kind)
                    .unwrap_or(FailureKind::Protocol)
            });
            self.failed = true;
            self.failure_kind = Some(kind);
            terminate(&self.child);
            let status = self
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait()
                .ok()
                .flatten();
            let failure = HelperFailure { kind, operation };
            self.failure_message = Some(format!("{failure} exit={status:?}"));
            tracing::warn!(operation = ?operation, cause = ?kind, exit = ?status, "renderer helper failure");
            return Err(failure.into());
        }
        result
    }
    pub fn open(&mut self, path: PathBuf) -> Result<(String, u32)> {
        crate::pdf::preflight_pdf_input(&path)?;
        let request = self.broker.command(
            RenderCommand::Open {
                session_id: RenderSessionId(1),
                path,
            },
            RenderJobId(0),
        )?;
        match self.exchange(request, Operation::Open)? {
            Some(RenderEvent::Opened {
                title, page_count, ..
            }) => Ok((title, page_count)),
            Some(RenderEvent::OpenFailed { message, .. }) => bail!("{message}"),
            _ => bail!("missing open response"),
        }
    }
    pub fn render(&mut self, request: RenderRequest) -> Result<RenderedPagePixels> {
        let priority = if request.purpose == crate::rendering::RenderPurpose::CurrentSlide {
            RenderPriority::BlockingVisible
        } else {
            RenderPriority::VisibleAux
        };
        self.render_with_priority(request, priority)
    }
    fn render_with_priority(
        &mut self,
        request: RenderRequest,
        priority: RenderPriority,
    ) -> Result<RenderedPagePixels> {
        let frame = self.broker.command(
            RenderCommand::RenderPage {
                session_id: RenderSessionId(1),
                request,
                priority,
            },
            RenderJobId(1),
        )?;
        let operation = if priority == RenderPriority::BlockingVisible {
            Operation::VisibleRender
        } else {
            Operation::AuxiliaryRender
        };
        match self.exchange(frame, operation)? {
            Some(RenderEvent::PageRendered { page, .. }) => Ok(page),
            Some(RenderEvent::PageFailed { message, .. }) => bail!("{message}"),
            _ => bail!("missing render response"),
        }
    }
    pub fn notes_page(&mut self, index: u32) -> Result<Vec<(u32, String)>> {
        let frame = self.broker.notes_page(index)?;
        match self.exchange(frame, Operation::Notes)? {
            Some(RenderEvent::SpeakerNotesLoaded {
                notes, status_text, ..
            }) if status_text == "Ready" => Ok(notes
                .note_for_page_index(index)
                .map(|n| vec![(index + 1, n.to_owned())])
                .unwrap_or_default()),
            Some(RenderEvent::SpeakerNotesLoaded { status_text, .. }) => bail!("{status_text}"),
            _ => bail!("missing notes response"),
        }
    }
    pub fn process_id(&self) -> u32 {
        self.child.lock().unwrap_or_else(|e| e.into_inner()).id()
    }
    fn poll_failure(&mut self) -> bool {
        if !self.failed {
            let status = self
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait();
            if !matches!(status, Ok(None)) || self.watchdog.finish().is_some() {
                self.failed = true;
                let kind = self.watchdog.finish().unwrap_or(FailureKind::Crash);
                self.failure_kind = Some(kind);
                self.failure_message = Some(format!(
                    "Renderer helper failed: cause={kind:?} exit={status:?}"
                ));
                terminate(&self.child);
            }
        }
        self.failed
    }
}
impl Drop for HelperClient {
    fn drop(&mut self) {
        let exited = self
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .try_wait()
            .ok()
            .flatten()
            .is_some();
        if !self.failed && !exited {
            if let Ok(frame) = self.broker.command(RenderCommand::Shutdown, RenderJobId(0)) {
                let _ = self.exchange(frame, Operation::Shutdown);
            }
        }
        terminate(&self.child);
    }
}

pub struct RemoteDocument {
    client: RefCell<HelperClient>,
    title: String,
    page_count: u32,
    initial: RefCell<Option<(RenderRequest, RenderedPagePixels)>>,
    path: PathBuf,
    group: Arc<ProcessGroup>,
    active: Cell<bool>,
    session_id: Cell<Option<RenderSessionId>>,
    restart: RefCell<RestartBudget>,
    current_request: Cell<RenderRequest>,
    file_identity: (u64, Option<std::time::SystemTime>),
    restart_attempts: Cell<u32>,
}
impl RemoteDocument {
    pub fn open(path: PathBuf, group: &Arc<ProcessGroup>) -> Result<Self> {
        let metadata = std::fs::metadata(&path)?;
        let mut client = HelperClient::spawn_for_document(group, &path)?;
        let (title, page_count) = client.open(path.clone())?;
        Ok(Self {
            client: RefCell::new(client),
            title,
            page_count,
            initial: RefCell::new(None),
            path,
            group: Arc::clone(group),
            active: Cell::new(false),
            session_id: Cell::new(None),
            restart: RefCell::new(RestartBudget::default()),
            current_request: Cell::new(RenderRequest {
                page_index: 0,
                width: crate::render_controller::CURRENT_RENDER_WIDTH,
                purpose: crate::rendering::RenderPurpose::CurrentSlide,
            }),
            file_identity: (metadata.len(), metadata.modified().ok()),
            restart_attempts: Cell::new(0),
        })
    }
    pub fn title(&self) -> String {
        self.title.clone()
    }
    pub fn page_count(&self) -> u32 {
        self.page_count
    }
    pub fn prepare_initial(&self) -> Result<()> {
        let request = RenderRequest {
            page_index: 0,
            width: crate::render_controller::CURRENT_RENDER_WIDTH,
            purpose: crate::rendering::RenderPurpose::CurrentSlide,
        };
        let pixels = self.client.borrow_mut().render(request)?;
        *self.initial.borrow_mut() = Some((request, pixels));
        Ok(())
    }
    pub fn render(&self, request: RenderRequest) -> Result<RenderedPagePixels> {
        let priority = if request.purpose == crate::rendering::RenderPurpose::CurrentSlide {
            RenderPriority::BlockingVisible
        } else {
            RenderPriority::VisibleAux
        };
        self.render_with_priority(request, priority)
    }
    pub fn render_with_priority(
        &self,
        request: RenderRequest,
        priority: RenderPriority,
    ) -> Result<RenderedPagePixels> {
        if priority == RenderPriority::BlockingVisible {
            self.current_request.set(request);
        }
        if let Some((cached, page)) = self.initial.borrow().as_ref() {
            if *cached == request {
                return Ok(page.clone());
            }
        }
        let result = self
            .client
            .borrow_mut()
            .render_with_priority(request, priority);
        if result.is_err() && self.recover() {
            if let Some((cached, page)) = self.initial.borrow().as_ref() {
                if *cached == request {
                    return Ok(page.clone());
                }
            }
            return self
                .client
                .borrow_mut()
                .render_with_priority(request, priority);
        }
        result
    }
    pub fn notes_page(&self, index: u32) -> Result<Vec<(u32, String)>> {
        let result = self.client.borrow_mut().notes_page(index);
        if result.is_err() && self.recover() {
            return self.client.borrow_mut().notes_page(index);
        }
        result
    }
    pub fn has_failed(&self) -> bool {
        let failed = self.client.borrow_mut().poll_failure();
        failed && !self.recover()
    }
    pub fn activate(&self, session_id: RenderSessionId) {
        self.active.set(true);
        self.session_id.set(Some(session_id));
    }
    fn recover(&self) -> bool {
        let kind = self.client.borrow().failure_kind;
        if !self.active.get()
            || !matches!(kind, Some(FailureKind::Crash | FailureKind::Timeout))
            || self.group.is_stopped()
        {
            return false;
        }
        if !self.restart.borrow_mut().take(std::time::Instant::now()) {
            return false;
        }
        self.restart_attempts
            .set(self.restart_attempts.get().saturating_add(1));
        tracing::warn!(cause = ?kind, "renderer helper restart attempt");
        let until = std::time::Instant::now() + RESTART_BACKOFF;
        while std::time::Instant::now() < until {
            if self.group.is_stopped() {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let result = (|| -> Result<_> {
            let metadata = std::fs::metadata(&self.path)?;
            ensure!(
                (metadata.len(), metadata.modified().ok()) == self.file_identity,
                "document changed during renderer recovery"
            );
            let mut replacement = HelperClient::spawn_for_document(&self.group, &self.path)?;
            let (title, count) = replacement.open(self.path.clone())?;
            ensure!(
                title == self.title && count == self.page_count,
                "document changed during renderer recovery"
            );
            let request = self
                .session_id
                .get()
                .and_then(|id| self.group.visible_request(id))
                .unwrap_or_else(|| self.current_request.get());
            let page = replacement.render(request)?;
            Ok((replacement, request, page))
        })();
        match result {
            Ok((replacement, request, page)) => {
                *self.client.borrow_mut() = replacement;
                *self.initial.borrow_mut() = Some((request, page));
                tracing::info!("renderer helper recovered");
                true
            }
            Err(_) => {
                self.restart.borrow_mut().suppress();
                tracing::warn!("renderer helper restart suppressed; explicit reopen required");
                false
            }
        }
    }
    pub fn failure_message(&self) -> Option<String> {
        self.client.borrow().failure_message.clone()
    }
}

/// Run before diagnostics or GUI initialization. stdout is exclusively protocol bytes.
pub fn run() -> Result<()> {
    #[cfg(target_os = "windows")]
    let mut brokered_input = None;
    #[cfg(target_os = "windows")]
    {
        #[cfg(debug_assertions)]
        let raw_test = std::env::var_os("QUICK_PRESENTER_HELPER_TEST_RAW_PROTOCOL").is_some();
        #[cfg(not(debug_assertions))]
        let raw_test = false;
        if !raw_test {
            crate::renderer_process::verify_token()?;
            brokered_input = Some(crate::renderer_process::take_document_file()?);
            crate::renderer_process::verify_denials()?;
        }
    }
    crate::renderer_resources::constrain_helper()?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(64);
    std::thread::spawn(move || {
        let mut input = Framed::new(io::stdin().lock());
        loop {
            match input.receive() {
                Ok(Some(frame)) => {
                    if sender.try_send(frame).is_err() {
                        std::process::exit(1);
                    }
                }
                // This guardian can terminate a helper even while PDFium is blocked.
                Ok(None) => std::process::exit(0),
                Err(_) => std::process::exit(1),
            }
        }
    });
    let mut output = Framed::new(io::stdout().lock());
    let mut document: Option<PdfDocumentState> = None;
    let mut session = None;
    let mut ready = false;
    let mut last_request = 0;
    for frame in receiver {
        let Envelope {
            request_id,
            session_id,
            message,
        } = frame.envelope;
        ensure!(request_id > last_request, "request ID is not monotonic");
        last_request = request_id;
        ensure!(
            ready || matches!(message, Message::Hello {}),
            "missing helper handshake"
        );
        #[cfg(debug_assertions)]
        if matches!(
            (
                &message,
                request_fault(&message, document.as_ref()).as_deref()
            ),
            (Message::Hello {}, Some("hang-on-handshake"))
                | (Message::Open { .. }, Some("hang-on-open"))
                | (Message::Shutdown {}, Some("hang-on-shutdown"))
                | (
                    Message::NotesPage { .. } | Message::Notes {},
                    Some("hang-on-notes")
                )
        ) {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        let mut pixels = Vec::new();
        let response = match message {
            Message::Hello {} => {
                ensure!(!ready, "duplicate helper handshake");
                ready = true;
                Message::Ready {}
            }
            Message::Shutdown {} => {
                output.send(&Frame {
                    envelope: Envelope {
                        request_id,
                        session_id,
                        message: Message::Stopped {},
                    },
                    pixels,
                })?;
                return Ok(());
            }
            Message::Open { path } => {
                ensure!(document.is_none(), "helper document already open");
                #[cfg(target_os = "windows")]
                let opened = if let Some(file) = brokered_input.take() {
                    PdfDocumentState::open_brokered(path.into_path()?, file)
                } else {
                    #[cfg(debug_assertions)]
                    {
                        ensure!(
                            std::env::var_os("QUICK_PRESENTER_HELPER_TEST_RAW_PROTOCOL").is_some(),
                            "missing brokered PDF handle"
                        );
                        PdfDocumentState::open(path.into_path()?)
                    }
                    #[cfg(not(debug_assertions))]
                    {
                        bail!("missing brokered PDF handle");
                    }
                };
                #[cfg(not(target_os = "windows"))]
                let opened = PdfDocumentState::open(path.into_path()?);
                match opened {
                    Ok(doc) if doc.page_count() > 0 => {
                        let response = Message::Opened {
                            title: doc.title(),
                            page_count: doc.page_count(),
                        };
                        document = Some(doc);
                        session = Some(session_id);
                        response
                    }
                    Ok(_) => failure(FailureCode::InvalidPdf, "PDF contains no pages"),
                    Err(error) => failure(FailureCode::InvalidPdf, &format!("{error:#}")),
                }
            }
            Message::Render { page_index, width } => {
                #[cfg(debug_assertions)]
                match render_fault(page_index, document.as_ref()).as_deref() {
                    Some("abort-on-render") => std::process::abort(),
                    Some("eof-on-render") => std::process::exit(0),
                    Some("mismatch-on-render") => {
                        use std::io::Write;
                        io::stdout().write_all(b"QPRP\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00x")?;
                        return Ok(());
                    }
                    Some("wait-on-render") => loop {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    },
                    Some("truncated-on-render") => {
                        use std::io::Write;
                        io::stdout().write_all(b"QPRP\x02\x00\x01")?;
                        return Ok(());
                    }
                    Some("oversized-on-render") => {
                        use std::io::Write;
                        let mut bytes = b"QPRP".to_vec();
                        bytes.extend(crate::renderer_protocol::VERSION.to_le_bytes());
                        bytes.extend(
                            (crate::renderer_protocol::MAX_CONTROL_BYTES as u32 + 1).to_le_bytes(),
                        );
                        bytes.extend(0u32.to_le_bytes());
                        io::stdout().write_all(&bytes)?;
                        return Ok(());
                    }
                    Some("malformed-on-render") => {
                        use std::io::Write;
                        let control = br#"{"request_id":1,"session_id":1,"message":{"kind":"SECRET_PDF_NOTE"}}"#;
                        let mut bytes = b"QPRP".to_vec();
                        bytes.extend(crate::renderer_protocol::VERSION.to_le_bytes());
                        bytes.extend((control.len() as u32).to_le_bytes());
                        bytes.extend(0u32.to_le_bytes());
                        bytes.extend(control);
                        io::stdout().write_all(&bytes)?;
                        return Ok(());
                    }
                    Some("allocate-on-render") => {
                        let allocation = crate::renderer_limits::MAX_HELPER_MEMORY_BYTES
                            .checked_mul(2)
                            .context("test allocation size overflow")?;
                        // macOS has sampled RSS rather than a reservation cap. Do not
                        // actually touch multi-GB allocations in this test hook.
                        #[cfg(any(target_os = "linux", target_os = "windows"))]
                        {
                            let mut bytes = Vec::<u8>::new();
                            ensure!(
                                bytes.try_reserve_exact(allocation).is_err(),
                                "helper memory reservation unexpectedly succeeded"
                            );
                        }
                        let _ = allocation;
                        output.send(&Frame {
                            envelope: Envelope {
                                request_id,
                                session_id,
                                message: failure(
                                    FailureCode::LimitExceeded,
                                    "helper allocation limit enforced",
                                ),
                            },
                            pixels: Vec::new(),
                        })?;
                        continue;
                    }
                    _ => {}
                }
                ensure!(session == Some(session_id), "wrong document session");
                let doc = document.as_ref().context("document not open")?;
                match doc.render_page_pixels(page_index, width as i32) {
                    Ok(buffer) => {
                        pixels = buffer.as_bytes().to_vec();
                        Message::Pixels {
                            width: buffer.width(),
                            height: buffer.height(),
                            format: PixelFormat::Rgba8,
                        }
                    }
                    Err(error) => failure(FailureCode::RenderFailed, &error.to_string()),
                }
            }
            Message::NotesPage { page_index } => {
                ensure!(session == Some(session_id), "wrong document session");
                let doc = document.as_ref().context("document not open")?;
                let result = doc
                    .speaker_notes_for_page_cancellable(page_index, &|| false)
                    .and_then(|notes| bounded_notes(notes.unwrap_or_default()));
                match result {
                    Ok(pages) => Message::NotesLoaded { pages },
                    Err(error) => failure(FailureCode::NotesFailed, &error.to_string()),
                }
            }
            Message::Notes {} => {
                ensure!(session == Some(session_id), "wrong document session");
                let doc = document.as_ref().context("document not open")?;
                let result = (|| {
                    let notes = doc.speaker_notes()?;
                    bounded_notes(
                        (0..doc.page_count())
                            .filter_map(|i| {
                                notes.note_for_page_index(i).map(|n| (i + 1, n.to_owned()))
                            })
                            .collect(),
                    )
                })();
                match result {
                    Ok(pages) => Message::NotesLoaded { pages },
                    Err(error) => failure(FailureCode::NotesFailed, &error.to_string()),
                }
            }
            Message::Close {} => {
                ensure!(session == Some(session_id), "wrong document session");
                document = None;
                session = None;
                Message::Closed {}
            }
            _ => bail!("unexpected helper request"),
        };
        let response = Frame {
            envelope: Envelope {
                request_id,
                session_id,
                message: response,
            },
            pixels,
        };
        output.send(&response)?;
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn test_fault() -> Option<String> {
    std::env::var("QUICK_PRESENTER_HELPER_TEST_FAULT").ok()
}
#[cfg(debug_assertions)]
fn request_fault(message: &Message, document: Option<&PdfDocumentState>) -> Option<String> {
    if let Ok(title) = std::env::var("QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE") {
        let requested_title = match message {
            Message::Open { path } => path.clone().into_path().ok().and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            }),
            _ => document.map(PdfDocumentState::title),
        };
        if requested_title.as_deref() != Some(title.as_str()) {
            return None;
        }
    }
    test_fault()
}
#[cfg(debug_assertions)]
fn render_fault(page_index: u32, document: Option<&PdfDocumentState>) -> Option<String> {
    if std::env::var("QUICK_PRESENTER_HELPER_TEST_FAULT_PAGE")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .is_some_and(|page| page != page_index)
    {
        return None;
    }
    if std::env::var("QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE")
        .ok()
        .is_some_and(|title| document.is_none_or(|doc| doc.title() != title))
    {
        return None;
    }
    let fault = test_fault()?;
    if let Ok(marker) = std::env::var("QUICK_PRESENTER_HELPER_TEST_FAULT_ONCE") {
        // Only debug test-owned marker files are consumed; release ignores all hooks.
        if std::fs::remove_file(marker).is_err() {
            return None;
        }
    }
    Some(fault)
}

/// Internal executable-level test entry point: exercise real broker supervision
/// without requiring a GUI display or opening PDFium in the test parent.
#[cfg(debug_assertions)]
pub fn recovery_smoke(fault_path: PathBuf, valid_path: PathBuf) -> Result<()> {
    let group = Arc::new(ProcessGroup::default());
    let document = RemoteDocument::open(fault_path, &group)?;
    document.prepare_initial()?;
    document.activate(RenderSessionId(1));
    let last_good = document.initial.borrow().as_ref().unwrap().1.pixels.clone();
    let request = RenderRequest {
        page_index: 1,
        width: crate::render_controller::CURRENT_RENDER_WIDTH,
        purpose: crate::rendering::RenderPurpose::CurrentSlide,
    };
    let result = document.render(request);
    println!(
        "recovery_result={} restart_attempts={} last_good_bytes={}",
        result.is_ok(),
        document.restart_attempts.get(),
        last_good.as_bytes().len()
    );
    if result.is_err() {
        ensure!(document.has_failed(), "failed helper not terminal");
    }
    let valid = RemoteDocument::open(valid_path, &group)?;
    valid.prepare_initial()?;
    valid.activate(RenderSessionId(2));
    valid.render(request)?;
    group.shutdown();
    println!("later_valid_pdf=true");
    Ok(())
}

#[cfg(debug_assertions)]
pub fn scheduler_smoke(fault_path: PathBuf, valid_path: PathBuf) -> Result<()> {
    use crate::render_scheduler::{RenderScheduler, RenderWorkerLifecycle};
    use std::time::{Duration, Instant};
    let scheduler = RenderScheduler::start();
    let wait_event = || -> Result<RenderEvent> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(event) = scheduler.drain_events().into_iter().next() {
                return Ok(event);
            }
            ensure!(
                Instant::now() < deadline,
                "scheduler test response deadline exceeded"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    scheduler.open(RenderSessionId(1), valid_path);
    ensure!(
        matches!(wait_event()?, RenderEvent::Opened { .. }),
        "valid deck did not open"
    );
    let shutdown_test = std::env::var_os("QUICK_PRESENTER_HELPER_TEST_SHUTDOWN").is_some();
    if !shutdown_test {
        scheduler.open(RenderSessionId(2), fault_path);
        ensure!(
            matches!(wait_event()?, RenderEvent::OpenFailed { .. }),
            "candidate unexpectedly committed"
        );
    }
    scheduler.render_page(
        RenderSessionId(1),
        RenderRequest {
            page_index: 1,
            width: crate::render_controller::CURRENT_RENDER_WIDTH,
            purpose: crate::rendering::RenderPurpose::CurrentSlide,
        },
        RenderPriority::BlockingVisible,
    );
    if shutdown_test {
        std::thread::sleep(Duration::from_millis(200));
    } else {
        ensure!(
            matches!(
                wait_event()?,
                RenderEvent::PageRendered {
                    session_id: RenderSessionId(1),
                    ..
                }
            ),
            "failed candidate replaced active deck"
        );
    }
    let start = Instant::now();
    scheduler.request_shutdown();
    while !scheduler.can_be_replaced() {
        ensure!(
            start.elapsed() < Duration::from_secs(2),
            "shutdown remained blocked"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    ensure!(
        scheduler.lifecycle() == RenderWorkerLifecycle::Stopped
            && scheduler.try_join_finished_worker(),
        "scheduler did not stop cleanly"
    );
    println!("active_preserved={} shutdown_reaped=true", !shutdown_test);
    Ok(())
}
fn failure(code: FailureCode, message: &str) -> Message {
    let mut end = message.len().min(MAX_TEXT_BYTES);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    Message::Failed {
        code,
        message: message[..end].to_owned(),
    }
}
fn bounded_notes(notes: Vec<(u32, String)>) -> Result<Vec<PageNote>> {
    let merged = SpeakerNotes::from_page_notes(notes.iter().cloned());
    let mut pages = std::collections::BTreeSet::new();
    let mut total = 0usize;
    for (page, _) in &notes {
        pages.insert(*page);
    }
    pages
        .into_iter()
        .filter_map(|page| merged.note_for_page_number(page).map(|text| (page, text)))
        .map(|(page_number, text)| {
            ensure!(text.len() <= MAX_NOTE_BYTES, "speaker note exceeds limit");
            total = total
                .checked_add(text.len())
                .context("notes length overflow")?;
            ensure!(
                total <= MAX_DOCUMENT_NOTE_BYTES,
                "document notes exceed limit"
            );
            Ok(PageNote {
                page_number,
                text: text.to_owned(),
            })
        })
        .collect()
}

//! Child-process PDFium boundary. The broker never owns native PDF objects.

use crate::{
    notes::SpeakerNotes,
    pdf::PdfDocumentState,
    render_scheduler::{RenderCommand, RenderEvent, RenderJobId, RenderPriority, RenderSessionId},
    renderer_protocol::{
        Broker, Envelope, FailureCode, Frame, Framed, Message, PageNote, PixelFormat,
        MAX_DOCUMENT_NOTE_BYTES, MAX_NOTE_BYTES, MAX_TEXT_BYTES,
    },
    rendering::{RenderRequest, RenderedPagePixels},
};
use anyhow::{bail, ensure, Context, Result};
use std::{
    cell::RefCell,
    io,
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
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
}
impl ProcessGroup {
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
fn terminate(child: &Mutex<Child>) {
    let mut child = child.lock().unwrap_or_else(|e| e.into_inner());
    // Reap even an already-exited child. No protocol round trip can block cleanup.
    let _ = child.kill();
    if let Err(error) = child.wait() {
        tracing::warn!(%error, "failed to reap renderer helper");
    }
}

pub struct HelperClient {
    child: Arc<Mutex<Child>>,
    writer: Framed<ChildStdin>,
    reader: Framed<ChildStdout>,
    broker: Broker,
    failed: bool,
    failure_message: Option<String>,
}
impl HelperClient {
    pub fn spawn(group: &ProcessGroup) -> Result<Self> {
        Self::spawn_command(
            Command::new(std::env::current_exe().context("cannot locate renderer executable")?),
            group,
        )
    }
    pub fn spawn_command(mut command: Command, group: &ProcessGroup) -> Result<Self> {
        command
            .arg(HELPER_ARGUMENT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().context("failed to spawn renderer helper")?;
        let stdin = child.stdin.take().context("missing helper stdin")?;
        let stdout = child.stdout.take().context("missing helper stdout")?;
        let child = Arc::new(Mutex::new(child));
        group.register(&child)?;
        let mut client = Self {
            child,
            writer: Framed::new(stdin),
            reader: Framed::new(stdout),
            broker: Broker::new(RenderSessionId(1))?,
            failed: false,
            failure_message: None,
        };
        let hello = client.broker.hello()?;
        client.exchange(hello)?;
        Ok(client)
    }
    fn exchange(&mut self, request: Frame) -> Result<Option<RenderEvent>> {
        let result = (|| {
            self.writer.send(&request)?;
            let response = self
                .broker
                .read_response(&mut self.reader)?
                .context("renderer helper closed its output pipe")?;
            self.broker.accept(response)
        })();
        if let Err(error) = &result {
            self.failed = true;
            terminate(&self.child);
            let status = self
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait()
                .ok()
                .flatten();
            self.failure_message = Some(format!("Renderer helper failed ({status:?}): {error:#}"));
        }
        result.with_context(|| {
            self.failure_message
                .clone()
                .unwrap_or_else(|| "renderer helper transport failed".into())
        })
    }
    pub fn open(&mut self, path: PathBuf) -> Result<(String, u32)> {
        let request = self.broker.command(
            RenderCommand::Open {
                session_id: RenderSessionId(1),
                path,
            },
            RenderJobId(0),
        )?;
        match self.exchange(request)? {
            Some(RenderEvent::Opened {
                title, page_count, ..
            }) => Ok((title, page_count)),
            Some(RenderEvent::OpenFailed { message, .. }) => bail!("{message}"),
            _ => bail!("missing open response"),
        }
    }
    pub fn render(&mut self, request: RenderRequest) -> Result<RenderedPagePixels> {
        let frame = self.broker.command(
            RenderCommand::RenderPage {
                session_id: RenderSessionId(1),
                request,
                priority: RenderPriority::BlockingVisible,
            },
            RenderJobId(1),
        )?;
        match self.exchange(frame)? {
            Some(RenderEvent::PageRendered { page, .. }) => Ok(page),
            Some(RenderEvent::PageFailed { message, .. }) => bail!("{message}"),
            _ => bail!("missing render response"),
        }
    }
    pub fn notes_page(&mut self, index: u32) -> Result<Vec<(u32, String)>> {
        let frame = self.broker.notes_page(index)?;
        match self.exchange(frame)? {
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
}
impl Drop for HelperClient {
    fn drop(&mut self) {
        terminate(&self.child);
    }
}

pub struct RemoteDocument {
    client: RefCell<HelperClient>,
    title: String,
    page_count: u32,
    initial: RefCell<Option<(RenderRequest, RenderedPagePixels)>>,
}
impl RemoteDocument {
    pub fn open(path: PathBuf, group: &ProcessGroup) -> Result<Self> {
        let mut client = HelperClient::spawn(group)?;
        let (title, page_count) = client.open(path)?;
        Ok(Self {
            client: RefCell::new(client),
            title,
            page_count,
            initial: RefCell::new(None),
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
        if let Some((cached, page)) = self.initial.borrow().as_ref() {
            if *cached == request {
                return Ok(page.clone());
            }
        }
        self.client.borrow_mut().render(request)
    }
    pub fn notes_page(&self, index: u32) -> Result<Vec<(u32, String)>> {
        self.client.borrow_mut().notes_page(index)
    }
    pub fn has_failed(&self) -> bool {
        let mut client = self.client.borrow_mut();
        if !client.failed {
            let result = client
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait();
            match result {
                Ok(None) => {}
                status => {
                    client.failed = true;
                    client.failure_message =
                        Some(format!("Renderer helper exited unexpectedly: {status:?}"));
                    terminate(&client.child);
                }
            }
        }
        client.failed
    }
    pub fn failure_message(&self) -> Option<String> {
        self.client.borrow().failure_message.clone()
    }
}

/// Run before diagnostics or GUI initialization. stdout is exclusively protocol bytes.
pub fn run() -> Result<()> {
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
                match PdfDocumentState::open(path.into_path()?) {
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
                match std::env::var("QUICK_PRESENTER_HELPER_TEST_FAULT")
                    .ok()
                    .as_deref()
                {
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

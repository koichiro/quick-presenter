//! Versioned renderer IPC. This module never opens PDFium or mutates app state.

use crate::{
    notes::SpeakerNotes,
    render_scheduler::{RenderCommand, RenderEvent, RenderJobId, RenderSessionId},
    rendering::{RenderRequest, RenderedPagePixels},
};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    path::PathBuf,
};

pub const VERSION: u16 = 2;
pub const MAX_CONTROL_BYTES: usize = 1024 * 1024;
pub const MAX_PIXEL_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_DIMENSION: u32 = 16_384;
pub const MAX_PAGES: u32 = 100_000;
pub const MAX_PATH_BYTES: usize = 32_768;
pub const MAX_TEXT_BYTES: usize = 4096;
pub const MAX_NOTE_BYTES: usize = 64 * 1024;
pub const MAX_DOCUMENT_NOTE_BYTES: usize = 512 * 1024;
pub const MAX_PENDING_REQUESTS: usize = 64;
const MAGIC: [u8; 4] = *b"QPRP";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub request_id: u64,
    pub session_id: u64,
    pub message: Message,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Message {
    Hello {},
    Ready {},
    Open {
        path: NativePath,
    },
    Render {
        page_index: u32,
        width: u32,
    },
    Notes {},
    NotesPage {
        page_index: u32,
    },
    Close {},
    Shutdown {},
    Opened {
        title: String,
        page_count: u32,
    },
    Pixels {
        width: u32,
        height: u32,
        format: PixelFormat,
    },
    NotesLoaded {
        pages: Vec<PageNote>,
    },
    Closed {},
    Stopped {},
    Failed {
        code: FailureCode,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FailureCode {
    InvalidPdf,
    UnsupportedPdf,
    LimitExceeded,
    RenderFailed,
    NotesFailed,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PixelFormat {
    Rgba8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageNote {
    pub page_number: u32,
    pub text: String,
}

/// Lossless OS paths: Unix bytes or Windows little-endian UTF-16 bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "encoding", content = "bytes", deny_unknown_fields)]
pub enum NativePath {
    Unix(Vec<u8>),
    Windows(Vec<u8>),
}

impl NativePath {
    pub fn from_path(path: PathBuf) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            Self::Unix(path.as_os_str().as_bytes().to_vec())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Self::Windows(
                path.as_os_str()
                    .encode_wide()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            )
        }
    }
    pub fn into_path(self) -> Result<PathBuf> {
        self.validate()?;
        match self {
            #[cfg(unix)]
            Self::Unix(bytes) => {
                use std::os::unix::ffi::OsStringExt;
                Ok(std::ffi::OsString::from_vec(bytes).into())
            }
            #[cfg(windows)]
            Self::Windows(bytes) => {
                use std::os::windows::ffi::OsStringExt;
                let wide: Vec<_> = bytes
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect();
                Ok(std::ffi::OsString::from_wide(&wide).into())
            }
            _ => bail!("path encoding does not match this platform"),
        }
    }
    fn validate(&self) -> Result<()> {
        let bytes = match self {
            Self::Unix(bytes) | Self::Windows(bytes) => bytes,
        };
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_PATH_BYTES,
            "invalid path length"
        );
        match self {
            Self::Unix(_) => ensure!(!bytes.contains(&0), "path contains NUL"),
            Self::Windows(_) => {
                ensure!(bytes.len() % 2 == 0, "invalid UTF-16 path length");
                ensure!(
                    !bytes.chunks_exact(2).any(|b| b == [0, 0]),
                    "path contains NUL"
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub envelope: Envelope,
    pub pixels: Vec<u8>,
}

fn pixel_len(width: u32, height: u32) -> Result<usize> {
    ensure!(
        (1..=MAX_DIMENSION).contains(&width) && (1..=MAX_DIMENSION).contains(&height),
        "invalid pixel dimensions"
    );
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|n| n.checked_mul(4))
        .context("pixel size overflow")?;
    ensure!(
        bytes <= MAX_PIXEL_BYTES as u64,
        "pixel payload exceeds limit"
    );
    usize::try_from(bytes).context("pixel size exceeds address space")
}

impl Envelope {
    fn validate(&self) -> Result<usize> {
        ensure!(self.request_id != 0, "zero request ID");
        let global = matches!(
            self.message,
            Message::Hello {} | Message::Ready {} | Message::Shutdown {} | Message::Stopped {}
        );
        ensure!(global == (self.session_id == 0), "invalid session ID");
        match &self.message {
            Message::Open { path } => path.validate()?,
            Message::NotesPage { page_index } => {
                ensure!(*page_index < MAX_PAGES, "page index exceeds limit")
            }
            Message::Render { page_index, width } => {
                ensure!(*page_index < MAX_PAGES, "page index exceeds limit");
                ensure!((1..=MAX_DIMENSION).contains(width), "invalid render width");
            }
            Message::Opened { title, page_count } => {
                ensure!(title.len() <= MAX_TEXT_BYTES, "title exceeds limit");
                ensure!((1..=MAX_PAGES).contains(page_count), "invalid page count");
            }
            Message::Pixels { width, height, .. } => return pixel_len(*width, *height),
            Message::NotesLoaded { pages } => {
                ensure!(pages.len() <= MAX_PAGES as usize, "too many notes");
                let mut total = 0usize;
                let mut previous = 0;
                for note in pages {
                    ensure!(
                        note.page_number > previous && note.page_number <= MAX_PAGES,
                        "invalid or duplicate note page"
                    );
                    previous = note.page_number;
                    ensure!(note.text.len() <= MAX_NOTE_BYTES, "note exceeds limit");
                    total = total
                        .checked_add(note.text.len())
                        .context("note size overflow")?;
                    ensure!(
                        total <= MAX_DOCUMENT_NOTE_BYTES,
                        "document notes exceed limit"
                    );
                }
            }
            Message::Failed { message, .. } => {
                ensure!(message.len() <= MAX_TEXT_BYTES, "error exceeds limit")
            }
            _ => {}
        }
        Ok(0)
    }
}

/// Read/Write can be anonymous pipes, in-memory cursors, or fault-injection streams.
pub struct Framed<T> {
    stream: T,
}
impl<T> Framed<T> {
    pub fn new(stream: T) -> Self {
        Self { stream }
    }
    pub fn into_inner(self) -> T {
        self.stream
    }
}
impl<T: Write> Framed<T> {
    pub fn send(&mut self, frame: &Frame) -> Result<()> {
        let expected = frame.envelope.validate()?;
        ensure!(
            frame.pixels.len() == expected,
            "pixel payload length mismatch"
        );
        let mut control = ControlBuffer(Vec::new());
        serde_json::to_writer(&mut control, &frame.envelope)?;
        let control = control.0;
        self.stream.write_all(&MAGIC)?;
        self.stream.write_all(&VERSION.to_le_bytes())?;
        self.stream
            .write_all(&(control.len() as u32).to_le_bytes())?;
        self.stream.write_all(&(expected as u32).to_le_bytes())?;
        self.stream.write_all(&control)?;
        self.stream.write_all(&frame.pixels)?;
        self.stream.flush()?;
        Ok(())
    }
}
impl<T: Read> Framed<T> {
    pub fn receive(&mut self) -> Result<Option<Frame>> {
        self.receive_checked(|_| Ok(()))
    }

    /// Correlate metadata before allocating or reading a pixel payload.
    pub fn receive_checked(
        &mut self,
        validate: impl FnOnce(&Envelope) -> Result<()>,
    ) -> Result<Option<Frame>> {
        let mut header = [0u8; 14];
        loop {
            match self.stream.read(&mut header[..1]) {
                Ok(0) => return Ok(None),
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        self.stream
            .read_exact(&mut header[1..])
            .context("truncated frame header")?;
        ensure!(header[..4] == MAGIC, "invalid protocol magic");
        ensure!(
            u16::from_le_bytes([header[4], header[5]]) == VERSION,
            "protocol version mismatch"
        );
        let control_len = u32::from_le_bytes(header[6..10].try_into()?) as usize;
        let payload_len = u32::from_le_bytes(header[10..14].try_into()?) as usize;
        ensure!(
            control_len > 0 && control_len <= MAX_CONTROL_BYTES,
            "invalid control frame length"
        );
        ensure!(payload_len <= MAX_PIXEL_BYTES, "pixel frame exceeds limit");
        let mut control = vec![0; control_len];
        self.stream
            .read_exact(&mut control)
            .context("truncated control frame")?;
        let envelope: Envelope =
            serde_json::from_slice(&control).context("invalid control message")?;
        ensure!(
            envelope.validate()? == payload_len,
            "pixel payload length mismatch"
        );
        validate(&envelope)?;
        let mut pixels = vec![0; payload_len];
        self.stream
            .read_exact(&mut pixels)
            .context("truncated pixel payload")?;
        Ok(Some(Frame { envelope, pixels }))
    }
}

struct ControlBuffer(Vec<u8>);
impl Write for ControlBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_CONTROL_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "control frame exceeds limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Broker adapter tracks outstanding requests without owning presentation state.
/// One instance belongs to one helper/session. Candidate helpers use another instance.
pub struct Broker {
    session: RenderSessionId,
    next_id: u64,
    ready: bool,
    page_count: Option<u32>,
    pending: HashMap<u64, Pending>,
}
#[derive(Clone)]
enum Pending {
    Hello,
    Open,
    Render(RenderRequest, RenderJobId),
    Notes,
    NotesPage(u32),
    Close,
    Shutdown,
}

impl Broker {
    pub fn new(session: RenderSessionId) -> Result<Self> {
        ensure!(session.0 != 0, "zero document session");
        Ok(Self {
            session,
            next_id: 0,
            ready: false,
            page_count: None,
            pending: HashMap::new(),
        })
    }
    fn request(&mut self, pending: Pending, message: Message) -> Result<Frame> {
        ensure!(
            self.pending.len() < MAX_PENDING_REQUESTS || matches!(pending, Pending::Shutdown),
            "too many pending requests"
        );
        let id = self
            .next_id
            .checked_add(1)
            .context("request ID exhausted")?;
        let session_id = if matches!(pending, Pending::Hello | Pending::Shutdown) {
            0
        } else {
            self.session.0
        };
        let envelope = Envelope {
            request_id: id,
            session_id,
            message,
        };
        envelope.validate()?;
        self.pending.insert(id, pending);
        self.next_id = id;
        Ok(Frame {
            envelope,
            pixels: Vec::new(),
        })
    }
    pub fn hello(&mut self) -> Result<Frame> {
        ensure!(
            !self.ready && self.pending.is_empty(),
            "handshake already started"
        );
        self.request(Pending::Hello, Message::Hello {})
    }
    pub fn command(&mut self, command: RenderCommand, job_id: RenderJobId) -> Result<Frame> {
        ensure!(self.ready, "helper handshake incomplete");
        ensure!(
            !self
                .pending
                .values()
                .any(|p| matches!(p, Pending::Close | Pending::Shutdown)),
            "helper lifecycle operation pending"
        );
        match command {
            RenderCommand::Open { session_id, path } => {
                self.check_session(session_id)?;
                ensure!(
                    self.pending.is_empty() && self.page_count.is_none(),
                    "helper already has document work"
                );
                self.request(
                    Pending::Open,
                    Message::Open {
                        path: NativePath::from_path(path),
                    },
                )
            }
            RenderCommand::RenderPage {
                session_id,
                request,
                ..
            } => {
                self.check_session(session_id)?;
                ensure!(
                    request.page_index < self.page_count.context("document not open")?,
                    "page outside document"
                );
                ensure!(job_id.0 != 0, "zero job ID");
                let width = u32::try_from(request.width).context("negative render width")?;
                self.request(
                    Pending::Render(request, job_id),
                    Message::Render {
                        page_index: request.page_index,
                        width,
                    },
                )
            }
            RenderCommand::ExtractSpeakerNotes { session_id } => {
                self.check_session(session_id)?;
                ensure!(self.page_count.is_some(), "document not open");
                self.request(Pending::Notes, Message::Notes {})
            }
            RenderCommand::Shutdown => self.request(Pending::Shutdown, Message::Shutdown {}),
            _ => bail!("reload transactions are broker operations across candidate helpers"),
        }
    }
    pub fn close(&mut self) -> Result<Frame> {
        ensure!(
            self.ready && self.pending.is_empty() && self.page_count.is_some(),
            "cannot close helper with pending or missing document"
        );
        self.request(Pending::Close, Message::Close {})
    }
    pub fn notes_page(&mut self, page_index: u32) -> Result<Frame> {
        ensure!(
            self.ready
                && !self
                    .pending
                    .values()
                    .any(|p| matches!(p, Pending::Close | Pending::Shutdown)),
            "helper is not ready for notes"
        );
        ensure!(
            page_index < self.page_count.context("document not open")?,
            "page outside document"
        );
        self.request(
            Pending::NotesPage(page_index),
            Message::NotesPage { page_index },
        )
    }
    fn check_session(&self, session: RenderSessionId) -> Result<()> {
        ensure!(session == self.session, "stale session");
        Ok(())
    }
    /// Use this entry point on the broker to reject stale/unsolicited pixels before allocation.
    pub fn read_response<T: Read>(&self, transport: &mut Framed<T>) -> Result<Option<Frame>> {
        transport.receive_checked(|envelope| self.validate_response(envelope).map(|_| ()))
    }

    fn validate_response(&self, envelope: &Envelope) -> Result<Pending> {
        envelope.validate()?;
        let pending = self
            .pending
            .get(&envelope.request_id)
            .context("unknown or duplicate response ID")?
            .clone();
        let expected_session = if matches!(pending, Pending::Hello | Pending::Shutdown) {
            0
        } else {
            self.session.0
        };
        ensure!(
            envelope.session_id == expected_session,
            "stale response session"
        );
        match (&pending, &envelope.message) {
            (Pending::Hello, Message::Ready {})
            | (Pending::Close, Message::Closed {})
            | (Pending::Shutdown, Message::Stopped {})
            | (Pending::Open, Message::Opened { .. }) => {}
            (Pending::Render(request, _), Message::Pixels { width, .. }) => ensure!(
                *width == request.width as u32,
                "response width differs from request"
            ),
            (Pending::Notes, Message::NotesLoaded { pages }) => {
                let count = self.page_count.context("document not open")?;
                ensure!(
                    pages.iter().all(|n| n.page_number <= count),
                    "notes outside document"
                );
            }
            (Pending::NotesPage(index), Message::NotesLoaded { pages }) => ensure!(
                pages.iter().all(|p| p.page_number == index + 1),
                "notes for unexpected page"
            ),
            (
                Pending::Open | Pending::Render(_, _) | Pending::Notes | Pending::NotesPage(_),
                Message::Failed { .. },
            ) => {}
            _ => bail!("unexpected response kind"),
        }
        Ok(pending)
    }
    pub fn accept(&mut self, frame: Frame) -> Result<Option<RenderEvent>> {
        ensure!(
            frame.envelope.validate()? == frame.pixels.len(),
            "pixel length mismatch"
        );
        let pending = self.validate_response(&frame.envelope)?;
        // Validate everything before changing broker state or allocating Slint pixels.
        let event = match (&pending, &frame.envelope.message) {
            (Pending::Hello, Message::Ready {})
            | (Pending::Close, Message::Closed {})
            | (Pending::Shutdown, Message::Stopped {}) => None,
            (Pending::Open, Message::Opened { title, page_count }) => Some(RenderEvent::Opened {
                session_id: self.session,
                title: title.clone(),
                page_count: *page_count,
                status_text: "Ready".into(),
            }),
            (Pending::Render(request, job), Message::Pixels { width, height, .. }) => {
                ensure!(
                    *width == request.width as u32,
                    "response width differs from request"
                );
                let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    &frame.pixels,
                    *width,
                    *height,
                );
                Some(RenderEvent::PageRendered {
                    session_id: self.session,
                    job_id: *job,
                    request: *request,
                    page: RenderedPagePixels {
                        pixels,
                        aspect_ratio: *width as f32 / *height as f32,
                        estimated_bytes: frame.pixels.len(),
                    },
                })
            }
            (Pending::Notes | Pending::NotesPage(_), Message::NotesLoaded { pages }) => {
                let count = self.page_count.context("document not open")?;
                ensure!(
                    pages.iter().all(|n| n.page_number <= count),
                    "notes outside document"
                );
                Some(RenderEvent::SpeakerNotesLoaded {
                    session_id: self.session,
                    notes: SpeakerNotes::from_page_notes(
                        pages.iter().map(|n| (n.page_number, n.text.clone())),
                    ),
                    status_text: "Ready".into(),
                })
            }
            (Pending::Open, Message::Failed { message, .. }) => Some(RenderEvent::OpenFailed {
                session_id: self.session,
                message: message.clone(),
            }),
            (Pending::Render(request, job), Message::Failed { message, .. }) => {
                Some(RenderEvent::PageFailed {
                    session_id: self.session,
                    job_id: *job,
                    request: *request,
                    message: message.clone(),
                })
            }
            (Pending::Notes | Pending::NotesPage(_), Message::Failed { message, .. }) => {
                Some(RenderEvent::SpeakerNotesLoaded {
                    session_id: self.session,
                    notes: SpeakerNotes::empty(),
                    status_text: message.clone(),
                })
            }
            _ => bail!("unexpected response kind"),
        };
        match frame.envelope.message {
            Message::Ready {} => self.ready = true,
            Message::Opened { page_count, .. } => self.page_count = Some(page_count),
            Message::Closed {} => self.page_count = None,
            Message::Stopped {} => {
                self.ready = false;
                self.page_count = None;
                self.pending.clear();
            }
            _ => {}
        }
        self.pending.remove(&frame.envelope.request_id);
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{render_scheduler::RenderPriority, rendering::RenderPurpose};
    use std::io::Cursor;

    fn frame(message: Message) -> Frame {
        let session_id = if matches!(
            message,
            Message::Hello {} | Message::Ready {} | Message::Shutdown {} | Message::Stopped {}
        ) {
            0
        } else {
            9
        };
        Frame {
            envelope: Envelope {
                request_id: 1,
                session_id,
                message,
            },
            pixels: Vec::new(),
        }
    }
    fn encode(frame: &Frame) -> Vec<u8> {
        let mut writer = Framed::new(Vec::new());
        writer.send(frame).unwrap();
        writer.into_inner()
    }
    fn response(request: &Frame, message: Message) -> Frame {
        Frame {
            envelope: Envelope {
                request_id: request.envelope.request_id,
                session_id: request.envelope.session_id,
                message,
            },
            pixels: Vec::new(),
        }
    }
    fn opened_broker() -> Broker {
        let mut broker = Broker::new(RenderSessionId(9)).unwrap();
        let hello = broker.hello().unwrap();
        broker.accept(response(&hello, Message::Ready {})).unwrap();
        let open = broker
            .command(
                RenderCommand::Open {
                    session_id: RenderSessionId(9),
                    path: PathBuf::from("deck.pdf"),
                },
                RenderJobId(0),
            )
            .unwrap();
        assert!(matches!(
            broker
                .accept(response(
                    &open,
                    Message::Opened {
                        title: "Deck".into(),
                        page_count: 3
                    }
                ))
                .unwrap(),
            Some(RenderEvent::Opened { page_count: 3, .. })
        ));
        broker
    }
    fn render_command(index: u32) -> RenderCommand {
        RenderCommand::RenderPage {
            session_id: RenderSessionId(9),
            request: RenderRequest {
                page_index: index,
                width: 2,
                purpose: RenderPurpose::CurrentSlide,
            },
            priority: RenderPriority::BlockingVisible,
        }
    }

    #[test]
    fn all_message_variants_round_trip_in_a_stream() {
        let messages = vec![
            Message::Hello {},
            Message::Ready {},
            Message::Open {
                path: NativePath::from_path("日本語.pdf".into()),
            },
            Message::Render {
                page_index: 2,
                width: 1920,
            },
            Message::Notes {},
            Message::NotesPage { page_index: 1 },
            Message::Close {},
            Message::Shutdown {},
            Message::Opened {
                title: "Deck".into(),
                page_count: 3,
            },
            Message::Pixels {
                width: 2,
                height: 1,
                format: PixelFormat::Rgba8,
            },
            Message::NotesLoaded {
                pages: vec![PageNote {
                    page_number: 2,
                    text: "speaker note".into(),
                }],
            },
            Message::Closed {},
            Message::Stopped {},
            Message::Failed {
                code: FailureCode::InvalidPdf,
                message: "Invalid PDF".into(),
            },
        ];
        let mut writer = Framed::new(Vec::new());
        let frames: Vec<_> = messages
            .into_iter()
            .map(|m| {
                let mut f = frame(m);
                if matches!(f.envelope.message, Message::Pixels { .. }) {
                    f.pixels = vec![0, 1, 2, 255, 4, 5, 6, 255];
                }
                writer.send(&f).unwrap();
                f
            })
            .collect();
        let mut reader = Framed::new(Cursor::new(writer.into_inner()));
        for expected in frames {
            assert_eq!(reader.receive().unwrap(), Some(expected));
        }
        assert_eq!(reader.receive().unwrap(), None);
    }

    #[test]
    fn notes_page_rejects_other_pages_without_consuming_pending_request() {
        let mut broker = opened_broker();
        assert!(broker.notes_page(3).is_err());
        let request = broker.notes_page(1).unwrap();
        let wrong = response(
            &request,
            Message::NotesLoaded {
                pages: vec![PageNote {
                    page_number: 1,
                    text: "wrong".into(),
                }],
            },
        );
        assert!(broker.accept(wrong).is_err());
        assert!(matches!(
            broker
                .accept(response(
                    &request,
                    Message::NotesLoaded {
                        pages: vec![PageNote {
                            page_number: 2,
                            text: "correct".into()
                        }]
                    }
                ))
                .unwrap(),
            Some(RenderEvent::SpeakerNotesLoaded { .. })
        ));
    }

    #[test]
    fn every_truncated_prefix_fails_except_clean_eof() {
        let mut f = frame(Message::Pixels {
            width: 2,
            height: 1,
            format: PixelFormat::Rgba8,
        });
        f.pixels = vec![42; 8];
        let bytes = encode(&f);
        for end in 1..bytes.len() {
            assert!(
                Framed::new(Cursor::new(&bytes[..end])).receive().is_err(),
                "prefix {end}"
            );
        }
        assert_eq!(
            Framed::new(Cursor::new(Vec::<u8>::new()))
                .receive()
                .unwrap(),
            None
        );
    }

    #[test]
    fn rejects_header_limits_before_reading_the_body() {
        let bytes = encode(&frame(Message::Hello {}));
        for (offset, value) in [
            (6, 0u32),
            (6, MAX_CONTROL_BYTES as u32 + 1),
            (10, MAX_PIXEL_BYTES as u32 + 1),
        ] {
            let mut hostile = bytes[..14].to_vec();
            hostile[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(Framed::new(Cursor::new(hostile)).receive().is_err());
        }
        for offset in [0, 4] {
            let mut hostile = bytes.clone();
            hostile[offset] ^= 255;
            assert!(Framed::new(Cursor::new(hostile)).receive().is_err());
        }
    }

    #[test]
    fn hostile_control_and_pixel_metadata_are_rejected() {
        for message in [
            Message::Opened {
                title: "a".repeat(MAX_TEXT_BYTES + 1),
                page_count: 1,
            },
            Message::Opened {
                title: "Deck".into(),
                page_count: 0,
            },
            Message::Opened {
                title: "Deck".into(),
                page_count: MAX_PAGES + 1,
            },
            Message::Render {
                page_index: MAX_PAGES,
                width: 1,
            },
            Message::Render {
                page_index: 0,
                width: 0,
            },
            Message::Failed {
                code: FailureCode::Internal,
                message: "a".repeat(MAX_TEXT_BYTES + 1),
            },
            Message::NotesLoaded {
                pages: vec![PageNote {
                    page_number: 1,
                    text: "a".repeat(MAX_NOTE_BYTES + 1),
                }],
            },
            Message::NotesLoaded {
                pages: vec![
                    PageNote {
                        page_number: 1,
                        text: "a".into(),
                    },
                    PageNote {
                        page_number: 1,
                        text: "b".into(),
                    },
                ],
            },
        ] {
            assert!(frame(message).envelope.validate().is_err());
        }
        for (width, height) in [
            (0, 1),
            (1, 0),
            (u32::MAX, 1),
            (1, u32::MAX),
            (MAX_DIMENSION, MAX_DIMENSION),
        ] {
            assert!(pixel_len(width, height).is_err());
        }
        assert_eq!(pixel_len(4096, 4096).unwrap(), MAX_PIXEL_BYTES);
        assert!(frame(Message::NotesLoaded {
            pages: (1..=9)
                .map(|page_number| PageNote {
                    page_number,
                    text: "a".repeat(MAX_NOTE_BYTES)
                })
                .collect()
        })
        .envelope
        .validate()
        .is_err());
    }

    #[test]
    fn decoder_rejects_unknown_and_extra_fields_and_payload_mismatch() {
        for control in [br#"{"request_id":1,"session_id":0,"message":{"kind":"Unknown"}}"#.as_slice(), br#"{"request_id":1,"session_id":0,"message":{"kind":"Hello","extra":1}}"#, br#"{"request_id":1,"session_id":9,"message":{"kind":"Pixels","width":0,"height":1,"format":"Rgba8"}}"#, br#"{"request_id":1,"session_id":9,"message":{"kind":"Pixels","width":2,"height":1,"format":"Bgra8"}}"#] {
            let mut bytes = Vec::new(); bytes.extend(MAGIC); bytes.extend(VERSION.to_le_bytes()); bytes.extend((control.len() as u32).to_le_bytes()); bytes.extend(0u32.to_le_bytes()); bytes.extend(control);
            assert!(Framed::new(Cursor::new(bytes)).receive().is_err());
        }
        let mut bytes = encode(&frame(Message::Hello {}));
        bytes[10..14].copy_from_slice(&4u32.to_le_bytes());
        bytes.extend([0; 4]);
        assert!(Framed::new(Cursor::new(bytes)).receive().is_err());
    }

    #[test]
    fn native_paths_preserve_non_unicode_data_and_reject_nul() {
        #[cfg(unix)]
        let path = {
            use std::os::unix::ffi::OsStringExt;
            PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', 255]))
        };
        #[cfg(windows)]
        let path = {
            use std::os::windows::ffi::OsStringExt;
            PathBuf::from(std::ffi::OsString::from_wide(&[0xD800, 65]))
        };
        assert_eq!(
            NativePath::from_path(path.clone()).into_path().unwrap(),
            path
        );
        assert!(NativePath::Unix(vec![0]).validate().is_err());
        assert!(NativePath::Windows(vec![1]).validate().is_err());
        assert!(NativePath::Windows(vec![0, 0]).validate().is_err());
        assert!(NativePath::Unix(vec![1; MAX_PATH_BYTES + 1])
            .validate()
            .is_err());
    }

    #[test]
    fn broker_rejects_stale_duplicate_wrong_kind_without_mutation() {
        let mut broker = opened_broker();
        let request = broker.command(render_command(1), RenderJobId(7)).unwrap();
        let pending_len = broker.pending.len();
        let mut good = response(
            &request,
            Message::Pixels {
                width: 2,
                height: 1,
                format: PixelFormat::Rgba8,
            },
        );
        good.pixels = vec![255; 8];
        let mut stale = good.clone();
        stale.envelope.session_id += 1;
        let mut unknown = good.clone();
        unknown.envelope.request_id += 1;
        let wrong = response(
            &request,
            Message::Opened {
                title: "Wrong".into(),
                page_count: 1,
            },
        );
        for f in [stale, unknown, wrong] {
            assert!(broker.accept(f).is_err());
            assert_eq!(broker.pending.len(), pending_len);
            assert_eq!(broker.page_count, Some(3));
        }
        assert!(matches!(
            broker.accept(good.clone()).unwrap(),
            Some(RenderEvent::PageRendered {
                job_id: RenderJobId(7),
                ..
            })
        ));
        assert!(broker.accept(good).is_err());
    }

    #[test]
    fn broker_notes_failure_close_shutdown_and_request_caps() {
        let mut broker = opened_broker();
        let request = broker
            .command(
                RenderCommand::ExtractSpeakerNotes {
                    session_id: RenderSessionId(9),
                },
                RenderJobId(0),
            )
            .unwrap();
        assert!(broker
            .accept(response(
                &request,
                Message::NotesLoaded {
                    pages: vec![PageNote {
                        page_number: 4,
                        text: "outside".into()
                    }]
                }
            ))
            .is_err());
        assert!(matches!(
            broker
                .accept(response(
                    &request,
                    Message::NotesLoaded {
                        pages: vec![PageNote {
                            page_number: 3,
                            text: "end".into()
                        }]
                    }
                ))
                .unwrap(),
            Some(RenderEvent::SpeakerNotesLoaded { .. })
        ));
        let request = broker.command(render_command(0), RenderJobId(1)).unwrap();
        assert!(matches!(
            broker
                .accept(response(
                    &request,
                    Message::Failed {
                        code: FailureCode::RenderFailed,
                        message: "failed".into()
                    }
                ))
                .unwrap(),
            Some(RenderEvent::PageFailed { .. })
        ));
        assert!(broker.command(render_command(3), RenderJobId(1)).is_err());
        let close = broker.close().unwrap();
        broker.accept(response(&close, Message::Closed {})).unwrap();
        assert_eq!(broker.page_count, None);
        let shutdown = broker
            .command(RenderCommand::Shutdown, RenderJobId(0))
            .unwrap();
        broker
            .accept(response(&shutdown, Message::Stopped {}))
            .unwrap();
        assert!(!broker.ready);
        let mut broker = opened_broker();
        for _ in 0..MAX_PENDING_REQUESTS {
            broker.command(render_command(0), RenderJobId(1)).unwrap();
        }
        assert!(broker.command(render_command(0), RenderJobId(1)).is_err());
    }

    #[test]
    fn deterministic_fuzz_bytes_never_panic() {
        let seed = encode(&frame(Message::Hello {}));
        for index in 0..seed.len() {
            for byte in 0..=255u8 {
                let mut mutated = seed.clone();
                mutated[index] = byte;
                let _ = Framed::new(Cursor::new(mutated)).receive();
            }
        }
    }

    #[test]
    fn broker_rejects_unsolicited_pixels_before_reading_the_payload() {
        let broker = opened_broker();
        let mut f = frame(Message::Pixels {
            width: 2,
            height: 1,
            format: PixelFormat::Rgba8,
        });
        f.pixels = vec![0; 8];
        let mut bytes = encode(&f);
        bytes.truncate(bytes.len() - 8);
        let error = broker
            .read_response(&mut Framed::new(Cursor::new(bytes)))
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("unknown or duplicate response ID"));
        assert!(broker.pending.is_empty());
    }

    #[test]
    fn short_reads_and_writes_preserve_frames() {
        struct Chunks<T>(T);
        impl<T: Read> Read for Chunks<T> {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                let len = bytes.len().min(1);
                self.0.read(&mut bytes[..len])
            }
        }
        impl<T: Write> Write for Chunks<T> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.write(&bytes[..bytes.len().min(1)])
            }
            fn flush(&mut self) -> io::Result<()> {
                self.0.flush()
            }
        }
        let f = frame(Message::Hello {});
        let mut writer = Framed::new(Chunks(Vec::new()));
        writer.send(&f).unwrap();
        let mut reader = Framed::new(Chunks(Cursor::new(writer.into_inner().0)));
        assert_eq!(reader.receive().unwrap(), Some(f));
    }

    #[test]
    fn oversized_serialized_control_does_not_write_a_partial_frame() {
        let f = frame(Message::NotesLoaded {
            pages: (1..=8)
                .map(|page_number| PageNote {
                    page_number,
                    text: "\u{0001}".repeat(MAX_NOTE_BYTES),
                })
                .collect(),
        });
        let mut writer = Framed::new(Vec::new());
        assert!(writer.send(&f).is_err());
        assert!(writer.into_inner().is_empty());
    }

    #[test]
    fn bounded_dimension_combinations_round_trip() {
        for width in [1, 2, 3, 17, 64] {
            for height in [1, 2, 3, 17, 64] {
                let mut f = frame(Message::Pixels {
                    width,
                    height,
                    format: PixelFormat::Rgba8,
                });
                f.pixels = (0..pixel_len(width, height).unwrap())
                    .map(|n| n as u8)
                    .collect();
                assert_eq!(
                    Framed::new(Cursor::new(encode(&f))).receive().unwrap(),
                    Some(f)
                );
            }
        }
    }

    #[test]
    fn shutdown_remains_available_at_capacity_and_blocks_later_work() {
        let mut broker = opened_broker();
        for _ in 0..MAX_PENDING_REQUESTS {
            broker.command(render_command(0), RenderJobId(1)).unwrap();
        }
        let shutdown = broker
            .command(RenderCommand::Shutdown, RenderJobId(0))
            .unwrap();
        assert!(broker.command(render_command(0), RenderJobId(1)).is_err());
        assert!(broker
            .command(RenderCommand::Shutdown, RenderJobId(0))
            .is_err());
        broker
            .accept(response(&shutdown, Message::Stopped {}))
            .unwrap();
        assert!(broker.pending.is_empty());
        let mut broker = opened_broker();
        let close = broker.close().unwrap();
        assert!(broker.command(render_command(0), RenderJobId(1)).is_err());
        broker.accept(response(&close, Message::Closed {})).unwrap();
    }

    #[test]
    fn handshake_ids_and_rejected_requests_preserve_state() {
        let mut broker = Broker::new(RenderSessionId(9)).unwrap();
        assert!(broker.command(render_command(0), RenderJobId(1)).is_err());
        let hello = broker.hello().unwrap();
        assert!(broker.hello().is_err());
        let mut bad = response(&hello, Message::Ready {});
        bad.envelope.request_id = 0;
        assert!(broker.accept(bad).is_err());
        assert!(!broker.ready);
        broker.accept(response(&hello, Message::Ready {})).unwrap();
        assert!(broker.hello().is_err());
        let mut broker = opened_broker();
        for (session, index, width, job) in [
            (8, 0, 2, 1),
            (9, 3, 2, 1),
            (9, 0, -1, 1),
            (9, 0, 0, 1),
            (9, 0, 2, 0),
        ] {
            let command = RenderCommand::RenderPage {
                session_id: RenderSessionId(session),
                request: RenderRequest {
                    page_index: index,
                    width,
                    purpose: RenderPurpose::Thumbnail,
                },
                priority: RenderPriority::Background,
            };
            let next = broker.next_id;
            assert!(broker.command(command, RenderJobId(job)).is_err());
            assert!(broker.pending.is_empty());
            assert_eq!(broker.next_id, next);
        }
    }
}

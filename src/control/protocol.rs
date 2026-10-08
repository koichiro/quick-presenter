use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub protocol_version: u32,
    pub id: u64,
    #[serde(flatten)]
    pub command: Command,
}

impl Request {
    pub fn new(id: u64, command: Command) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            id,
            command,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", deny_unknown_fields)]
pub enum Command {
    #[serde(rename = "presentation.status")]
    Status(Empty),
    #[serde(rename = "presentation.timer.elapsed")]
    TimerElapsed(Empty),
    #[serde(rename = "presentation.watch")]
    Watch(Empty),
    #[serde(rename = "presentation.next")]
    Next(Empty),
    #[serde(rename = "presentation.previous")]
    Previous(Empty),
    #[serde(rename = "presentation.goto")]
    GoTo(PageParams),
    #[serde(rename = "presentation.open")]
    Open(OpenParams),
    #[serde(rename = "presentation.close")]
    Close(Empty),
    #[serde(rename = "presentation.blackout")]
    Blackout(BlackoutParams),
    #[serde(rename = "presentation.notes")]
    Notes(Empty),
    #[serde(rename = "presentation.slide")]
    Slide(ContentParams),
    #[serde(rename = "presentation.context")]
    Context(ContentParams),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentParams {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub full: bool,
}
impl Command {
    pub fn full_text(&self) -> bool {
        matches!(
            self,
            Self::Slide(ContentParams { full: true }) | Self::Context(ContentParams { full: true })
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageParams {
    pub page: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenParams {
    pub file: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlackoutParams {
    pub value: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub protocol_version: u32,
    pub id: Option<u64>,
    #[serde(flatten)]
    pub outcome: Outcome,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Result(Reply),
    Error(ControlError),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Status(Status),
    TimerElapsed {
        session_id: String,
        document_revision: u64,
        #[serde(flatten)]
        timer: TimerStatus,
    },
    Watching {
        state: Status,
        sequence: u64,
    },
    Event {
        envelope: EventEnvelope,
    },
    Heartbeat {},
    Transfer {
        chunk: TransferChunk,
    },
    Slide {
        session_id: String,
        document_revision: u64,
        pages: u32,
        #[serde(flatten)]
        content: SlideText,
    },
    Context {
        presentation: Status,
        current: SlideContext,
        next: Option<SlideContext>,
    },
    Mutation {
        changed: bool,
        state: Status,
    },
    Notes {
        session_id: String,
        document_revision: u64,
        page: u32,
        notes: String,
    },
}
impl Response {
    pub fn success(id: u64, reply: Reply) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            id: Some(id),
            outcome: Outcome::Result(reply),
        }
    }
    pub fn error(id: Option<u64>, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            id,
            outcome: Outcome::Error(ControlError::new(code, message)),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlError {
    pub code: ErrorCode,
    pub message: String,
}
impl ControlError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    NotRunning,
    NoPresentation,
    InvalidPage,
    InvalidRequest,
    UnknownMethod,
    ProtocolMismatch,
    IpcFailure,
    Busy,
    Timeout,
    OpenFailed,
    NotesLoading,
    NotesFailed,
    TextFailed,
    Cancelled,
    UnsupportedPlatform,
    EventsLagged,
}
impl ErrorCode {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::EventsLagged => 11,
            Self::NotRunning => 3,
            Self::NoPresentation => 4,
            Self::InvalidPage => 5,
            Self::InvalidRequest | Self::UnknownMethod => 2,
            Self::ProtocolMismatch => 6,
            Self::IpcFailure | Self::UnsupportedPlatform => 7,
            Self::Busy | Self::NotesLoading => 8,
            Self::Timeout => 9,
            Self::OpenFailed | Self::NotesFailed | Self::TextFailed | Self::Cancelled => 10,
        }
    }
}
pub const MAX_ASSEMBLED_BYTES: usize = 32 * 1024 * 1024;
pub const TRANSFER_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_FULL_SLIDE_TEXT_BYTES: usize = 4 * 1024 * 1024;
/// Transport-only fragments of a serialized typed response. Never application state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferChunk {
    pub sequence: u32,
    pub total_bytes: u32,
    pub data: Vec<u8>,
}
impl TransferChunk {
    pub fn valid(&self) -> bool {
        let total = self.total_bytes as usize;
        total > MAX_FRAME_BYTES
            && total <= MAX_ASSEMBLED_BYTES
            && !self.data.is_empty()
            && self.data.len() <= TRANSFER_CHUNK_BYTES
            && self.sequence < (MAX_ASSEMBLED_BYTES / TRANSFER_CHUNK_BYTES) as u32
    }
}

pub fn serialize_bounded(value: &impl Serialize, limit: usize) -> io::Result<Vec<u8>> {
    struct Buffer {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Serialized response exceeds limit",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut buffer, value)?;
    Ok(buffer.bytes)
}

pub fn write_response(writer: &mut impl Write, response: &Response) -> io::Result<()> {
    let bytes = serialize_bounded(response, MAX_ASSEMBLED_BYTES)?;
    if bytes.len() <= MAX_FRAME_BYTES {
        return write_frame(writer, response);
    }
    for (sequence, data) in bytes.chunks(TRANSFER_CHUNK_BYTES).enumerate() {
        let fragment = Response {
            protocol_version: response.protocol_version,
            id: response.id,
            outcome: Outcome::Result(Reply::Transfer {
                chunk: TransferChunk {
                    sequence: sequence as u32,
                    total_bytes: bytes.len() as u32,
                    data: data.to_vec(),
                },
            }),
        };
        write_frame(writer, &fragment)?;
    }
    Ok(())
}

/// PDF source text in PDFium order. Truncation is explicit; no OCR or generated content.
pub const MAX_SLIDE_TEXT_BYTES: usize = 4 * 1024;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlideText {
    pub page: u32,
    pub text: Vec<String>,
    pub truncated: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlideContext {
    #[serde(flatten)]
    pub slide: SlideText,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub session_id: String,
    pub document_revision: u64,
    pub document: Option<String>,
    pub page: Option<u32>,
    pub pages: u32,
    pub fullscreen: bool,
    pub blackout: bool,
    pub timer: TimerStatus,
    pub opening: bool,
    pub render_state: RenderState,
    pub notes_state: NotesState,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerStatus {
    pub running: bool,
    pub elapsed_seconds: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotesState {
    #[default]
    Empty,
    Loading,
    Ready,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderState {
    Empty,
    Rendering,
    Ready,
    Failed,
}

/// Ordered owner-state changes. Sequence is instance-wide; watches start with a snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub protocol_version: u32,
    pub session_id: String,
    pub document_revision: u64,
    pub sequence: u64,
    #[serde(flatten)]
    pub event: Event,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum Event {
    #[serde(rename = "presentation.snapshot")]
    Snapshot { state: Status },
    #[serde(rename = "presentation.opened")]
    Opened { state: Status },
    #[serde(rename = "presentation.reloaded")]
    Reloaded { state: Status },
    #[serde(rename = "presentation.closed")]
    Closed {},
    #[serde(rename = "page.changed")]
    PageChanged { page: u32, pages: u32 },
    #[serde(rename = "blackout.changed")]
    BlackoutChanged { value: bool },
    #[serde(rename = "timer.started")]
    TimerStarted { timer: TimerStatus },
    #[serde(rename = "timer.reset")]
    TimerReset { timer: TimerStatus },
}

pub fn decode_request(bytes: &[u8]) -> Result<Request, Response> {
    #[derive(Deserialize)]
    struct Header {
        id: u64,
        protocol_version: u32,
        method: String,
    }
    let header: Header = serde_json::from_slice(bytes).map_err(|_| {
        Response::error(
            None,
            ErrorCode::InvalidRequest,
            "Malformed request envelope.",
        )
    })?;
    if header.protocol_version != PROTOCOL_VERSION {
        return Err(Response::error(
            Some(header.id),
            ErrorCode::ProtocolMismatch,
            "Unsupported control protocol version.",
        ));
    }
    if !matches!(
        header.method.as_str(),
        "presentation.status"
            | "presentation.timer.elapsed"
            | "presentation.watch"
            | "presentation.next"
            | "presentation.previous"
            | "presentation.goto"
            | "presentation.open"
            | "presentation.close"
            | "presentation.blackout"
            | "presentation.notes"
            | "presentation.slide"
            | "presentation.context"
    ) {
        return Err(Response::error(
            Some(header.id),
            ErrorCode::UnknownMethod,
            "Unknown control method.",
        ));
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|_| {
        Response::error(
            Some(header.id),
            ErrorCode::InvalidRequest,
            "Invalid method parameters.",
        )
    })?;
    if let Command::Open(params) = &request.command {
        if params.file.is_empty()
            || params.file.len() > 32768
            || params.file.contains('\0')
            || !std::path::Path::new(&params.file).is_absolute()
        {
            return Err(Response::error(
                Some(header.id),
                ErrorCode::InvalidRequest,
                "An absolute UTF-8 file path is required.",
            ));
        }
    }
    Ok(request)
}

// Length-prefixed JSON allows bounded allocation and unambiguous stream framing.
pub fn read_frame(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid control frame length",
        ));
    }
    let mut data = vec![0; length];
    reader.read_exact(&mut data)?;
    Ok(data)
}
pub fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > MAX_FRAME_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Control frame too large",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut data = Bounded(Vec::new());
    serde_json::to_writer(&mut data, value)?;
    writer.write_all(&(data.0.len() as u32).to_be_bytes())?;
    writer.write_all(&data.0)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn elapsed_and_watch_requests_are_versioned_but_timer_mutation_is_unavailable() {
        for command in [Command::TimerElapsed(Empty {}), Command::Watch(Empty {})] {
            let request = Request::new(42, command);
            assert_eq!(
                decode_request(&serde_json::to_vec(&request).unwrap()).unwrap(),
                request
            );
        }
        for method in [
            "presentation.timer.start",
            "presentation.timer.stop",
            "presentation.timer.reset",
        ] {
            let request =
                format!(r#"{{"protocol_version":1,"id":42,"method":"{method}","params":{{}}}}"#);
            assert!(matches!(
                decode_request(request.as_bytes()).unwrap_err().outcome,
                Outcome::Error(ControlError {
                    code: ErrorCode::UnknownMethod,
                    ..
                })
            ));
        }
        assert!(matches!(decode_request(br#"{"protocol_version":1,"id":42,"method":"presentation.timer.elapsed","params":{"reset":true}}"#).unwrap_err().outcome,
            Outcome::Error(ControlError { code: ErrorCode::InvalidRequest, .. })));
    }
    #[test]
    fn context_roundtrip_and_worst_case_escaped_text_fit_frame_budget() {
        let slide = SlideText {
            page: 1,
            text: vec!["\u{1}".repeat(MAX_SLIDE_TEXT_BYTES)],
            truncated: true,
        };
        let current = SlideContext {
            slide,
            notes: "\u{1}".repeat(64 * 1024),
        };
        let next = SlideContext {
            slide: SlideText {
                page: 2,
                ..current.slide.clone()
            },
            notes: current.notes.clone(),
        };
        let presentation = Status {
            session_id: "session".into(),
            document_revision: 1,
            document: Some("\u{1}".repeat(32768)),
            page: Some(1),
            pages: 2,
            fullscreen: false,
            blackout: false,
            timer: TimerStatus {
                running: true,
                elapsed_seconds: 12,
            },
            opening: false,
            render_state: RenderState::Ready,
            notes_state: NotesState::Ready,
        };
        let response = Response::success(
            42,
            Reply::Context {
                presentation,
                current,
                next: Some(next),
            },
        );
        let mut frame = Vec::new();
        write_frame(&mut frame, &response).unwrap();
        let decoded: Response =
            serde_json::from_slice(&read_frame(&mut frame.as_slice()).unwrap()).unwrap();
        assert_eq!(decoded, response);
    }
    #[test]
    fn commands_roundtrip() {
        let absolute_pdf = std::env::current_dir()
            .unwrap()
            .join("slides/demo.pdf")
            .to_string_lossy()
            .into_owned();
        for command in [
            Command::Status(Empty {}),
            Command::Next(Empty {}),
            Command::Previous(Empty {}),
            Command::GoTo(PageParams { page: 5 }),
            Command::Open(OpenParams { file: absolute_pdf }),
            Command::Close(Empty {}),
            Command::Blackout(BlackoutParams { value: true }),
            Command::Notes(Empty {}),
            Command::Slide(ContentParams::default()),
            Command::Context(ContentParams::default()),
        ] {
            let request = Request::new(42, command);
            let mut bytes = Vec::new();
            write_frame(&mut bytes, &request).unwrap();
            assert_eq!(
                decode_request(&read_frame(&mut bytes.as_slice()).unwrap()).unwrap(),
                request
            );
        }
    }
    #[test]
    fn invalid_requests_have_machine_errors() {
        for (data, code) in [
            (
                r#"{"id":42,"protocol_version":2,"method":"presentation.status","params":{}}"#,
                ErrorCode::ProtocolMismatch,
            ),
            (
                r#"{"id":42,"protocol_version":1,"method":"unknown","params":{}}"#,
                ErrorCode::UnknownMethod,
            ),
            (
                r#"{"id":42,"protocol_version":1,"method":"presentation.goto","params":{"page":"5"}}"#,
                ErrorCode::InvalidRequest,
            ),
            (
                r#"{"id":42,"protocol_version":1,"method":"presentation.next","params":{"page":5}}"#,
                ErrorCode::InvalidRequest,
            ),
            (
                r#"{"id":42,"protocol_version":1,"method":"presentation.open","params":{"file":"relative.pdf"}}"#,
                ErrorCode::InvalidRequest,
            ),
            ("{", ErrorCode::InvalidRequest),
        ] {
            assert!(
                matches!(decode_request(data.as_bytes()).unwrap_err().outcome, Outcome::Error(e) if e.code == code)
            );
        }
    }
    #[test]
    fn framing_rejects_truncation_and_oversize() {
        assert!(read_frame(&mut &[][..]).is_err());
        assert!(read_frame(&mut &[0, 0, 0, 2, b'{'][..]).is_err());
        assert!(read_frame(&mut &u32::MAX.to_be_bytes()[..]).is_err());
        assert!(write_frame(&mut Vec::new(), &"x".repeat(MAX_FRAME_BYTES)).is_err());
    }
    #[test]
    fn error_response_roundtrips() {
        let response = Response::error(
            Some(42),
            ErrorCode::NoPresentation,
            "No presentation is open.",
        );
        let bytes = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            serde_json::from_slice::<Response>(&bytes).unwrap(),
            response
        );
        assert_eq!(response.protocol_version, 1);
    }
}

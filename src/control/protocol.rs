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
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
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
    Cancelled,
    UnsupportedPlatform,
}
impl ErrorCode {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::NotRunning => 3,
            Self::NoPresentation => 4,
            Self::InvalidPage => 5,
            Self::InvalidRequest | Self::UnknownMethod => 2,
            Self::ProtocolMismatch => 6,
            Self::IpcFailure | Self::UnsupportedPlatform => 7,
            Self::Busy | Self::NotesLoading => 8,
            Self::Timeout => 9,
            Self::OpenFailed | Self::NotesFailed | Self::Cancelled => 10,
        }
    }
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
            | "presentation.next"
            | "presentation.previous"
            | "presentation.goto"
            | "presentation.open"
            | "presentation.close"
            | "presentation.blackout"
            | "presentation.notes"
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
    fn commands_roundtrip() {
        for command in [
            Command::Status(Empty {}),
            Command::Next(Empty {}),
            Command::Previous(Empty {}),
            Command::GoTo(PageParams { page: 5 }),
            Command::Open(OpenParams {
                file: "/slides/demo.pdf".into(),
            }),
            Command::Close(Empty {}),
            Command::Blackout(BlackoutParams { value: true }),
            Command::Notes(Empty {}),
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

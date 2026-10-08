use super::protocol::*;
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

pub struct CliOptions {
    pub command: Command,
    pub json: bool,
}
pub enum CliRequest {
    Help,
    Run(CliOptions),
}
pub const HELP: &str = "qp — control a running Quick Presenter\n\nUsage: qp <command> [--json]\n\nCommands:\n  status             Query presentation state\n  next | prev        Move one page\n  goto <page>        Go to a one-based page number\n  open <file>        Open a PDF (relative to the CLI working directory)\n  close              Close the PDF, leaving the application running\n  blackout on|off    Set the audience black screen\n  notes              Read current-page speaker notes\n\nLocal control is experimental and supports Windows, macOS, and Linux.\n";

pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<CliRequest, ControlError> {
    let mut args: Vec<_> = args.into_iter().collect();
    if args == [OsString::from("--help")] || args == [OsString::from("-h")] {
        return Ok(CliRequest::Help);
    }
    let json_count = args.iter().filter(|arg| *arg == "--json").count();
    let json = json_count == 1;
    if json_count > 1 {
        return Err(invalid());
    }
    args.retain(|arg| arg != "--json");
    let first = args
        .first()
        .and_then(|arg| arg.to_str())
        .ok_or_else(invalid)?;
    let command = match (first, args.len()) {
        ("status", 1) => Command::Status(Empty {}),
        ("next", 1) => Command::Next(Empty {}),
        ("prev", 1) => Command::Previous(Empty {}),
        ("close", 1) => Command::Close(Empty {}),
        ("notes", 1) => Command::Notes(Empty {}),
        ("goto", 2) => {
            let page = args[1]
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|page| *page > 0)
                .ok_or_else(|| {
                    ControlError::new(ErrorCode::InvalidPage, "Page must be a positive integer.")
                })?;
            Command::GoTo(PageParams { page })
        }
        ("open", 2) => {
            let path = PathBuf::from(&args[1]);
            if path.as_os_str().is_empty() {
                return Err(invalid());
            }
            let path = if path.is_absolute() {
                path
            } else {
                std::env::current_dir().map_err(ipc_error)?.join(path)
            };
            let file = path.into_os_string().into_string().map_err(|_| {
                ControlError::new(
                    ErrorCode::InvalidRequest,
                    "Control v1 requires a UTF-8 file path.",
                )
            })?;
            if file.len() > 32768 || file.contains('\0') {
                return Err(invalid());
            }
            Command::Open(OpenParams { file })
        }
        ("blackout", 2) => {
            let value = match args[1].to_str() {
                Some("on") => true,
                Some("off") => false,
                _ => return Err(invalid()),
            };
            Command::Blackout(BlackoutParams { value })
        }
        _ => return Err(invalid()),
    };
    Ok(CliRequest::Run(CliOptions { command, json }))
}
fn invalid() -> ControlError {
    ControlError::new(
        ErrorCode::InvalidRequest,
        "Invalid command or arguments. Use qp --help.",
    )
}
fn ipc_error(e: io::Error) -> ControlError {
    let code = match e.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => ErrorCode::NotRunning,
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ErrorCode::Timeout,
        io::ErrorKind::Unsupported => ErrorCode::UnsupportedPlatform,
        _ => ErrorCode::IpcFailure,
    };
    ControlError::new(code, e.to_string())
}
pub fn send(request: &Request) -> Result<Response, ControlError> {
    let path = super::transport::endpoint().map_err(ipc_error)?;
    send_to(&path, request)
}
#[cfg(unix)]
pub fn send_to(path: &Path, request: &Request) -> Result<Response, ControlError> {
    use std::os::unix::net::UnixStream;
    super::transport::validate_socket(path).map_err(ipc_error)?;
    let mut stream = UnixStream::connect(path).map_err(ipc_error)?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(1)))
        .map_err(ipc_error)?;
    stream
        .set_read_timeout(Some(REQUEST_TIMEOUT + std::time::Duration::from_secs(2)))
        .map_err(ipc_error)?;
    write_frame(&mut stream, request).map_err(ipc_error)?;
    let bytes = read_frame(&mut stream).map_err(ipc_error)?;
    validate_response(request, &bytes)
}

#[cfg(windows)]
pub fn send_to(path: &Path, request: &Request) -> Result<Response, ControlError> {
    let mut stream = super::transport::windows::connect(path, std::time::Duration::from_secs(1))
        .map_err(ipc_error)?;
    stream.set_deadlines(
        REQUEST_TIMEOUT + std::time::Duration::from_secs(2),
        std::time::Duration::from_secs(1),
    );
    write_frame(&mut stream, request).map_err(ipc_error)?;
    let bytes = read_frame(&mut stream).map_err(ipc_error)?;
    validate_response(request, &bytes)
}

fn validate_response(request: &Request, bytes: &[u8]) -> Result<Response, ControlError> {
    let response: Response = serde_json::from_slice(bytes)
        .map_err(|_| ControlError::new(ErrorCode::IpcFailure, "Malformed control response."))?;
    if response.protocol_version != PROTOCOL_VERSION {
        return Err(ControlError::new(
            ErrorCode::ProtocolMismatch,
            "Unsupported control response version.",
        ));
    }
    if response.id != Some(request.id) {
        return Err(ControlError::new(
            ErrorCode::IpcFailure,
            "Control response ID mismatch.",
        ));
    }
    let matches = match (&request.command, &response.outcome) {
        (_, Outcome::Error(_)) => true,
        (Command::Status(_), Outcome::Result(Reply::Status(_))) => true,
        (Command::Notes(_), Outcome::Result(Reply::Notes { .. })) => true,
        (
            Command::Next(_)
            | Command::Previous(_)
            | Command::GoTo(_)
            | Command::Open(_)
            | Command::Close(_)
            | Command::Blackout(_),
            Outcome::Result(Reply::Mutation { .. }),
        ) => true,
        _ => false,
    };
    if !matches {
        return Err(ControlError::new(
            ErrorCode::IpcFailure,
            "Unexpected control response kind.",
        ));
    }
    Ok(response)
}

#[cfg(not(any(unix, windows)))]
pub fn send_to(_path: &Path, _request: &Request) -> Result<Response, ControlError> {
    Err(ControlError::new(
        ErrorCode::UnsupportedPlatform,
        "Local control is not implemented for this platform.",
    ))
}
pub fn format_reply(reply: &Reply, json: bool) -> Result<String, serde_json::Error> {
    if json {
        #[derive(serde::Serialize)]
        struct Output<'a> {
            protocol_version: u32,
            #[serde(flatten)]
            result: &'a Reply,
        }
        return serde_json::to_string(&Output {
            protocol_version: PROTOCOL_VERSION,
            result: reply,
        })
        .map(|s| s + "\n");
    }
    Ok(match reply {
        Reply::Status(state) => format!(
            "{}\nPage: {} / {}\nBlackout: {}\nRender: {:?}; notes: {:?}\n",
            state.document.as_deref().unwrap_or("No presentation open"),
            state.page.unwrap_or(0),
            state.pages,
            state.blackout,
            state.render_state,
            state.notes_state
        ),
        Reply::Mutation { changed, state } => format!(
            "Page: {} / {}; changed: {}; blackout: {}\n",
            state.page.unwrap_or(0),
            state.pages,
            changed,
            state.blackout
        ),
        Reply::Notes { notes, .. } => format!("{notes}\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Command {
        match parse_args(args.iter().map(OsString::from)).unwrap() {
            CliRequest::Run(options) => options.command,
            _ => panic!("expected command"),
        }
    }
    #[test]
    fn parses_supported_commands() {
        assert_eq!(parse(&["status", "--json"]), Command::Status(Empty {}));
        assert_eq!(parse(&["goto", "5"]), Command::GoTo(PageParams { page: 5 }));
        assert_eq!(
            parse(&["blackout", "on"]),
            Command::Blackout(BlackoutParams { value: true })
        );
        assert_eq!(
            parse(&["blackout", "off"]),
            Command::Blackout(BlackoutParams { value: false })
        );
        for name in ["next", "prev", "close", "notes"] {
            parse(&[name, "--json"]);
        }
        assert!(
            matches!(parse(&["open", "deck with spaces.pdf"]), Command::Open(p) if Path::new(&p.file).is_absolute())
        );
    }
    #[test]
    fn rejects_invalid_arguments_and_deferred_features() {
        for args in [
            &["goto", "0"][..],
            &["goto", "-1"],
            &["blackout", "toggle"],
            &["status", "extra"],
            &["context", "--json"],
            &["status", "--json", "--json"],
            &[],
        ] {
            assert!(parse_args(args.iter().map(OsString::from)).is_err());
        }
    }
    #[test]
    fn json_notes_preserves_source_text_without_decoration() {
        let reply = Reply::Notes {
            session_id: "session".into(),
            document_revision: 1,
            page: 5,
            notes: "Japanese 日本語\n\"notes\"".into(),
        };
        let output = format_reply(&reply, true).unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["protocol_version"], 1);
        assert_eq!(value["page"], 5);
        assert_eq!(value["notes"], "Japanese 日本語\n\"notes\"");
    }
}

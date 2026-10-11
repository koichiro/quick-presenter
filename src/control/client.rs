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
    Export,
    Version { json: bool },
    Run(CliOptions),
}
pub const HELP: &str = "qp — control a running Quick Presenter\n\nUsage: qp <command> [--json] [--full]\n       qp --version [--json]\n\nCommands:\n  status             Query presentation state\n  timer elapsed      Read elapsed seconds (no timer mutation)\n  watch              Stream presentation events (NDJSON with --json)\n  next | prev        Move one page\n  goto <page>        Go to a one-based page number\n  open <file>        Open a PDF (starts the GUI if needed; paths relative to CLI)\n  close              Close the PDF, leaving the application running\n  blackout on|off    Set the audience black screen\n  notes              Read current-page speaker notes\n  slide              Read current-page PDF text (--full for unabridged text)\n  context            Read current and next page text and notes (--full supported)\n  export             Export every page as full context JSON (--json or --JSON)\n\nControl Protocol v1 is a stable public automation API.\nSupported: Windows, Linux, macOS Developer ID/DMG and development builds.\nMac App Store presentation control is not supported in v2.0.0.\n";

pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<CliRequest, ControlError> {
    let mut args: Vec<_> = args.into_iter().collect();
    if args == [OsString::from("--help")] || args == [OsString::from("-h")] {
        return Ok(CliRequest::Help);
    }
    let export = args.iter().any(|arg| arg == "export");
    if export {
        for arg in &mut args {
            if arg == "--JSON" {
                *arg = "--json".into();
            }
        }
    }
    let json_count = args.iter().filter(|arg| *arg == "--json").count();
    let json = json_count == 1;
    if json_count > 1 {
        return Err(invalid());
    }
    let full_count = args.iter().filter(|arg| *arg == "--full").count();
    if full_count > 1 {
        return Err(invalid());
    }
    let full = full_count == 1;
    args.retain(|arg| arg != "--json" && arg != "--full");
    let first = args
        .first()
        .and_then(|arg| arg.to_str())
        .ok_or_else(invalid)?;
    if !full && args.len() == 1 && matches!(first, "--version" | "-V") {
        return Ok(CliRequest::Version { json });
    }
    if full && !matches!(first, "slide" | "context") {
        return Err(invalid());
    }
    if first == "export" && args.len() == 1 {
        return Ok(CliRequest::Export);
    }
    let command = match (first, args.len()) {
        ("status", 1) => Command::Status(Empty {}),
        ("watch", 1) => Command::Watch(Empty {}),
        ("timer", 2) if args[1] == "elapsed" => Command::TimerElapsed(Empty {}),
        ("next", 1) => Command::Next(Empty {}),
        ("prev", 1) => Command::Previous(Empty {}),
        ("close", 1) => Command::Close(Empty {}),
        ("notes", 1) => Command::Notes(Empty {}),
        ("slide", 1) => Command::Slide(ContentParams { full }),
        ("context", 1) => Command::Context(ContentParams { full }),
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
/// Stage the complete JSON array on disk, keeping only adjacent page contexts in memory.
/// No stdout is emitted until every page and the final identity check succeed.
pub fn export(
    mut send_request: impl FnMut(&Request) -> Result<Response, ControlError>,
) -> Result<std::fs::File, ControlError> {
    use std::io::{Seek, SeekFrom, Write};
    let mut id = 0;
    let mut call = |command| {
        id += 1;
        let request = Request::new(id, command);
        let response = send_request(&request)?;
        match validate_typed_response(&request, response)?.outcome {
            Outcome::Result(reply) => Ok(reply),
            Outcome::Error(error) => Err(error),
        }
    };
    let Reply::Status(presentation) = call(Command::Status(Empty {}))? else {
        unreachable!("validated status reply")
    };
    if presentation.page.is_none() || presentation.pages == 0 {
        return Err(ControlError::new(
            ErrorCode::NoPresentation,
            "No presentation is currently open.",
        ));
    }
    let cancelled =
        || ControlError::new(ErrorCode::Cancelled, "Presentation changed during export.");
    let same_document = |state: &Status| {
        state.session_id == presentation.session_id
            && state.document_revision == presentation.document_revision
            && state.document == presentation.document
            && state.pages == presentation.pages
            && state.page.is_some()
    };
    let mut file = tempfile::tempfile().map_err(ipc_error)?;
    file.write_all(b"[\n").map_err(ipc_error)?;
    let mut previous: Option<SlideContext> = None;
    for page in 1..=presentation.pages {
        let Reply::Context {
            presentation: observed,
            current,
            next,
        } = call(Command::ExportPage(ExportPageParams {
            page,
            session_id: presentation.session_id.clone(),
            document_revision: presentation.document_revision,
        }))?
        else {
            unreachable!("validated export reply")
        };
        if !same_document(&observed) {
            return Err(cancelled());
        }
        if current.slide.page != page || next.is_some() {
            return Err(ControlError::new(
                ErrorCode::IpcFailure,
                "Invalid export page response.",
            ));
        }
        if let Some(previous) = previous.take() {
            let output = format_reply(
                &Reply::Context {
                    presentation: presentation.clone(),
                    current: previous,
                    next: Some(current.clone()),
                },
                true,
            )
            .map_err(|e| ControlError::new(ErrorCode::IpcFailure, e.to_string()))?;
            file.write_all(output.trim_end().as_bytes())
                .map_err(ipc_error)?;
            file.write_all(b",\n").map_err(ipc_error)?;
        }
        previous = Some(current);
    }
    let Reply::Status(final_state) = call(Command::Status(Empty {}))? else {
        unreachable!("validated status reply")
    };
    if !same_document(&final_state) {
        return Err(cancelled());
    }
    let output = format_reply(
        &Reply::Context {
            presentation,
            current: previous.expect("nonempty presentation"),
            next: None,
        },
        true,
    )
    .map_err(|e| ControlError::new(ErrorCode::IpcFailure, e.to_string()))?;
    file.write_all(output.as_bytes()).map_err(ipc_error)?;
    file.write_all(b"]\n").map_err(ipc_error)?;
    file.seek(SeekFrom::Start(0)).map_err(ipc_error)?;
    Ok(file)
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
    receive_response(&mut stream, request)
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
    receive_response(&mut stream, request)
}

fn receive_response(
    reader: &mut impl io::Read,
    request: &Request,
) -> Result<Response, ControlError> {
    let bytes = read_frame(reader).map_err(ipc_error)?;
    let first: Response = serde_json::from_slice(&bytes).map_err(|_| {
        ipc_error(io::Error::new(
            io::ErrorKind::InvalidData,
            "Malformed control response",
        ))
    })?;
    let Outcome::Result(Reply::Transfer { chunk }) = first.outcome else {
        return validate_response(request, &bytes);
    };
    let invalid = || ControlError::new(ErrorCode::IpcFailure, "Invalid response transfer.");
    if !request.command.full_text()
        || first.protocol_version != PROTOCOL_VERSION
        || first.id != Some(request.id)
        || !chunk.valid()
        || chunk.sequence != 0
    {
        return Err(invalid());
    }
    let total = chunk.total_bytes;
    let mut assembled = chunk.data;
    let mut sequence = 1;
    while assembled.len() < total as usize {
        let bytes = read_frame(reader).map_err(ipc_error)?;
        let fragment: Response = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let Outcome::Result(Reply::Transfer { chunk }) = fragment.outcome else {
            return Err(invalid());
        };
        if fragment.protocol_version != PROTOCOL_VERSION
            || fragment.id != Some(request.id)
            || !chunk.valid()
            || chunk.total_bytes != total
            || chunk.sequence != sequence
            || chunk.data.len() > (total as usize).saturating_sub(assembled.len())
        {
            return Err(invalid());
        }
        assembled.extend_from_slice(&chunk.data);
        sequence += 1;
    }
    validate_response(request, &assembled)
}

fn validate_response(request: &Request, bytes: &[u8]) -> Result<Response, ControlError> {
    let response: Response = serde_json::from_slice(bytes)
        .map_err(|_| ControlError::new(ErrorCode::IpcFailure, "Malformed control response."))?;
    validate_typed_response(request, response)
}

fn validate_typed_response(
    request: &Request,
    response: Response,
) -> Result<Response, ControlError> {
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
        (Command::TimerElapsed(_), Outcome::Result(Reply::TimerElapsed { .. })) => true,
        (Command::Watch(_), Outcome::Result(Reply::Watching { .. })) => true,
        (Command::Notes(_), Outcome::Result(Reply::Notes { .. })) => true,
        (Command::Slide(_), Outcome::Result(Reply::Slide { .. })) => true,
        (Command::Context(_) | Command::ExportPage(_), Outcome::Result(Reply::Context { .. })) => {
            true
        }
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
    if request.command.full_text() {
        let truncated = match &response.outcome {
            Outcome::Result(Reply::Slide { content, .. }) => content.truncated,
            Outcome::Result(Reply::Context { current, next, .. }) => {
                current.slide.truncated || next.as_ref().is_some_and(|s| s.slide.truncated)
            }
            _ => false,
        };
        if truncated {
            return Err(ControlError::new(
                ErrorCode::IpcFailure,
                "Full source response was truncated.",
            ));
        }
    }
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
pub fn watch(
    request: &Request,
    on_event: impl FnMut(EventEnvelope) -> Result<(), ControlError>,
) -> Result<(), ControlError> {
    watch_to(
        &super::transport::endpoint().map_err(ipc_error)?,
        request,
        on_event,
    )
}
#[cfg(any(unix, windows))]
pub fn watch_to(
    path: &Path,
    request: &Request,
    on_event: impl FnMut(EventEnvelope) -> Result<(), ControlError>,
) -> Result<(), ControlError> {
    if !matches!(request.command, Command::Watch(_)) {
        return Err(invalid());
    }
    #[cfg(unix)]
    let mut stream = {
        super::transport::validate_socket(path).map_err(ipc_error)?;
        let stream = std::os::unix::net::UnixStream::connect(path).map_err(ipc_error)?;
        stream
            .set_write_timeout(Some(std::time::Duration::from_secs(1)))
            .map_err(ipc_error)?;
        stream
            .set_read_timeout(Some(REQUEST_TIMEOUT + std::time::Duration::from_secs(2)))
            .map_err(ipc_error)?;
        stream
    };
    #[cfg(windows)]
    let mut stream = {
        let mut stream =
            super::transport::windows::connect(path, std::time::Duration::from_secs(1))
                .map_err(ipc_error)?;
        stream.set_deadlines(
            REQUEST_TIMEOUT + std::time::Duration::from_secs(2),
            std::time::Duration::from_secs(1),
        );
        stream
    };
    write_frame(&mut stream, request).map_err(ipc_error)?;
    // On Windows deadlines are absolute and must be renewed after the handshake.
    receive_watch(&mut stream, request, on_event, |stream| {
        #[cfg(windows)]
        stream.set_deadlines(
            std::time::Duration::from_secs(5),
            std::time::Duration::from_secs(1),
        );
        #[cfg(unix)]
        let _ = stream;
    })
}
#[cfg(not(any(unix, windows)))]
pub fn watch_to(
    _path: &Path,
    _request: &Request,
    _on_event: impl FnMut(EventEnvelope) -> Result<(), ControlError>,
) -> Result<(), ControlError> {
    Err(ControlError::new(
        ErrorCode::UnsupportedPlatform,
        "Local control is not implemented for this platform.",
    ))
}
fn receive_watch<R: io::Read>(
    reader: &mut R,
    request: &Request,
    mut on_event: impl FnMut(EventEnvelope) -> Result<(), ControlError>,
    mut renew: impl FnMut(&mut R),
) -> Result<(), ControlError> {
    let bytes = read_frame(reader).map_err(ipc_error)?;
    let response = validate_response(request, &bytes)?;
    let (state, mut sequence) = match response.outcome {
        Outcome::Result(Reply::Watching { state, sequence }) => (state, sequence),
        Outcome::Error(error) => return Err(error),
        _ => return Err(invalid()),
    };
    let session_id = state.session_id.clone();
    on_event(EventEnvelope {
        protocol_version: PROTOCOL_VERSION,
        session_id: session_id.clone(),
        document_revision: state.document_revision,
        sequence,
        event: Event::Snapshot { state },
    })?;
    loop {
        renew(reader);
        let bytes = read_frame(reader).map_err(ipc_error)?;
        let response: Response = serde_json::from_slice(&bytes)
            .map_err(|_| ControlError::new(ErrorCode::IpcFailure, "Malformed event response."))?;
        if response.protocol_version != PROTOCOL_VERSION || response.id != Some(request.id) {
            return Err(ControlError::new(
                ErrorCode::IpcFailure,
                "Event response identity mismatch.",
            ));
        }
        match response.outcome {
            Outcome::Result(Reply::Heartbeat {}) => {}
            Outcome::Error(error) => return Err(error),
            Outcome::Result(Reply::Event { envelope })
                if envelope.protocol_version == PROTOCOL_VERSION
                    && envelope.session_id == session_id
                    && sequence.checked_add(1) == Some(envelope.sequence) =>
            {
                sequence = envelope.sequence;
                on_event(envelope)?;
            }
            _ => {
                return Err(ControlError::new(
                    ErrorCode::IpcFailure,
                    "Invalid or out-of-order event.",
                ))
            }
        }
    }
}
/// CLI build metadata is available without IPC or a running GUI.
pub fn format_version(json: bool) -> Result<String, serde_json::Error> {
    #[derive(serde::Serialize)]
    struct Version {
        application_version: &'static str,
        protocol_version: u32,
    }
    let version = Version {
        application_version: env!("CARGO_PKG_VERSION"),
        protocol_version: PROTOCOL_VERSION,
    };
    if json {
        serde_json::to_string(&version).map(|s| s + "\n")
    } else {
        Ok(format!(
            "qp {} (Control Protocol v{})\n",
            version.application_version, version.protocol_version
        ))
    }
}

pub fn format_event(envelope: &EventEnvelope, json: bool) -> Result<String, serde_json::Error> {
    if json {
        return serde_json::to_string(envelope).map(|s| s + "\n");
    }
    Ok(match &envelope.event {
        Event::Snapshot { state } => format!(
            "Snapshot: page {} / {}\n",
            state.page.unwrap_or(0),
            state.pages
        ),
        Event::Opened { state } => format!(
            "Opened: {}; pages: {}\n",
            state.document.as_deref().unwrap_or_default(),
            state.pages
        ),
        Event::Reloaded { state } => format!(
            "Reloaded: page {} / {}\n",
            state.page.unwrap_or(0),
            state.pages
        ),
        Event::Closed {} => "Presentation closed\n".into(),
        Event::PageChanged { page, pages } => format!("Page: {page} / {pages}\n"),
        Event::BlackoutChanged { value } => format!("Blackout: {value}\n"),
    })
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
        Reply::TimerElapsed { timer, .. } => format!("{}\n", timer.elapsed_seconds),
        Reply::Watching { .. } | Reply::Event { .. } | Reply::Heartbeat {} => {
            "Event transport\n".into()
        }
        Reply::Transfer { .. } => "Transport fragment\n".into(),
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
        Reply::Slide { content, pages, .. } => format!(
            "Page: {} / {}{}\n{}\n",
            content.page,
            pages,
            if content.truncated {
                " (text truncated)"
            } else {
                ""
            },
            content.text.join("\n")
        ),
        Reply::Context {
            presentation,
            current,
            next,
        } => {
            let describe = |s: &SlideContext| {
                format!(
                    "Page: {}{}\n{}\nNotes:\n{}\n",
                    s.slide.page,
                    if s.slide.truncated {
                        " (text truncated)"
                    } else {
                        ""
                    },
                    s.slide.text.join("\n"),
                    s.notes
                )
            };
            format!(
                "{}\nCurrent:\n{}Next:\n{}",
                presentation.document.as_deref().unwrap_or_default(),
                describe(current),
                next.as_ref()
                    .map(describe)
                    .unwrap_or_else(|| "End of presentation\n".into())
            )
        }
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
    fn export_cli_accepts_both_json_spellings_and_rejects_invalid_options() {
        for args in [
            &["export"][..],
            &["export", "--json"],
            &["export", "--JSON"],
            &["--JSON", "export"],
        ] {
            assert!(matches!(
                parse_args(args.iter().map(OsString::from)).unwrap(),
                CliRequest::Export
            ));
        }
        for args in [
            &["export", "--json", "--JSON"][..],
            &["export", "--full"],
            &["export", "file.pdf"],
            &["status", "--JSON"],
        ] {
            assert!(parse_args(args.iter().map(OsString::from)).is_err());
        }
    }

    fn export_status() -> Status {
        serde_json::from_str(r#"{"session_id":"test","document_revision":1,"document":"/deck.pdf","page":2,"pages":3,"fullscreen":true,"blackout":true,"timer":{"running":true,"elapsed_seconds":42},"opening":false,"render_state":"ready","notes_state":"ready"}"#).unwrap()
    }
    fn export_response(request: &Request) -> Response {
        let reply = match &request.command {
            Command::Status(_) => Reply::Status(export_status()),
            Command::ExportPage(params) => {
                assert_eq!(params.session_id, "test");
                assert_eq!(params.document_revision, 1);
                assert!(request.command.full_text());
                Reply::Context {
                    presentation: export_status(),
                    current: SlideContext {
                        slide: SlideText {
                            page: params.page,
                            text: vec![format!("Source {} 日本語\n", params.page)],
                            truncated: false,
                        },
                        notes: if params.page == 2 {
                            "Note 日本語\n".into()
                        } else {
                            String::new()
                        },
                    },
                    next: None,
                }
            }
            _ => panic!("export must only use read commands"),
        };
        Response::success(request.id, reply)
    }
    #[test]
    fn export_array_reuses_exact_context_schema_and_links_adjacent_pages() {
        let mut requests = Vec::new();
        let mut file = export(|request| {
            requests.push(request.clone());
            Ok(export_response(request))
        })
        .unwrap();
        let array: Vec<serde_json::Value> = serde_json::from_reader(&mut file).unwrap();
        assert_eq!(array.len(), 3);
        assert_eq!(requests.len(), 5);
        for (index, item) in array.iter().enumerate() {
            assert_eq!(item["protocol_version"], 1);
            assert_eq!(item["kind"], "context");
            assert_eq!(
                item["presentation"],
                serde_json::to_value(export_status()).unwrap()
            );
            assert_eq!(item["current"]["page"], index + 1);
            assert_eq!(item["current"]["truncated"], false);
            if index < 2 {
                assert_eq!(item["next"], array[index + 1]["current"]);
            } else {
                assert!(item["next"].is_null());
            }
        }
        assert_eq!(array[1]["current"]["notes"], "Note 日本語\n");
        assert_eq!(array[0]["current"]["text"][0], "Source 1 日本語\n");
    }
    #[test]
    fn export_discards_staged_output_on_failure_or_document_change() {
        for failure_id in [2, 3, 5] {
            let error = export(|request| {
                if request.id == failure_id {
                    return Ok(Response::error(
                        Some(request.id),
                        ErrorCode::Cancelled,
                        "Changed",
                    ));
                }
                Ok(export_response(request))
            })
            .unwrap_err();
            assert_eq!(error.code, ErrorCode::Cancelled);
        }
        for failure_id in [2, 5] {
            assert_eq!(
                export(|request| {
                    let mut response = export_response(request);
                    if request.id == failure_id {
                        match &mut response.outcome {
                            Outcome::Result(Reply::Status(status))
                            | Outcome::Result(Reply::Context {
                                presentation: status,
                                ..
                            }) => status.session_id = "replacement".into(),
                            _ => unreachable!(),
                        }
                    }
                    Ok(response)
                })
                .unwrap_err()
                .code,
                ErrorCode::Cancelled
            );
        }
        assert_eq!(
            export(|request| {
                let mut status = export_status();
                status.page = None;
                status.pages = 0;
                Ok(Response::success(request.id, Reply::Status(status)))
            })
            .unwrap_err()
            .code,
            ErrorCode::NoPresentation
        );
        assert_eq!(
            export(|request| {
                let mut response = export_response(request);
                if let Outcome::Result(Reply::Context { current, .. }) = &mut response.outcome {
                    current.slide.truncated = true;
                }
                Ok(response)
            })
            .unwrap_err()
            .code,
            ErrorCode::IpcFailure
        );
    }

    #[test]
    fn version_is_local_metadata_and_rejects_extra_arguments() {
        for name in ["--version", "-V"] {
            assert!(matches!(
                parse_args([name.into()]).unwrap(),
                CliRequest::Version { json: false }
            ));
            assert!(matches!(
                parse_args([name.into(), "--json".into()]).unwrap(),
                CliRequest::Version { json: true }
            ));
        }
        for args in [
            &["--version", "--full"][..],
            &["--version", "status"],
            &["--version", "--version"],
            &["--version", "--json", "--json"],
        ] {
            assert!(parse_args(args.iter().map(OsString::from)).is_err());
        }
        let json: serde_json::Value = serde_json::from_str(&format_version(true).unwrap()).unwrap();
        assert_eq!(json["application_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["protocol_version"], 1);
        assert!(format_version(false).unwrap().starts_with("qp "));
    }
    #[test]
    fn elapsed_and_watch_parse_without_exposing_timer_mutation() {
        assert_eq!(
            parse(&["timer", "elapsed", "--json"]),
            Command::TimerElapsed(Empty {})
        );
        assert_eq!(parse(&["watch", "--json"]), Command::Watch(Empty {}));
        for args in [
            &["timer", "start"][..],
            &["timer", "stop"],
            &["timer", "reset"],
            &["start"],
            &["stop"],
            &["reset"],
            &["timer", "elapsed", "--full"],
            &["watch", "--full"],
        ] {
            assert!(parse_args(args.iter().map(OsString::from)).is_err());
        }
        let reply = Reply::TimerElapsed {
            session_id: "session".into(),
            document_revision: 1,
            timer: TimerStatus {
                running: true,
                elapsed_seconds: 42,
            },
        };
        assert_eq!(format_reply(&reply, false).unwrap(), "42\n");
        let json: serde_json::Value =
            serde_json::from_str(&format_reply(&reply, true).unwrap()).unwrap();
        assert_eq!(json["elapsed_seconds"], 42);
        assert_eq!(json["running"], true);
    }
    #[test]
    fn watch_validates_identity_sequence_and_ignores_transport_heartbeats() {
        use std::io::Cursor;
        let state: Status = serde_json::from_str(r#"{"session_id":"test","document_revision":1,"document":null,"page":null,"pages":0,"fullscreen":false,"blackout":false,"timer":{"running":false,"elapsed_seconds":0},"opening":false,"render_state":"empty","notes_state":"empty"}"#).unwrap();
        let initial = Response::success(
            1,
            Reply::Watching {
                state: state.clone(),
                sequence: 7,
            },
        );
        let envelope = EventEnvelope {
            protocol_version: 1,
            session_id: "test".into(),
            document_revision: 2,
            sequence: 8,
            event: Event::Closed {},
        };
        let request = Request::new(1, Command::Watch(Empty {}));
        for mode in 0..5 {
            let mut event = envelope.clone();
            let mut response_id = 1;
            match mode {
                1 => event.sequence = 9,
                2 => event.session_id = "wrong".into(),
                3 => event.protocol_version = 2,
                4 => response_id = 2,
                _ => {}
            }
            let mut bytes = Vec::new();
            write_frame(&mut bytes, &initial).unwrap();
            write_frame(&mut bytes, &Response::success(1, Reply::Heartbeat {})).unwrap();
            write_frame(
                &mut bytes,
                &Response::success(response_id, Reply::Event { envelope: event }),
            )
            .unwrap();
            write_frame(
                &mut bytes,
                &Response::error(Some(1), ErrorCode::EventsLagged, "reconnect"),
            )
            .unwrap();
            let mut output = Vec::new();
            let error = receive_watch(
                &mut Cursor::new(bytes),
                &request,
                |event| {
                    output.push(event);
                    Ok(())
                },
                |_| {},
            )
            .unwrap_err();
            assert_eq!(output.len(), if mode == 0 { 2 } else { 1 });
            assert_eq!(
                error.code,
                if mode == 0 {
                    ErrorCode::EventsLagged
                } else {
                    ErrorCode::IpcFailure
                }
            );
        }
    }
    #[test]
    fn full_option_is_scoped_and_large_utf8_responses_reassemble() {
        assert_eq!(
            parse(&["slide", "--full", "--json"]),
            Command::Slide(ContentParams { full: true })
        );
        assert_eq!(
            parse(&["--full", "context"]),
            Command::Context(ContentParams { full: true })
        );
        for args in [
            &["status", "--full"][..],
            &["notes", "--full"],
            &["slide", "--full", "--full"],
            &["goto", "5", "--full"],
        ] {
            assert!(parse_args(args.iter().map(OsString::from)).is_err());
        }
        let source = "日本語\n".repeat(150_000);
        let response = Response::success(
            42,
            Reply::Slide {
                session_id: "session".into(),
                document_revision: 1,
                pages: 1,
                content: SlideText {
                    page: 1,
                    text: vec![source.clone()],
                    truncated: false,
                },
            },
        );
        let request = Request::new(42, Command::Slide(ContentParams { full: true }));
        let mut wire = Vec::new();
        write_response(&mut wire, &response).unwrap();
        let decoded = receive_response(&mut wire.as_slice(), &request).unwrap();
        assert_eq!(decoded, response);
        let mut frames = wire.as_slice();
        let mut count = 0;
        while !frames.is_empty() {
            let data = read_frame(&mut frames).unwrap();
            assert!(data.len() <= MAX_FRAME_BYTES);
            let frame: Response = serde_json::from_slice(&data).unwrap();
            assert!(
                matches!(frame.outcome, Outcome::Result(Reply::Transfer { chunk }) if chunk.valid())
            );
            count += 1;
        }
        assert!(count > 1);
        assert!(receive_response(
            &mut wire.as_slice(),
            &Request::new(42, Command::Slide(ContentParams::default()))
        )
        .is_err());
        let short = Response::success(
            42,
            Reply::Slide {
                session_id: "session".into(),
                document_revision: 1,
                pages: 1,
                content: SlideText {
                    page: 1,
                    text: vec!["prefix".into()],
                    truncated: true,
                },
            },
        );
        assert!(validate_response(&request, &serde_json::to_vec(&short).unwrap()).is_err());
    }
    #[test]
    fn response_transfers_reject_bad_sequences_limits_ids_and_truncation() {
        let request = Request::new(7, Command::Context(ContentParams { full: true }));
        let first = TransferChunk {
            sequence: 0,
            total_bytes: (MAX_FRAME_BYTES + 1) as u32,
            data: vec![b'x'; TRANSFER_CHUNK_BYTES],
        };
        let frame = |id, chunk| Response::success(id, Reply::Transfer { chunk });
        for bad in [
            TransferChunk {
                sequence: 0,
                ..first.clone()
            },
            TransferChunk {
                sequence: 1,
                total_bytes: u32::MAX,
                ..first.clone()
            },
            TransferChunk {
                sequence: 1,
                data: vec![],
                ..first.clone()
            },
        ] {
            let mut wire = Vec::new();
            write_frame(&mut wire, &frame(7, first.clone())).unwrap();
            write_frame(&mut wire, &frame(7, bad)).unwrap();
            assert_eq!(
                receive_response(&mut wire.as_slice(), &request)
                    .unwrap_err()
                    .code,
                ErrorCode::IpcFailure
            );
        }
        let mut wire = Vec::new();
        write_frame(&mut wire, &frame(7, first.clone())).unwrap();
        assert!(receive_response(&mut wire.as_slice(), &request).is_err());
        write_frame(
            &mut wire,
            &frame(
                8,
                TransferChunk {
                    sequence: 1,
                    ..first
                },
            ),
        )
        .unwrap();
        assert!(receive_response(&mut wire.as_slice(), &request).is_err());
    }
    #[test]
    fn context_output_and_response_validation_preserve_the_contract() {
        let reply = Reply::Slide {
            session_id: "test".into(),
            document_revision: 2,
            pages: 3,
            content: SlideText {
                page: 2,
                text: vec!["Original 日本語".into()],
                truncated: false,
            },
        };
        let response = Response::success(1, reply.clone());
        let bytes = serde_json::to_vec(&response).unwrap();
        assert!(validate_response(
            &Request::new(1, Command::Slide(ContentParams::default())),
            &bytes
        )
        .is_ok());
        assert_eq!(
            validate_response(
                &Request::new(1, Command::Context(ContentParams::default())),
                &bytes
            )
            .unwrap_err()
            .code,
            ErrorCode::IpcFailure
        );
        let json: serde_json::Value =
            serde_json::from_str(&format_reply(&reply, true).unwrap()).unwrap();
        assert_eq!(json["text"][0], "Original 日本語");
        assert_eq!(json["truncated"], false);
        assert_eq!(json["page"], 2);
        assert!(format_reply(&reply, false)
            .unwrap()
            .contains("Original 日本語"));
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
        for name in ["next", "prev", "close", "notes", "slide", "context"] {
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
            &["timer", "start"],
            &["timer", "stop"],
            &["timer", "reset"],
            &["start"],
            &["stop"],
            &["reset"],
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

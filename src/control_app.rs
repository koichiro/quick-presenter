//! Adapter that serializes control requests on the existing presentation event loop.
use crate::{
    app_state::AppState, control_state, input::PresentationCommand,
    render_scheduler::RenderSessionId, window_controller::AppWindowRefs,
};
use quick_presenter::control::{
    protocol::*,
    server::{ControlServer, PendingRequest},
};
use slint::{Timer, TimerMode};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

pub struct ControlRuntime {
    _timer: Timer,
    _server: ControlServer,
}
struct Dispatcher {
    receiver: Receiver<PendingRequest>,
    opening: Option<(RenderSessionId, u64, PendingRequest)>,
    queries: Vec<ContentQuery>,
}
struct ContentQuery {
    revision: u64,
    page: u32,
    pending: PendingRequest,
    retry_at: Instant,
}
impl ControlRuntime {
    pub fn install(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) -> std::io::Result<Self> {
        let (server, receiver) = ControlServer::start()?;
        Ok(Self::with_server(windows, state, server, receiver))
    }
    #[cfg(unix)]
    pub(crate) fn install_at(
        windows: AppWindowRefs,
        state: Rc<RefCell<AppState>>,
        path: &std::path::Path,
    ) -> std::io::Result<Self> {
        let (server, receiver) = ControlServer::bind(path)?;
        Ok(Self::with_server(windows, state, server, receiver))
    }
    fn with_server(
        windows: AppWindowRefs,
        state: Rc<RefCell<AppState>>,
        server: ControlServer,
        receiver: Receiver<PendingRequest>,
    ) -> Self {
        let mut dispatcher = Dispatcher {
            receiver,
            opening: None,
            queries: Vec::new(),
        };
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            dispatcher.poll(&windows, &state)
        });
        Self {
            _timer: timer,
            _server: server,
        }
    }
}
impl Dispatcher {
    fn poll(&mut self, windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
        if let Some((session, revision, pending)) = self.opening.take() {
            if pending.is_expired() {
                reply_error(
                    &pending,
                    ErrorCode::Timeout,
                    "PDF open timed out; query status before retrying.",
                );
            } else {
                match open_completion(&state.borrow(), session, revision) {
                    Some(Ok(result)) => reply(&pending, result),
                    Some(Err(error)) => reply_error(&pending, error.code, &error.message),
                    None => self.opening = Some((session, revision, pending)),
                }
            }
        }
        let mut waiting = Vec::new();
        for mut query in self.queries.drain(..) {
            if query.pending.is_expired() {
                reply_error(
                    &query.pending,
                    ErrorCode::Timeout,
                    "Slide text extraction timed out.",
                );
                continue;
            }
            let state = state.borrow();
            match content_reply(
                &state,
                &query.pending.request.command,
                query.revision,
                query.page,
            ) {
                Ok(Some(result)) => reply(&query.pending, result),
                Err(error) => reply_error(&query.pending, error.code, &error.message),
                Ok(None) => {
                    if Instant::now() >= query.retry_at {
                        request_content(&state, &query.pending.request.command, query.page);
                        query.retry_at = Instant::now() + Duration::from_millis(250);
                    }
                    waiting.push(query);
                }
            }
        }
        self.queries = waiting;
        for _ in 0..8 {
            let Ok(pending) = self.receiver.try_recv() else {
                break;
            };
            if pending.is_expired() {
                reply_error(
                    &pending,
                    ErrorCode::Timeout,
                    "Request expired before execution.",
                );
                continue;
            }
            let command = pending.request.command.clone();
            match command {
                Command::Status(_) => reply(
                    &pending,
                    Reply::Status(control_state::snapshot(&state.borrow(), Instant::now())),
                ),
                Command::Notes(_) => match notes_reply(&state.borrow()) {
                    Ok(result) => reply(&pending, result),
                    Err(error) => reply_error(&pending, error.code, &error.message),
                },
                Command::Slide(_) | Command::Context(_) => {
                    let state = state.borrow();
                    let revision = state.control.document_revision;
                    let page = state
                        .presentation
                        .snapshot()
                        .map(|p| p.current_number)
                        .unwrap_or(0);
                    match content_reply(&state, &command, revision, page) {
                        Ok(Some(result)) => reply(&pending, result),
                        Err(error) => reply_error(&pending, error.code, &error.message),
                        Ok(None) => {
                            request_content(&state, &command, page);
                            self.queries.push(ContentQuery {
                                revision,
                                page,
                                pending,
                                retry_at: Instant::now() + Duration::from_millis(250),
                            });
                        }
                    }
                }
                Command::Open(params) => {
                    if self.opening.is_some()
                        || state.borrow().pending_open.is_some()
                        || state.borrow().file_dialog.open
                    {
                        reply_error(
                            &pending,
                            ErrorCode::Busy,
                            "A PDF open or file dialog is already in progress.",
                        );
                        continue;
                    }
                    let revision = state.borrow().control.document_revision;
                    crate::begin_open_pdf(windows, state, params.file.into());
                    let session = state
                        .borrow()
                        .pending_open
                        .as_ref()
                        .map(|open| open.session_id);
                    if let Some(session) = session {
                        self.opening = Some((session, revision, pending));
                    } else {
                        reply_error(
                            &pending,
                            ErrorCode::Busy,
                            "Renderer is stopping. Try again shortly.",
                        );
                    }
                }
                Command::Close(_) => {
                    if let Some((_, _, opening)) = self.opening.take() {
                        reply_error(
                            &opening,
                            ErrorCode::Cancelled,
                            "PDF open was cancelled by close.",
                        );
                    }
                    let before = control_state::snapshot(&state.borrow(), Instant::now());
                    crate::handle_presentation_command(windows, state, PresentationCommand::Close);
                    reply(
                        &pending,
                        Reply::Mutation {
                            changed: before.page.is_some() || before.opening,
                            state: control_state::snapshot(&state.borrow(), Instant::now()),
                        },
                    );
                }
                command => {
                    let before = control_state::snapshot(&state.borrow(), Instant::now());
                    let validated = navigation_command(&state.borrow(), &command);
                    match validated {
                        Ok(command) => {
                            crate::handle_presentation_command(windows, state, command);
                            let after = control_state::snapshot(&state.borrow(), Instant::now());
                            let changed =
                                before.page != after.page || before.blackout != after.blackout;
                            reply(
                                &pending,
                                Reply::Mutation {
                                    changed,
                                    state: after,
                                },
                            );
                        }
                        Err(error) => reply_error(&pending, error.code, &error.message),
                    }
                }
            }
        }
    }
}
fn content_pages(state: &AppState, command: &Command, page: u32) -> Vec<u32> {
    let mut pages = vec![page - 1];
    if matches!(command, Command::Context(_))
        && state
            .presentation
            .snapshot()
            .is_some_and(|p| page < p.total_pages)
    {
        pages.push(page);
    }
    pages
}
fn request_content(state: &AppState, command: &Command, page: u32) {
    if let (Some(scheduler), Some(session)) = (
        &state.render_scheduler,
        state.render_sessions.current_session(),
    ) {
        for index in content_pages(state, command, page) {
            if !state.control.slide_text.contains_key(&index) {
                scheduler.extract_slide_text(session, index);
            }
        }
    }
}
/// Queries wait off the UI thread and only return a consistent owner-state snapshot.
fn content_reply(
    state: &AppState,
    command: &Command,
    revision: u64,
    page: u32,
) -> Result<Option<Reply>, ControlError> {
    if state.control.document_revision != revision
        || state
            .presentation
            .snapshot()
            .is_some_and(|p| p.current_number != page)
    {
        return Err(ControlError::new(
            ErrorCode::Cancelled,
            "Presentation changed during the query; request the context again.",
        ));
    }
    let snapshot = state.presentation.snapshot().ok_or_else(|| {
        ControlError::new(
            ErrorCode::NoPresentation,
            "No presentation is currently open.",
        )
    })?;
    if matches!(command, Command::Context(_)) {
        notes_reply(state)?;
    }
    let mut text = Vec::new();
    for index in content_pages(state, command, page) {
        match state.control.slide_text.get(&index) {
            Some(Ok(content)) => text.push(content.clone()),
            Some(Err(_)) => {
                return Err(ControlError::new(
                    ErrorCode::TextFailed,
                    "PDF text extraction failed.",
                ))
            }
            None if state.render_scheduler.as_ref().is_some_and(|scheduler| {
                scheduler.lifecycle() != crate::render_scheduler::RenderWorkerLifecycle::Running
            }) =>
            {
                return Err(ControlError::new(
                    ErrorCode::TextFailed,
                    "Renderer is unavailable for PDF text extraction.",
                ));
            }
            None => return Ok(None),
        }
    }
    let current = text.remove(0);
    if matches!(command, Command::Slide(_)) {
        return Ok(Some(Reply::Slide {
            session_id: state.control.session_id.clone(),
            document_revision: revision,
            pages: snapshot.total_pages,
            content: current,
        }));
    }
    let with_notes = |slide: SlideText| SlideContext {
        notes: state
            .notes
            .note_for_page_number(slide.page)
            .unwrap_or_default()
            .to_owned(),
        slide,
    };
    Ok(Some(Reply::Context {
        presentation: control_state::snapshot(state, Instant::now()),
        current: with_notes(current),
        next: text.into_iter().next().map(with_notes),
    }))
}

fn open_completion(
    state: &AppState,
    session: RenderSessionId,
    revision: u64,
) -> Option<Result<Reply, ControlError>> {
    if state.render_sessions.current_session() == Some(session) {
        Some(Ok(Reply::Mutation {
            changed: true,
            state: control_state::snapshot(state, Instant::now()),
        }))
    } else if state
        .pending_open
        .as_ref()
        .is_some_and(|pending| pending.session_id == session)
    {
        None
    } else if state.pending_open.is_some() || state.control.document_revision != revision {
        Some(Err(ControlError::new(
            ErrorCode::Cancelled,
            "PDF open was superseded by another presentation operation.",
        )))
    } else {
        Some(Err(ControlError::new(
            ErrorCode::OpenFailed,
            "Could not open the PDF. The previous deck is retained when available.",
        )))
    }
}

fn reply(pending: &PendingRequest, reply: Reply) {
    let _ = pending
        .response
        .try_send(Response::success(pending.request.id, reply));
}
fn reply_error(pending: &PendingRequest, code: ErrorCode, message: &str) {
    let _ = pending
        .response
        .try_send(Response::error(Some(pending.request.id), code, message));
}
fn navigation_command(
    state: &AppState,
    command: &Command,
) -> Result<PresentationCommand, ControlError> {
    let page = state.presentation.snapshot().ok_or_else(|| {
        ControlError::new(
            ErrorCode::NoPresentation,
            "No presentation is currently open.",
        )
    })?;
    Ok(match command {
        Command::Next(_) => PresentationCommand::NextPage,
        Command::Previous(_) => PresentationCommand::PreviousPage,
        Command::GoTo(params) if params.page > 0 && params.page <= page.total_pages => {
            PresentationCommand::JumpToPage(params.page - 1)
        }
        Command::GoTo(_) => {
            return Err(ControlError::new(
                ErrorCode::InvalidPage,
                "Page is outside the presentation.",
            ))
        }
        Command::Blackout(params) => PresentationCommand::SetBlackScreen(params.value),
        _ => {
            return Err(ControlError::new(
                ErrorCode::UnknownMethod,
                "Unsupported presentation command.",
            ))
        }
    })
}
fn notes_reply(state: &AppState) -> Result<Reply, ControlError> {
    let page = state.presentation.snapshot().ok_or_else(|| {
        ControlError::new(
            ErrorCode::NoPresentation,
            "No presentation is currently open.",
        )
    })?;
    match state.control.notes_state {
        NotesState::Ready => Ok(Reply::Notes {
            session_id: state.control.session_id.clone(),
            document_revision: state.control.document_revision,
            page: page.current_number,
            notes: state
                .notes
                .note_for_page_number(page.current_number)
                .unwrap_or_default()
                .to_owned(),
        }),
        NotesState::Failed => Err(ControlError::new(
            ErrorCode::NotesFailed,
            "Speaker-note extraction failed.",
        )),
        _ => Err(ControlError::new(
            ErrorCode::NotesLoading,
            "Speaker notes are not ready yet.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{presentation::PresentationState, session_controller::apply_session_command};
    #[test]
    fn stopped_workers_fail_missing_text_immediately_but_cached_source_remains_readable() {
        use crate::render_scheduler::{RenderScheduler, RenderWorkerLifecycle};
        for lifecycle in [
            RenderWorkerLifecycle::Failed,
            RenderWorkerLifecycle::Stopped,
            RenderWorkerLifecycle::ShutdownRequested,
        ] {
            let mut state = AppState {
                presentation: PresentationState::open_document("Deck", 1),
                render_scheduler: Some(RenderScheduler::without_worker_with_lifecycle_for_test(
                    lifecycle,
                )),
                ..AppState::default()
            };
            state.control.notes_state = NotesState::Ready;
            for command in [Command::Slide(Empty {}), Command::Context(Empty {})] {
                assert_eq!(
                    content_reply(&state, &command, 0, 1).unwrap_err().code,
                    ErrorCode::TextFailed
                );
            }
            state.control.slide_text.insert(
                0,
                Ok(SlideText {
                    page: 1,
                    text: vec!["Retained source".into()],
                    truncated: false,
                }),
            );
            assert!(matches!(
                content_reply(&state, &Command::Slide(Empty {}), 0, 1).unwrap(),
                Some(Reply::Slide { .. })
            ));
        }
    }
    #[test]
    fn context_is_consistent_handles_end_of_deck_and_preserves_source_notes() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 2),
            ..AppState::default()
        };
        state.control.notes_state = NotesState::Ready;
        let context = Command::Context(Empty {});
        assert!(content_reply(&state, &context, 0, 1).unwrap().is_none());
        for page in 1..=2 {
            state.control.slide_text.insert(
                page - 1,
                Ok(SlideText {
                    page,
                    text: vec![format!("Slide {page} 日本語")],
                    truncated: false,
                }),
            );
        }
        state.notes =
            crate::notes::SpeakerNotes::from_page_notes([(2, "Original note\n日本語".into())]);
        match content_reply(&state, &context, 0, 1).unwrap().unwrap() {
            Reply::Context {
                presentation,
                current,
                next,
            } => {
                assert_eq!(presentation.page, Some(1));
                assert!(current.notes.is_empty());
                assert_eq!(current.slide.text, ["Slide 1 日本語"]);
                assert_eq!(next.unwrap().notes, "Original note\n日本語");
            }
            _ => panic!("expected context"),
        }
        apply_session_command(&mut state, PresentationCommand::NextPage, Instant::now());
        assert!(matches!(
            content_reply(&state, &context, 0, 2).unwrap(),
            Some(Reply::Context { next: None, .. })
        ));
        assert_eq!(
            content_reply(&state, &context, 0, 1).unwrap_err().code,
            ErrorCode::Cancelled
        );
        assert_eq!(
            content_reply(&state, &context, 1, 2).unwrap_err().code,
            ErrorCode::Cancelled
        );
        state.control.notes_state = NotesState::Loading;
        assert_eq!(
            content_reply(&state, &context, 0, 2).unwrap_err().code,
            ErrorCode::NotesLoading
        );
        let slide = Command::Slide(Empty {});
        assert!(matches!(
            content_reply(&state, &slide, 0, 2).unwrap(),
            Some(Reply::Slide { .. })
        ));
        state.control.slide_text.insert(1, Err("failure".into()));
        assert_eq!(
            content_reply(&state, &slide, 0, 2).unwrap_err().code,
            ErrorCode::TextFailed
        );
        apply_session_command(&mut state, PresentationCommand::Close, Instant::now());
        assert!(state.control.slide_text.is_empty());
        assert_eq!(
            content_reply(&state, &slide, state.control.document_revision, 0)
                .unwrap_err()
                .code,
            ErrorCode::NoPresentation
        );
    }
    #[test]
    fn validated_navigation_uses_shared_state_transitions() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 5),
            ..AppState::default()
        };
        let now = Instant::now();
        for (command, page) in [
            (Command::Next(Empty {}), 2),
            (Command::Previous(Empty {}), 1),
            (Command::GoTo(PageParams { page: 5 }), 5),
            (Command::Next(Empty {}), 5),
        ] {
            let command = navigation_command(&state, &command).unwrap();
            apply_session_command(&mut state, command, now);
            assert_eq!(control_state::snapshot(&state, now).page, Some(page));
        }
        for page in [0, 6, u32::MAX] {
            assert_eq!(
                navigation_command(&state, &Command::GoTo(PageParams { page }))
                    .unwrap_err()
                    .code,
                ErrorCode::InvalidPage
            );
            assert_eq!(state.presentation.snapshot().unwrap().current_number, 5);
        }
        for value in [true, true, false, false] {
            let command =
                navigation_command(&state, &Command::Blackout(BlackoutParams { value })).unwrap();
            apply_session_command(&mut state, command, now);
            assert_eq!(state.black_screen.is_active(), value);
            assert_eq!(state.presentation.snapshot().unwrap().current_number, 5);
        }
    }
    #[test]
    fn notes_readiness_is_not_confused_with_no_notes() {
        let mut state = AppState::default();
        assert_eq!(
            notes_reply(&state).unwrap_err().code,
            ErrorCode::NoPresentation
        );
        assert_eq!(
            control_state::snapshot(&state, Instant::now()).render_state,
            RenderState::Empty
        );
        state.presentation = PresentationState::open_document("Deck", 2);
        state.control.notes_state = NotesState::Loading;
        assert_eq!(
            notes_reply(&state).unwrap_err().code,
            ErrorCode::NotesLoading
        );
        state.control.notes_state = NotesState::Failed;
        assert_eq!(
            notes_reply(&state).unwrap_err().code,
            ErrorCode::NotesFailed
        );
        state.control.notes_state = NotesState::Ready;
        assert!(
            matches!(notes_reply(&state).unwrap(), Reply::Notes { notes, .. } if notes.is_empty())
        );
        state.notes =
            crate::notes::SpeakerNotes::from_page_notes([(1, "Original source note".into())]);
        assert!(
            matches!(notes_reply(&state).unwrap(), Reply::Notes { notes, .. } if notes == "Original source note")
        );
    }
    #[cfg(unix)]
    #[test]
    fn ipc_navigation_changes_the_owner_state_without_gui_input() {
        use quick_presenter::control::{client, server::ControlServer};
        use std::{
            os::unix::fs::DirBuilderExt,
            sync::atomic::{AtomicU64, Ordering},
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "qp-domain-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let path = directory.join("control.sock");
        let (server, receiver) = ControlServer::bind(&path).unwrap();
        let client = std::thread::spawn(move || {
            client::send_to(
                &path,
                &Request::new(7, Command::GoTo(PageParams { page: 5 })),
            )
            .unwrap()
        });
        let pending = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 5),
            ..AppState::default()
        };
        let command = navigation_command(&state, &pending.request.command).unwrap();
        apply_session_command(&mut state, command, Instant::now());
        reply(
            &pending,
            Reply::Mutation {
                changed: true,
                state: control_state::snapshot(&state, Instant::now()),
            },
        );
        assert!(
            matches!(client.join().unwrap().outcome, Outcome::Result(Reply::Mutation { state, .. }) if state.page == Some(5))
        );
        assert_eq!(state.presentation.snapshot().unwrap().current_number, 5);
        drop(server);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn close_clears_the_deck_and_rejects_late_renderer_events() {
        use crate::session_controller::*;
        let mut state = AppState::default();
        let session = begin_open_pdf_state(&mut state, "/slides/demo.pdf".into());
        commit_render_opened_state(&mut state, session, "Deck".into(), 5, "Ready".into()).unwrap();
        state.notes = crate::notes::SpeakerNotes::from_page_notes([(1, "note".into())]);
        state.black_screen.set_active(true);
        state.timer.start(Instant::now());
        let revision = state.control.document_revision;
        apply_session_command(&mut state, PresentationCommand::Close, Instant::now());
        assert!(state.presentation.snapshot().is_none());
        assert!(state.active_document_path.is_none());
        assert!(!state.black_screen.is_active());
        assert!(!state.timer.is_running());
        assert_eq!(state.control.notes_state, NotesState::Empty);
        assert_eq!(state.control.document_revision, revision + 1);
        assert!(!commit_speaker_notes_loaded_state(
            &mut state,
            session,
            crate::notes::SpeakerNotes::empty(),
            "Ready".into()
        ));
        apply_session_command(&mut state, PresentationCommand::Close, Instant::now());
        assert_eq!(state.control.document_revision, revision + 1);
        let next_session = begin_open_pdf_state(&mut state, "/slides/next.pdf".into());
        assert_ne!(next_session, session);
    }
    #[test]
    fn open_completion_distinguishes_failure_from_supersession() {
        use crate::session_controller::*;
        let mut state = AppState::default();
        let session = begin_open_pdf_state(&mut state, "/slides/first.pdf".into());
        assert!(open_completion(&state, session, 0).is_none());
        commit_render_opened_state(&mut state, session, "First".into(), 5, "Ready".into()).unwrap();
        assert!(matches!(
            open_completion(&state, session, 0),
            Some(Ok(Reply::Mutation { .. }))
        ));
        let revision = state.control.document_revision;
        let failed = begin_open_pdf_state(&mut state, "/slides/missing.pdf".into());
        commit_render_open_failed_state(&mut state, failed, "Open failed".into());
        assert!(
            matches!(open_completion(&state, failed, revision), Some(Err(error)) if error.code == ErrorCode::OpenFailed)
        );
        let superseded = begin_open_pdf_state(&mut state, "/slides/superseded.pdf".into());
        let replacement = begin_open_pdf_state(&mut state, "/slides/replacement.pdf".into());
        assert!(
            matches!(open_completion(&state, superseded, revision), Some(Err(error)) if error.code == ErrorCode::Cancelled)
        );
        commit_render_opened_state(
            &mut state,
            replacement,
            "Replacement".into(),
            2,
            "Ready".into(),
        )
        .unwrap();
        assert!(
            matches!(open_completion(&state, superseded, revision), Some(Err(error)) if error.code == ErrorCode::Cancelled)
        );
    }
}

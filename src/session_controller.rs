use std::{path::PathBuf, time::Instant};

use crate::{
    app_state::{AppState, PendingOpenState, ThumbnailState},
    input::{apply_presentation_command, PresentationCommand},
    notes::SpeakerNotes,
    presentation::{PageSnapshot, PresentationState},
    render_scheduler::RenderSessionId,
    rendering::{CacheContext, RenderPurpose, RenderRequest, RenderedPage},
    timer::{timer_transition_for_page_change, TimerTransition},
};

pub const SLOW_OPEN_STATUS_TEXT: &str = "Still opening PDF. The current deck remains available.";

pub struct SessionCommandOutcome {
    pub snapshot: Option<PageSnapshot>,
    pub slide_fullscreen: Option<bool>,
}

pub fn apply_session_command(
    state: &mut AppState,
    command: PresentationCommand,
    now: Instant,
) -> SessionCommandOutcome {
    if command == PresentationCommand::ExitSlideFullscreen {
        return SessionCommandOutcome {
            snapshot: None,
            slide_fullscreen: Some(state.fullscreen.exit_slide_fullscreen()),
        };
    }

    if command == PresentationCommand::ToggleBlackScreen {
        state.black_screen.toggle();
        return SessionCommandOutcome {
            snapshot: state.presentation.snapshot(),
            slide_fullscreen: None,
        };
    }

    let before = state.presentation.snapshot();
    apply_presentation_command(&mut state.presentation, command);
    let after = state.presentation.snapshot();
    update_elapsed_timer_for_page_change(before.as_ref(), after.as_ref(), state, now);
    let snapshot = (before != after).then_some(after).flatten();

    SessionCommandOutcome {
        snapshot,
        slide_fullscreen: None,
    }
}

pub fn begin_open_pdf_state(state: &mut AppState, path: PathBuf) -> RenderSessionId {
    begin_open_pdf_state_at(state, path, Instant::now())
}

pub fn begin_open_pdf_state_at(
    state: &mut AppState,
    path: PathBuf,
    requested_at: Instant,
) -> RenderSessionId {
    let session_id = state.render_sessions.begin_open_session();
    state.pending_open = Some(PendingOpenState {
        session_id,
        path,
        requested_at,
        slow_status_shown: false,
    });
    state.status_text = "Opening PDF...".to_owned();
    session_id
}

pub fn pending_open_session_id(state: &AppState) -> Option<RenderSessionId> {
    state
        .pending_open
        .as_ref()
        .map(|pending| pending.session_id)
}

pub fn mark_pending_open_slow(
    state: &mut AppState,
    session_id: RenderSessionId,
    now: Instant,
    delay: std::time::Duration,
) -> bool {
    if !state.render_sessions.accepts_pending_open(session_id) {
        return false;
    }

    let Some(pending_open) = state.pending_open.as_mut() else {
        return false;
    };
    if pending_open.session_id != session_id || pending_open.slow_status_shown {
        return false;
    }
    if now
        .checked_duration_since(pending_open.requested_at)
        .unwrap_or_default()
        < delay
    {
        return false;
    }

    pending_open.slow_status_shown = true;
    state.status_text = SLOW_OPEN_STATUS_TEXT.to_owned();
    true
}

pub fn commit_render_opened_state(
    state: &mut AppState,
    session_id: RenderSessionId,
    title: String,
    page_count: u32,
    status_text: String,
) -> Option<OpenedSessionOutcome> {
    if !state.render_sessions.commit_pending_open(session_id) {
        return None;
    }

    state.render_generation = state.render_generation.wrapping_add(1);
    state.audience_slide.last_good_current = None;
    state.audience_slide.failed_current_page = None;
    state.render_cache.clear();
    state.notes = SpeakerNotes::empty();
    state.presentation = PresentationState::open_document(title, page_count);
    state.thumbnails = ThumbnailState {
        total_pages: page_count,
    };
    state.black_screen.set_active(false);
    state.timer.reset();
    state.status_text = status_text;
    let loaded_path = state.pending_open.take().map(|pending| pending.path);

    Some(OpenedSessionOutcome {
        snapshot: state.presentation.snapshot(),
        loaded_path,
    })
}

pub struct OpenedSessionOutcome {
    pub snapshot: Option<PageSnapshot>,
    pub loaded_path: Option<PathBuf>,
}

pub fn commit_render_reloaded_state(
    state: &mut AppState,
    session_id: RenderSessionId,
    title: String,
    page_count: u32,
    current_page_index: u32,
    current_page: RenderedPage,
    presentation_cache_radius: u32,
) -> Option<PageSnapshot> {
    if page_count == 0 {
        return None;
    }
    if !state.render_sessions.commit_pending_reload(session_id) {
        return None;
    }

    state.render_generation = state.render_generation.wrapping_add(1);
    state.render_cache.clear();
    state.notes = SpeakerNotes::empty();
    state.presentation = PresentationState::open_document_at(title, page_count, current_page_index);
    let snapshot = state.presentation.snapshot()?;
    let current_page_index = snapshot.current_index;
    state.thumbnails = ThumbnailState {
        total_pages: page_count,
    };
    state.audience_slide.last_good_current = Some(current_page.clone());
    state.audience_slide.failed_current_page = None;
    state.render_cache.insert_with_context(
        RenderRequest {
            page_index: current_page_index,
            width: crate::render_controller::CURRENT_RENDER_WIDTH,
            purpose: RenderPurpose::CurrentSlide,
        },
        current_page,
        Some(CacheContext {
            current_index: current_page_index,
            total_pages: page_count,
            presentation_radius: presentation_cache_radius,
        }),
    );
    state.status_text = "PDF reloaded.".to_owned();
    Some(snapshot)
}

pub fn clear_render_reload_state(state: &mut AppState, session_id: RenderSessionId) -> bool {
    state.render_sessions.clear_pending_reload(session_id)
}

pub fn commit_render_open_failed_state(state: &mut AppState, session_id: RenderSessionId) -> bool {
    if !state.render_sessions.clear_pending_open(session_id) {
        return false;
    }

    state.status_text = "Could not open PDF. Choose another file.".to_owned();
    state.pending_open = None;
    true
}

pub fn commit_speaker_notes_loaded_state(
    state: &mut AppState,
    session_id: RenderSessionId,
    notes: SpeakerNotes,
    status_text: String,
) -> bool {
    if !state.render_sessions.accepts(session_id) {
        return false;
    }

    state.notes = notes;
    state.status_text = status_text;
    true
}

pub fn commit_page_rendered_state(
    state: &mut AppState,
    session_id: RenderSessionId,
    request: RenderRequest,
    page: RenderedPage,
    presentation_cache_radius: u32,
) -> Option<PageRenderedOutcome> {
    if !state.render_sessions.accepts(session_id) {
        return None;
    }

    let snapshot = state.presentation.snapshot();
    let cache_context = snapshot.as_ref().map(|snapshot| CacheContext {
        current_index: snapshot.current_index,
        total_pages: snapshot.total_pages,
        presentation_radius: presentation_cache_radius,
    });
    state
        .render_cache
        .insert_with_context(request, page.clone(), cache_context);
    if let Some(snapshot) = snapshot.as_ref() {
        state.render_cache.retain_presentation_window(
            snapshot.current_index,
            snapshot.total_pages,
            presentation_cache_radius,
        );
    }

    let is_visible_current_slide = snapshot.as_ref().filter(|snapshot| {
        request.purpose == RenderPurpose::CurrentSlide
            && request.page_index == snapshot.current_index
    });

    let initial_fit_aspect_ratio = is_visible_current_slide.and_then(|_| {
        state
            .audience_slide
            .last_good_current
            .is_none()
            .then_some(page.aspect_ratio)
    });
    if is_visible_current_slide.is_some() {
        state.audience_slide.last_good_current = Some(page);
        state.audience_slide.failed_current_page = None;
    }

    Some(PageRenderedOutcome {
        snapshot,
        initial_fit_aspect_ratio,
    })
}

pub struct PageRenderedOutcome {
    pub snapshot: Option<PageSnapshot>,
    pub initial_fit_aspect_ratio: Option<f32>,
}

pub fn commit_page_render_failed_state(
    state: &mut AppState,
    session_id: RenderSessionId,
    request: RenderRequest,
) -> bool {
    if !state.render_sessions.accepts(session_id) {
        return false;
    }

    let is_visible_current_slide = state.presentation.snapshot().is_some_and(|snapshot| {
        request.purpose == RenderPurpose::CurrentSlide
            && request.page_index == snapshot.current_index
    });

    if is_visible_current_slide {
        state.audience_slide.failed_current_page = Some(request.page_index);
        state.status_text = "Could not render this page. Try another PDF or page.".to_owned();
        true
    } else {
        false
    }
}

pub fn commit_render_worker_failed_state(
    state: &mut AppState,
    session_id: Option<RenderSessionId>,
) -> bool {
    if !state.render_sessions.mark_worker_failed(session_id) {
        return false;
    }

    if let Some(snapshot) = state.presentation.snapshot() {
        state.audience_slide.failed_current_page = Some(snapshot.current_index);
    }
    state.status_text = "Rendering stopped. Open the PDF again.".to_owned();
    state.pending_open = None;
    true
}

fn update_elapsed_timer_for_page_change(
    before: Option<&PageSnapshot>,
    after: Option<&PageSnapshot>,
    state: &mut AppState,
    now: Instant,
) {
    match timer_transition_for_page_change(
        before.map(|snapshot| snapshot.current_index),
        after.map(|snapshot| snapshot.current_index),
    ) {
        TimerTransition::Start => state.timer.start(now),
        TimerTransition::Reset => state.timer.reset(),
        TimerTransition::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::RenderedPage;
    use std::time::Duration;

    fn rendered_page(aspect_ratio: f32) -> RenderedPage {
        RenderedPage {
            image: crate::view_sync::placeholder_slide().image,
            aspect_ratio,
            estimated_bytes: 64,
        }
    }

    fn current_slide_request(page_index: u32) -> RenderRequest {
        RenderRequest {
            page_index,
            width: crate::render_controller::CURRENT_RENDER_WIDTH,
            purpose: RenderPurpose::CurrentSlide,
        }
    }

    fn preview_request(page_index: u32) -> RenderRequest {
        RenderRequest {
            page_index,
            width: crate::render_controller::PREVIEW_RENDER_WIDTH,
            purpose: RenderPurpose::NextPreview,
        }
    }

    fn pending_open_path(state: &AppState) -> Option<PathBuf> {
        state
            .pending_open
            .as_ref()
            .map(|pending_open| pending_open.path.clone())
    }

    #[test]
    fn reloaded_document_preserves_page_and_presentation_runtime_state() {
        let started_at = Instant::now();
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 5),
            notes: SpeakerNotes::from_page_notes([(4, "Old note".to_owned())]),
            ..AppState::default()
        };
        state.presentation.jump_to_page_index(3);
        state.black_screen.set_active(true);
        state.fullscreen.set_slide_fullscreen(true);
        state.timer.start(started_at);
        let current = state.render_sessions.begin_open_session();
        assert!(state.render_sessions.commit_pending_open(current));
        let reload = state.render_sessions.begin_reload_session();

        let snapshot = commit_render_reloaded_state(
            &mut state,
            reload,
            "Deck".to_owned(),
            6,
            3,
            rendered_page(4.0 / 3.0),
            2,
        )
        .unwrap();

        assert_eq!(snapshot.current_index, 3);
        assert!(state.black_screen.is_active());
        assert!(state.fullscreen.is_slide_fullscreen());
        assert!(state.timer.is_running());
        assert!(state.notes.is_empty());
        assert_eq!(state.status_text, "PDF reloaded.");
        assert!(state.render_cache.peek(current_slide_request(3)).is_some());
        assert!(state.audience_slide.last_good_current.is_some());
    }

    #[test]
    fn reloaded_document_clamps_page_and_rejects_stale_session() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 5),
            ..AppState::default()
        };
        state.presentation.jump_to_page_index(4);
        let current = state.render_sessions.begin_open_session();
        assert!(state.render_sessions.commit_pending_open(current));
        let stale = state.render_sessions.begin_reload_session();
        let latest = state.render_sessions.begin_reload_session();

        assert!(commit_render_reloaded_state(
            &mut state,
            stale,
            "Stale".to_owned(),
            2,
            1,
            rendered_page(1.0),
            2,
        )
        .is_none());
        let snapshot = commit_render_reloaded_state(
            &mut state,
            latest,
            "Deck".to_owned(),
            2,
            99,
            rendered_page(1.0),
            2,
        )
        .unwrap();

        assert_eq!(snapshot.current_index, 1);
        assert_eq!(snapshot.total_pages, 2);
    }

    #[test]
    fn begin_open_pdf_state_tracks_pending_open_without_clearing_current_deck() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Existing deck", 3),
            status_text: "Ready".to_owned(),
            render_generation: 7,
            ..AppState::default()
        };
        state.presentation.next_page();
        state.black_screen.set_active(true);
        let original_snapshot = state.presentation.snapshot();

        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));

        assert_eq!(state.render_sessions.current_session(), None);
        assert!(state.render_sessions.accepts_pending_open(session_id));
        assert_eq!(state.render_generation, 7);
        assert_eq!(state.presentation.snapshot(), original_snapshot);
        assert!(state.black_screen.is_active());
        assert_eq!(state.status_text, "Opening PDF...");
        assert_eq!(pending_open_path(&state), Some(PathBuf::from("deck.pdf")));
        let pending_open = state
            .pending_open
            .as_ref()
            .expect("pending open state should be stored");
        assert_eq!(pending_open.session_id, session_id);
        assert!(!pending_open.slow_status_shown);
    }

    #[test]
    fn slow_open_status_waits_until_delay_elapses() {
        let mut state = AppState::default();
        let requested_at = Instant::now();
        let session_id =
            begin_open_pdf_state_at(&mut state, PathBuf::from("deck.pdf"), requested_at);

        assert!(!mark_pending_open_slow(
            &mut state,
            session_id,
            requested_at + Duration::from_millis(999),
            Duration::from_secs(1),
        ));

        assert_eq!(state.status_text, "Opening PDF...");
        assert_eq!(
            state
                .pending_open
                .as_ref()
                .map(|pending_open| pending_open.slow_status_shown),
            Some(false)
        );
    }

    #[test]
    fn slow_open_status_is_marked_once_after_delay() {
        let mut state = AppState::default();
        let requested_at = Instant::now();
        let session_id =
            begin_open_pdf_state_at(&mut state, PathBuf::from("deck.pdf"), requested_at);

        assert!(mark_pending_open_slow(
            &mut state,
            session_id,
            requested_at + Duration::from_secs(1),
            Duration::from_secs(1),
        ));
        assert_eq!(state.status_text, SLOW_OPEN_STATUS_TEXT);
        assert_eq!(
            state
                .pending_open
                .as_ref()
                .map(|pending_open| pending_open.slow_status_shown),
            Some(true)
        );

        assert!(!mark_pending_open_slow(
            &mut state,
            session_id,
            requested_at + Duration::from_secs(2),
            Duration::from_secs(1),
        ));
    }

    #[test]
    fn repeated_open_resets_slow_open_status_tracking() {
        let mut state = AppState::default();
        let requested_at = Instant::now();
        let first_session =
            begin_open_pdf_state_at(&mut state, PathBuf::from("old.pdf"), requested_at);
        assert!(mark_pending_open_slow(
            &mut state,
            first_session,
            requested_at + Duration::from_secs(2),
            Duration::from_secs(1),
        ));

        let second_requested_at = requested_at + Duration::from_secs(3);
        let second_session =
            begin_open_pdf_state_at(&mut state, PathBuf::from("new.pdf"), second_requested_at);

        assert_eq!(pending_open_path(&state), Some(PathBuf::from("new.pdf")));
        assert_eq!(state.status_text, "Opening PDF...");
        assert!(!mark_pending_open_slow(
            &mut state,
            first_session,
            second_requested_at + Duration::from_secs(2),
            Duration::from_secs(1),
        ));
        assert!(!mark_pending_open_slow(
            &mut state,
            second_session,
            second_requested_at + Duration::from_millis(999),
            Duration::from_secs(1),
        ));
        assert!(mark_pending_open_slow(
            &mut state,
            second_session,
            second_requested_at + Duration::from_secs(1),
            Duration::from_secs(1),
        ));
        assert_eq!(state.status_text, SLOW_OPEN_STATUS_TEXT);
    }

    #[test]
    fn black_screen_command_toggles_without_changing_page() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 2),
            ..AppState::default()
        };

        let outcome = apply_session_command(
            &mut state,
            PresentationCommand::ToggleBlackScreen,
            Instant::now(),
        );

        assert!(state.black_screen.is_active());
        assert_eq!(outcome.snapshot.unwrap().current_index, 0);
        assert_eq!(outcome.slide_fullscreen, None);
    }

    #[test]
    fn page_command_updates_snapshot_without_requiring_windows() {
        let now = Instant::now();
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 2),
            ..AppState::default()
        };

        let outcome = apply_session_command(&mut state, PresentationCommand::NextPage, now);

        assert_eq!(outcome.snapshot.unwrap().current_index, 1);
        assert_eq!(outcome.slide_fullscreen, None);
        assert!(state.timer.is_running());
    }

    #[test]
    fn next_page_at_last_page_returns_no_snapshot_refresh() {
        let now = Instant::now();
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 2),
            ..AppState::default()
        };
        state.presentation.next_page();

        let outcome = apply_session_command(&mut state, PresentationCommand::NextPage, now);

        assert_eq!(state.presentation.snapshot().unwrap().current_index, 1);
        assert_eq!(outcome.snapshot, None);
        assert_eq!(outcome.slide_fullscreen, None);
    }

    #[test]
    fn previous_page_at_first_page_returns_no_snapshot_refresh() {
        let now = Instant::now();
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 2),
            ..AppState::default()
        };

        let outcome = apply_session_command(&mut state, PresentationCommand::PreviousPage, now);

        assert_eq!(state.presentation.snapshot().unwrap().current_index, 0);
        assert_eq!(outcome.snapshot, None);
        assert_eq!(outcome.slide_fullscreen, None);
    }

    #[test]
    fn fullscreen_command_returns_window_effect_without_changing_page() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 2),
            ..AppState::default()
        };
        state.fullscreen.set_slide_fullscreen(true);

        let outcome = apply_session_command(
            &mut state,
            PresentationCommand::ExitSlideFullscreen,
            Instant::now(),
        );

        assert_eq!(outcome.snapshot, None);
        assert_eq!(outcome.slide_fullscreen, Some(false));
        assert_eq!(state.presentation.snapshot().unwrap().current_index, 0);
    }

    #[test]
    fn stale_opened_event_does_not_replace_current_session() {
        let mut state = AppState::default();
        let stale_session = begin_open_pdf_state(&mut state, PathBuf::from("old.pdf"));
        let current_session = begin_open_pdf_state(&mut state, PathBuf::from("new.pdf"));

        let outcome = commit_render_opened_state(
            &mut state,
            stale_session,
            "Old".to_owned(),
            3,
            "Ready".to_owned(),
        );

        assert!(outcome.is_none());
        assert_eq!(state.render_sessions.current_session(), None);
        assert!(state.render_sessions.accepts_pending_open(current_session));
        assert_eq!(state.presentation.snapshot(), None);
        assert_eq!(pending_open_path(&state), Some(PathBuf::from("new.pdf")));
    }

    #[test]
    fn opened_event_commits_new_presentation_and_loaded_path() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        state.notes = SpeakerNotes::from_page_notes([(1, "stale".to_owned())]);

        let outcome = commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            3,
            "Ready".to_owned(),
        )
        .expect("current session should accept opened event");

        let snapshot = outcome
            .snapshot
            .expect("opened document should have a first page");
        assert_eq!(snapshot.title, "Deck");
        assert_eq!(snapshot.page_label, "1 / 3");
        assert_eq!(state.thumbnails.total_pages, 3);
        assert_eq!(state.status_text, "Ready");
        assert!(state.notes.is_empty());
        assert_eq!(state.render_sessions.current_session(), Some(session_id));
        assert_eq!(outcome.loaded_path, Some(PathBuf::from("deck.pdf")));
        assert_eq!(state.pending_open, None);
    }

    #[test]
    fn speaker_notes_loaded_event_commits_for_current_session() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );
        let notes = SpeakerNotes::from_page_notes([(2, "Presenter note".to_owned())]);

        assert!(commit_speaker_notes_loaded_state(
            &mut state,
            session_id,
            notes,
            "Ready".to_owned(),
        ));

        assert_eq!(state.notes.note_for_page_number(2), Some("Presenter note"));
        assert_eq!(state.status_text, "Ready");
    }

    #[test]
    fn stale_speaker_notes_loaded_event_is_ignored() {
        let mut state = AppState::default();
        let stale_session = begin_open_pdf_state(&mut state, PathBuf::from("old.pdf"));
        begin_open_pdf_state(&mut state, PathBuf::from("new.pdf"));

        assert!(!commit_speaker_notes_loaded_state(
            &mut state,
            stale_session,
            SpeakerNotes::from_page_notes([(1, "Old note".to_owned())]),
            "Ready".to_owned(),
        ));

        assert!(state.notes.is_empty());
        assert_eq!(state.status_text, "Opening PDF...");
    }

    #[test]
    fn open_failed_event_clears_pending_path_for_current_session() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("broken.pdf"));

        assert!(commit_render_open_failed_state(&mut state, session_id));

        assert_eq!(state.pending_open, None);
        assert_eq!(
            state.status_text,
            "Could not open PDF. Choose another file."
        );
    }

    #[test]
    fn failed_replacement_open_preserves_current_presentation() {
        let mut state = AppState::default();
        let current_session = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            current_session,
            "Existing deck".to_owned(),
            3,
            "Ready".to_owned(),
        );
        state.presentation.next_page();
        state.notes = SpeakerNotes::from_page_notes([(2, "Keep this note".to_owned())]);
        state.black_screen.set_active(true);
        let original_snapshot = state.presentation.snapshot();

        let pending_session = begin_open_pdf_state(&mut state, PathBuf::from("broken.pdf"));

        assert!(commit_render_open_failed_state(&mut state, pending_session));

        assert_eq!(
            state.render_sessions.current_session(),
            Some(current_session)
        );
        assert_eq!(state.presentation.snapshot(), original_snapshot);
        assert_eq!(state.notes.note_for_page_number(2), Some("Keep this note"));
        assert!(state.black_screen.is_active());
        assert_eq!(state.pending_open, None);
        assert_eq!(
            state.status_text,
            "Could not open PDF. Choose another file."
        );
    }

    #[test]
    fn stale_open_failed_event_is_ignored() {
        let mut state = AppState::default();
        let stale_session = begin_open_pdf_state(&mut state, PathBuf::from("old.pdf"));
        begin_open_pdf_state(&mut state, PathBuf::from("new.pdf"));

        assert!(!commit_render_open_failed_state(&mut state, stale_session));

        assert_eq!(pending_open_path(&state), Some(PathBuf::from("new.pdf")));
        assert_eq!(state.status_text, "Opening PDF...");
    }

    #[test]
    fn first_current_page_render_requests_initial_slide_window_fit() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );
        let request = current_slide_request(0);

        let outcome = commit_page_rendered_state(
            &mut state,
            session_id,
            request,
            rendered_page(4.0 / 3.0),
            2,
        )
        .expect("current session should accept rendered page");

        assert_eq!(outcome.snapshot.unwrap().current_index, 0);
        assert_eq!(outcome.initial_fit_aspect_ratio, Some(4.0 / 3.0));
        assert!(state.render_cache.peek(request).is_some());
    }

    #[test]
    fn subsequent_current_page_render_does_not_request_slide_window_fit() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );
        commit_page_rendered_state(
            &mut state,
            session_id,
            current_slide_request(0),
            rendered_page(4.0 / 3.0),
            2,
        )
        .expect("first current page render should be accepted");
        state.presentation.next_page();
        let request = current_slide_request(1);

        let outcome = commit_page_rendered_state(
            &mut state,
            session_id,
            request,
            rendered_page(16.0 / 9.0),
            2,
        )
        .expect("current session should accept subsequent current page render");

        assert_eq!(outcome.snapshot.unwrap().current_index, 1);
        assert_eq!(outcome.initial_fit_aspect_ratio, None);
    }

    #[test]
    fn preview_render_does_not_request_initial_slide_window_fit() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );

        let outcome = commit_page_rendered_state(
            &mut state,
            session_id,
            preview_request(1),
            rendered_page(16.0 / 9.0),
            2,
        )
        .expect("current session should accept preview render");

        assert_eq!(outcome.snapshot.unwrap().current_index, 0);
        assert_eq!(outcome.initial_fit_aspect_ratio, None);
    }

    #[test]
    fn visible_current_page_render_updates_audience_last_good_slide() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );

        commit_page_rendered_state(
            &mut state,
            session_id,
            current_slide_request(0),
            rendered_page(4.0 / 3.0),
            2,
        )
        .expect("current session should accept rendered page");

        assert_eq!(
            state
                .audience_slide
                .last_good_current
                .as_ref()
                .map(|page| page.aspect_ratio),
            Some(4.0 / 3.0)
        );
        assert_eq!(state.audience_slide.failed_current_page, None);
    }

    #[test]
    fn non_visible_current_render_does_not_replace_audience_last_good_slide() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );
        state.audience_slide.last_good_current = Some(rendered_page(16.0 / 9.0));

        commit_page_rendered_state(
            &mut state,
            session_id,
            current_slide_request(1),
            rendered_page(4.0 / 3.0),
            2,
        )
        .expect("current session should accept rendered page");

        assert_eq!(
            state
                .audience_slide
                .last_good_current
                .as_ref()
                .map(|page| page.aspect_ratio),
            Some(16.0 / 9.0)
        );
    }

    #[test]
    fn current_page_render_failure_keeps_audience_last_good_slide() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );
        state.audience_slide.last_good_current = Some(rendered_page(16.0 / 9.0));

        assert!(commit_page_render_failed_state(
            &mut state,
            session_id,
            current_slide_request(0),
        ));

        assert_eq!(
            state
                .audience_slide
                .last_good_current
                .as_ref()
                .map(|page| page.aspect_ratio),
            Some(16.0 / 9.0)
        );
        assert_eq!(state.audience_slide.failed_current_page, Some(0));
    }

    #[test]
    fn stale_page_render_is_ignored() {
        let mut state = AppState::default();
        let stale_session = begin_open_pdf_state(&mut state, PathBuf::from("old.pdf"));
        begin_open_pdf_state(&mut state, PathBuf::from("new.pdf"));

        let outcome = commit_page_rendered_state(
            &mut state,
            stale_session,
            current_slide_request(0),
            rendered_page(1.0),
            2,
        );

        assert!(outcome.is_none());
    }

    #[test]
    fn current_page_render_failure_updates_status() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );

        assert!(commit_page_render_failed_state(
            &mut state,
            session_id,
            current_slide_request(0),
        ));

        assert_eq!(
            state.status_text,
            "Could not render this page. Try another PDF or page."
        );
    }

    #[test]
    fn non_visible_current_page_render_failure_does_not_replace_status() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );

        assert!(!commit_page_render_failed_state(
            &mut state,
            session_id,
            current_slide_request(1),
        ));

        assert_eq!(state.status_text, "Ready");
        assert_eq!(state.audience_slide.failed_current_page, None);
    }

    #[test]
    fn preview_render_failure_does_not_replace_status() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));

        assert!(!commit_page_render_failed_state(
            &mut state,
            session_id,
            preview_request(1),
        ));

        assert_eq!(state.status_text, "Opening PDF...");
    }

    #[test]
    fn render_worker_failure_updates_current_session_status() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );

        assert!(commit_render_worker_failed_state(
            &mut state,
            Some(session_id)
        ));

        assert_eq!(state.status_text, "Rendering stopped. Open the PDF again.");
        assert_eq!(state.pending_open, None);
        assert_eq!(state.render_sessions.current_session(), None);
        assert_eq!(state.audience_slide.failed_current_page, Some(0));
    }

    #[test]
    fn render_worker_failure_clears_pending_open_for_identified_session() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));

        assert!(commit_render_worker_failed_state(
            &mut state,
            Some(session_id)
        ));

        assert_eq!(state.status_text, "Rendering stopped. Open the PDF again.");
        assert_eq!(state.pending_open, None);
        assert_eq!(state.render_sessions.current_session(), None);
        assert_eq!(state.audience_slide.failed_current_page, None);
    }

    #[test]
    fn stale_render_worker_failure_is_ignored() {
        let mut state = AppState::default();
        let stale_session = begin_open_pdf_state(&mut state, PathBuf::from("old.pdf"));
        begin_open_pdf_state(&mut state, PathBuf::from("new.pdf"));

        assert!(!commit_render_worker_failed_state(
            &mut state,
            Some(stale_session)
        ));

        assert_eq!(state.status_text, "Opening PDF...");
        assert_eq!(pending_open_path(&state), Some(PathBuf::from("new.pdf")));
    }

    #[test]
    fn unidentified_render_worker_failure_clears_pending_open_and_current_session() {
        let mut state = AppState::default();
        let current_session = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            current_session,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );
        let pending_session = begin_open_pdf_state(&mut state, PathBuf::from("new.pdf"));

        assert!(commit_render_worker_failed_state(&mut state, None));

        assert_eq!(state.render_sessions.current_session(), None);
        assert!(!state.render_sessions.accepts_pending_open(pending_session));
        assert_eq!(state.pending_open, None);
        assert_eq!(state.status_text, "Rendering stopped. Open the PDF again.");
        assert_eq!(state.audience_slide.failed_current_page, Some(0));
    }
}

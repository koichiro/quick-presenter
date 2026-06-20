use std::{path::PathBuf, time::Instant};

use crate::{
    app_state::{AppState, ThumbnailState},
    input::{apply_presentation_command, PresentationCommand},
    notes::SpeakerNotes,
    presentation::{PageSnapshot, PresentationState},
    render_scheduler::RenderSessionId,
    rendering::{CacheContext, RenderPurpose, RenderRequest, RenderedPage},
    timer::{timer_transition_for_page_change, TimerTransition},
};

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

    SessionCommandOutcome {
        snapshot: after,
        slide_fullscreen: None,
    }
}

pub fn begin_open_pdf_state(state: &mut AppState, path: PathBuf) -> RenderSessionId {
    let session_id = state.render_sessions.begin_session();
    state.render_generation = state.render_generation.wrapping_add(1);
    state.audience_slide.last_good_current = None;
    state.audience_slide.failed_current_page = None;
    state.pdf = None;
    state.pending_open_path = Some(path);
    state.render_cache.clear();
    state.thumbnails = ThumbnailState::default();
    state.notes = SpeakerNotes::empty();
    state.presentation = PresentationState::empty();
    state.black_screen.set_active(false);
    state.timer.reset();
    state.status_text = "Opening PDF...".to_owned();
    session_id
}

pub fn commit_render_opened_state(
    state: &mut AppState,
    session_id: RenderSessionId,
    title: String,
    page_count: u32,
    status_text: String,
) -> Option<OpenedSessionOutcome> {
    if !state.render_sessions.accepts(session_id) {
        return None;
    }

    state.render_generation = state.render_generation.wrapping_add(1);
    state.audience_slide.last_good_current = None;
    state.audience_slide.failed_current_page = None;
    state.render_cache.clear();
    state.presentation = PresentationState::open_document(title, page_count);
    state.thumbnails = ThumbnailState {
        total_pages: page_count,
    };
    state.black_screen.set_active(false);
    state.timer.reset();
    state.status_text = status_text;

    Some(OpenedSessionOutcome {
        snapshot: state.presentation.snapshot(),
        loaded_path: state.pending_open_path.take(),
    })
}

pub struct OpenedSessionOutcome {
    pub snapshot: Option<PageSnapshot>,
    pub loaded_path: Option<PathBuf>,
}

pub fn commit_render_open_failed_state(state: &mut AppState, session_id: RenderSessionId) -> bool {
    if !state.render_sessions.accepts(session_id) {
        return false;
    }

    state.status_text = "Could not open PDF. Choose another file.".to_owned();
    state.pending_open_path = None;
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

    let fit_aspect_ratio = is_visible_current_slide.map(|_| page.aspect_ratio);
    if is_visible_current_slide.is_some() {
        state.audience_slide.last_good_current = Some(page);
        state.audience_slide.failed_current_page = None;
    }

    Some(PageRenderedOutcome {
        snapshot,
        fit_aspect_ratio,
    })
}

pub struct PageRenderedOutcome {
    pub snapshot: Option<PageSnapshot>,
    pub fit_aspect_ratio: Option<f32>,
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

    #[test]
    fn begin_open_pdf_state_resets_current_session_metadata() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Existing deck", 3),
            status_text: "Ready".to_owned(),
            render_generation: 7,
            ..AppState::default()
        };
        state.presentation.next_page();
        state.black_screen.set_active(true);

        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));

        assert_eq!(state.render_sessions.current_session(), Some(session_id));
        assert_eq!(state.render_generation, 8);
        assert_eq!(state.presentation.snapshot(), None);
        assert!(!state.black_screen.is_active());
        assert_eq!(state.status_text, "Opening PDF...");
        assert_eq!(state.pending_open_path, Some(PathBuf::from("deck.pdf")));
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
        assert_eq!(
            state.render_sessions.current_session(),
            Some(current_session)
        );
        assert_eq!(state.presentation.snapshot(), None);
        assert_eq!(state.pending_open_path, Some(PathBuf::from("new.pdf")));
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
        assert_eq!(state.notes.note_for_page_number(1), Some("stale"));
        assert_eq!(outcome.loaded_path, Some(PathBuf::from("deck.pdf")));
        assert_eq!(state.pending_open_path, None);
    }

    #[test]
    fn speaker_notes_loaded_event_commits_for_current_session() {
        let mut state = AppState::default();
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
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

        assert_eq!(state.pending_open_path, None);
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

        assert_eq!(state.pending_open_path, Some(PathBuf::from("new.pdf")));
        assert_eq!(state.status_text, "Opening PDF...");
    }

    #[test]
    fn current_page_render_returns_snapshot_and_fit_aspect_ratio() {
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
        assert_eq!(outcome.fit_aspect_ratio, Some(4.0 / 3.0));
        assert!(state.render_cache.peek(request).is_some());
    }

    #[test]
    fn preview_render_does_not_request_slide_window_fit() {
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
        assert_eq!(outcome.fit_aspect_ratio, None);
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
}

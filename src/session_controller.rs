use std::{path::PathBuf, time::Instant};

use crate::{
    app_state::{AppState, ThumbnailState},
    input::{apply_presentation_command, PresentationCommand},
    notes::SpeakerNotes,
    presentation::{PageSnapshot, PresentationState},
    render_scheduler::RenderSessionId,
    rendering::{RenderPurpose, RenderRequest, RenderedPage},
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
    notes: SpeakerNotes,
    status_text: String,
) -> Option<OpenedSessionOutcome> {
    if !state.render_sessions.accepts(session_id) {
        return None;
    }

    state.render_generation = state.render_generation.wrapping_add(1);
    state.render_cache.clear();
    state.presentation = PresentationState::open_document(title, page_count);
    state.notes = notes;
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

    state.render_cache.insert(request, page.clone());

    let snapshot = state.presentation.snapshot();
    if let Some(snapshot) = snapshot.as_ref() {
        state.render_cache.retain_presentation_window(
            snapshot.current_index,
            snapshot.total_pages,
            presentation_cache_radius,
        );
    }

    let fit_aspect_ratio = snapshot
        .as_ref()
        .filter(|snapshot| {
            request.purpose == RenderPurpose::CurrentSlide
                && request.page_index == snapshot.current_index
        })
        .map(|_| page.aspect_ratio);

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

    if request.purpose == RenderPurpose::CurrentSlide {
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
            SpeakerNotes::empty(),
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
}

//! Public snapshots are derived from the GUI-owned state; the IPC server owns no deck.
use crate::app_state::AppState;
use quick_presenter::control::protocol::{NotesState, RenderState, Status, TimerStatus};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct ControlMetadata {
    pub session_id: String,
    pub document_revision: u64,
    pub notes_state: NotesState,
    pub slide_text: std::collections::BTreeMap<
        u32,
        Result<quick_presenter::control::protocol::SlideText, String>,
    >,
}
impl Default for ControlMetadata {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self {
            session_id: format!(
                "{:x}-{time:x}-{:x}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ),
            document_revision: 0,
            notes_state: NotesState::Empty,
            slide_text: Default::default(),
        }
    }
}
pub fn snapshot(state: &AppState, now: Instant) -> Status {
    let page = state.presentation.snapshot();
    let render_state = match page.as_ref() {
        None => RenderState::Empty,
        Some(page) if state.audience_slide.failed_current_page == Some(page.current_index) => {
            RenderState::Failed
        }
        Some(page)
            if state
                .render_cache
                .peek(state.current_slide_request(page.current_index))
                .is_some() =>
        {
            RenderState::Ready
        }
        Some(_) => RenderState::Rendering,
    };
    Status {
        session_id: state.control.session_id.clone(),
        document_revision: state.control.document_revision,
        document: state
            .active_document_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        page: page.as_ref().map(|page| page.current_number),
        pages: page.map(|page| page.total_pages).unwrap_or(0),
        fullscreen: state.fullscreen.is_slide_fullscreen(),
        blackout: state.black_screen.is_active(),
        timer: TimerStatus {
            running: state.timer.is_running(),
            elapsed_seconds: state.timer.elapsed_at(now).as_secs(),
        },
        opening: state.pending_open.is_some(),
        render_state,
        notes_state: state.control.notes_state,
    }
}

/// Ignore results from replaced/closed decks. Keep at most 32 small source-text entries.
pub fn commit_slide_text(
    state: &mut AppState,
    session: crate::render_scheduler::RenderSessionId,
    page_index: u32,
    result: Result<quick_presenter::control::protocol::SlideText, String>,
) -> bool {
    if state.render_sessions.current_session() != Some(session)
        || !state
            .presentation
            .snapshot()
            .is_some_and(|p| page_index < p.total_pages)
    {
        return false;
    }
    if !state.control.slide_text.contains_key(&page_index) && state.control.slide_text.len() >= 32 {
        // Keep current/next entries together even when revisiting an earlier page.
        let current = state
            .presentation
            .snapshot()
            .map(|p| p.current_index)
            .unwrap_or(page_index);
        if let Some(evicted) = state
            .control
            .slide_text
            .keys()
            .copied()
            .max_by_key(|index| index.abs_diff(current))
        {
            state.control.slide_text.remove(&evicted);
        }
    }
    state.control.slide_text.insert(page_index, result);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{render_scheduler::RenderSessionId, session_controller::*};
    use quick_presenter::control::protocol::SlideText;
    #[test]
    fn text_cache_is_bounded_and_ignores_results_after_replace_reload_or_close() {
        let mut state = AppState::default();
        let session = begin_open_pdf_state(&mut state, "/slides/first.pdf".into());
        commit_render_opened_state(&mut state, session, "First".into(), 40, "Ready".into())
            .unwrap();
        let content = |page| {
            Ok(SlideText {
                page,
                text: vec!["text".into()],
                truncated: false,
            })
        };
        for index in 0..40 {
            assert!(commit_slide_text(
                &mut state,
                session,
                index,
                content(index + 1)
            ));
        }
        assert_eq!(state.control.slide_text.len(), 32);
        assert!(commit_slide_text(&mut state, session, 0, content(1)));
        assert!(commit_slide_text(&mut state, session, 1, content(2)));
        assert!(state.control.slide_text.contains_key(&0));
        assert!(state.control.slide_text.contains_key(&1));
        assert!(!commit_slide_text(
            &mut state,
            RenderSessionId(session.0 + 1),
            0,
            content(1)
        ));
        assert!(!commit_slide_text(&mut state, session, 40, content(41)));
        let replacement = begin_open_pdf_state(&mut state, "/slides/second.pdf".into());
        commit_render_opened_state(&mut state, replacement, "Second".into(), 1, "Ready".into())
            .unwrap();
        assert!(state.control.slide_text.is_empty());
        assert!(!commit_slide_text(&mut state, session, 0, content(1)));
        assert!(commit_slide_text(&mut state, replacement, 0, content(1)));
        let reload = state.render_sessions.begin_reload_session();
        let image =
            slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(1, 1));
        commit_render_reloaded_state(
            &mut state,
            reload,
            "Second".into(),
            1,
            0,
            crate::rendering::RenderedPage {
                image,
                aspect_ratio: 1.0,
                estimated_bytes: 4,
            },
            1,
            0,
        )
        .unwrap();
        assert!(state.control.slide_text.is_empty());
        assert!(!commit_slide_text(&mut state, replacement, 0, content(1)));
        assert!(commit_slide_text(&mut state, reload, 0, content(1)));
        apply_session_command(
            &mut state,
            crate::input::PresentationCommand::Close,
            Instant::now(),
        );
        assert!(state.control.slide_text.is_empty());
        assert!(!commit_slide_text(&mut state, reload, 0, content(1)));
    }
}

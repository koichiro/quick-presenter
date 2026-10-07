//! Public snapshots are derived from the GUI-owned state; the IPC server owns no deck.
use crate::app_state::AppState;
use quick_presenter::control::protocol::{NotesState, RenderState, Status, TimerStatus};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct ControlMetadata {
    pub session_id: String,
    pub document_revision: u64,
    pub notes_state: NotesState,
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

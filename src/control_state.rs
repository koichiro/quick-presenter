//! Public snapshots are derived from the GUI-owned state; the IPC server owns no deck.
use crate::app_state::AppState;
use quick_presenter::control::protocol::{NotesState, RenderState, Status, TimerStatus};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct ControlMetadata {
    pub events: quick_presenter::control::events::EventHub,
    pub session_id: String,
    pub document_revision: u64,
    pub notes_state: NotesState,
    pub full_slide_text: std::collections::BTreeMap<
        u32,
        Result<quick_presenter::control::protocol::SlideText, String>,
    >,
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
            events: Default::default(),
            session_id: format!(
                "{:x}-{time:x}-{:x}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ),
            document_revision: 0,
            notes_state: NotesState::Empty,
            slide_text: Default::default(),
            full_slide_text: Default::default(),
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

/// Called at committed domain transitions, not by sampling the UI on a timer.
pub fn publish_changes(state: &mut AppState, before: Status, now: Instant) {
    publish_changes_inner(state, before, now, false);
}
pub fn publish_reload(state: &mut AppState, before: Status, now: Instant) {
    publish_changes_inner(state, before, now, true);
}
fn publish_changes_inner(state: &mut AppState, before: Status, now: Instant, reloaded: bool) {
    use quick_presenter::control::protocol::Event;
    let after = snapshot(state, now);
    if before.document_revision != after.document_revision {
        let event = if after.page.is_none() {
            Event::Closed {}
        } else if reloaded {
            Event::Reloaded {
                state: after.clone(),
            }
        } else {
            Event::Opened {
                state: after.clone(),
            }
        };
        state.control.events.publish(&after, event);
    } else if before.page != after.page {
        if let Some(page) = after.page {
            state.control.events.publish(
                &after,
                Event::PageChanged {
                    page,
                    pages: after.pages,
                },
            );
        }
    }
    if before.blackout != after.blackout {
        state.control.events.publish(
            &after,
            Event::BlackoutChanged {
                value: after.blackout,
            },
        );
    }
}

/// Ignore results from replaced/closed decks. Keep at most 32 small source-text entries.
pub fn commit_slide_text_mode(
    state: &mut AppState,
    session: crate::render_scheduler::RenderSessionId,
    page_index: u32,
    full: bool,
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
    let current = state
        .presentation
        .snapshot()
        .map(|p| p.current_index)
        .unwrap_or(page_index);
    let cache = if full {
        &mut state.control.full_slide_text
    } else {
        &mut state.control.slide_text
    };
    let capacity = if full { 2 } else { 32 };
    if !cache.contains_key(&page_index) && cache.len() >= capacity {
        if let Some(evicted) = cache
            .keys()
            .copied()
            .max_by_key(|index| index.abs_diff(current))
        {
            cache.remove(&evicted);
        }
    }
    cache.insert(page_index, result);
    true
}

impl ControlMetadata {
    pub fn text_cache(
        &self,
        full: bool,
    ) -> &std::collections::BTreeMap<
        u32,
        Result<quick_presenter::control::protocol::SlideText, String>,
    > {
        if full {
            &self.full_slide_text
        } else {
            &self.slide_text
        }
    }
}
#[cfg(test)]
fn commit_slide_text(
    state: &mut AppState,
    session: crate::render_scheduler::RenderSessionId,
    page_index: u32,
    result: Result<quick_presenter::control::protocol::SlideText, String>,
) -> bool {
    commit_slide_text_mode(state, session, page_index, false, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{render_scheduler::RenderSessionId, session_controller::*};
    use quick_presenter::control::protocol::SlideText;
    #[test]
    fn timer_changes_do_not_publish_control_events() {
        use quick_presenter::control::events;
        let mut state = AppState::default();
        let (subscriber, receiver) = events::channel();
        assert!(state.control.events.subscribe(subscriber));
        let now = Instant::now();
        let before = snapshot(&state, now);
        state.timer.start(now);
        publish_changes(&mut state, before, now);
        let before = snapshot(&state, now + std::time::Duration::from_secs(10));
        state.timer.reset();
        publish_changes(&mut state, before, now + std::time::Duration::from_secs(10));
        assert!(receiver.events.try_recv().is_err());
        assert_eq!(state.control.events.sequence(), 0);
    }
    #[test]
    fn every_committed_domain_change_is_ordered_including_gui_actions() {
        use crate::{input::PresentationCommand, rendering::RenderedPage, view_sync};
        use quick_presenter::control::{events, protocol::Event};
        let mut state = AppState::default();
        let (subscriber, receiver) = events::channel();
        assert!(state.control.events.subscribe(subscriber));
        let session = begin_open_pdf_state(&mut state, "/slides/deck.pdf".into());
        commit_render_opened_state(&mut state, session, "Deck".into(), 3, "Ready".into()).unwrap();
        let now = Instant::now();
        apply_session_command(&mut state, PresentationCommand::NextPage, now);
        apply_session_command(&mut state, PresentationCommand::NextPage, now);
        apply_session_command(&mut state, PresentationCommand::NextPage, now); // Boundary no-op.
        apply_session_command(&mut state, PresentationCommand::SetBlackScreen(true), now);
        apply_session_command(&mut state, PresentationCommand::SetBlackScreen(true), now); // Idempotent.
        apply_session_command(&mut state, PresentationCommand::FirstPage, now);
        let session = begin_open_pdf_state(&mut state, "/slides/deck.pdf".into());
        commit_render_opened_state(&mut state, session, "Deck".into(), 3, "Ready".into()).unwrap();
        let reload = state.render_sessions.begin_reload_session();
        commit_render_reloaded_state(
            &mut state,
            reload,
            "Deck".into(),
            3,
            0,
            RenderedPage {
                image: view_sync::placeholder_slide().image,
                aspect_ratio: 1.0,
                estimated_bytes: 4,
            },
            640,
            2,
        )
        .unwrap();
        close_presentation_state(&mut state);
        close_presentation_state(&mut state); // Idempotent.
        let events: Vec<_> = receiver.events.try_iter().collect();
        let kinds: Vec<_> = events
            .iter()
            .map(|e| match e.event {
                Event::Opened { .. } => "opened",
                Event::Reloaded { .. } => "reloaded",
                Event::Closed {} => "closed",
                Event::PageChanged { .. } => "page",
                Event::BlackoutChanged { .. } => "blackout",
                _ => "unexpected",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "opened", "page", "page", "blackout", "page", "opened", "blackout", "reloaded",
                "closed"
            ]
        );
        for (index, event) in events.iter().enumerate() {
            assert_eq!(event.sequence, index as u64 + 1);
        }
        assert_eq!(
            events.last().unwrap().document_revision,
            state.control.document_revision
        );
    }
    #[test]
    fn full_cache_is_separate_bounded_and_invalidated_with_document_lifecycle() {
        let mut state = AppState::default();
        let session = begin_open_pdf_state(&mut state, "/slides/full.pdf".into());
        commit_render_opened_state(&mut state, session, "Full".into(), 5, "Ready".into()).unwrap();
        for index in [4, 3, 0, 1] {
            let content = SlideText {
                page: index + 1,
                text: vec!["x".repeat(8_000)],
                truncated: false,
            };
            assert!(commit_slide_text_mode(
                &mut state,
                session,
                index,
                true,
                Ok(content)
            ));
        }
        assert_eq!(state.control.full_slide_text.len(), 2);
        assert!(
            state.control.full_slide_text.contains_key(&0)
                && state.control.full_slide_text.contains_key(&1)
        );
        assert!(state.control.slide_text.is_empty());
        apply_session_command(
            &mut state,
            crate::input::PresentationCommand::Close,
            Instant::now(),
        );
        assert!(state.control.full_slide_text.is_empty());
        assert!(!commit_slide_text_mode(
            &mut state,
            session,
            0,
            true,
            Err("late result".into())
        ));
    }
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

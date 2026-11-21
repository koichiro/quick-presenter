use std::{path::PathBuf, time::Instant};

#[cfg(target_os = "linux")]
use std::sync::mpsc::Receiver;

use crate::{
    black_screen::BlackScreenState,
    fullscreen::FullscreenState,
    hot_reload::{HotReloadState, PdfWatcher, WatcherRecoveryState},
    notes::SpeakerNotes,
    presentation::PresentationState,
    recent::RecentFileStore,
    recent::RecentFiles,
    render_scheduler::RenderScheduler,
    render_scheduler::RenderSessionId,
    render_scheduler::RenderSessionTracker,
    rendering::{RenderCache, RenderedPage},
    timer::PresentationTimer,
    window_menu::WindowMenuState,
};

#[derive(Default)]
pub struct AppState {
    pub audience_join_visible: bool,
    pub audience_join_available: bool,
    pub control: crate::control_state::ControlMetadata,
    pub render_sizing: crate::render_sizing::RenderSizingPolicy,
    pub page_aspects: std::collections::HashMap<u32, f32>,
    pub audience_slide: AudienceSlideState,
    pub black_screen: BlackScreenState,
    pub fullscreen: FullscreenState,
    pub hot_reload: HotReloadState,
    pub pdf_watcher: Option<PdfWatcher>,
    pub watcher_recovery: WatcherRecoveryState,
    pub active_document_path: Option<PathBuf>,
    pub automatic_reopen: Option<RenderSessionId>,
    pub render_cache: RenderCache,
    pub render_generation: u64,
    pub render_sessions: RenderSessionTracker,
    pub render_scheduler: Option<RenderScheduler>,
    pub thumbnails: ThumbnailState,
    pub pending_open: Option<PendingOpenState>,
    pub notes: SpeakerNotes,
    pub presentation: PresentationState,
    pub timer: PresentationTimer,
    pub window_menu: WindowMenuState,
    pub recent_files: RecentFiles,
    pub recent_menu_paths: Vec<PathBuf>,
    pub recent_store: Option<RecentFileStore>,
    pub diagnostics_log_path: Option<PathBuf>,
    pub file_dialog: FileDialogState,
    pub status_text: String,
}

impl AppState {
    pub fn update_render_sizing(
        &mut self,
        policy: crate::render_sizing::RenderSizingPolicy,
        radius: u32,
    ) {
        let old_request = self
            .presentation
            .snapshot()
            .map(|snapshot| self.current_slide_request(snapshot.current_index));
        if let Some(snapshot) = self.presentation.snapshot() {
            if let Some(page) = self
                .render_cache
                .peek(self.current_slide_request(snapshot.current_index))
            {
                self.audience_slide.last_good_current = Some(page);
            }
        }
        self.render_sizing = policy;
        self.render_generation = self.render_generation.wrapping_add(1);
        if old_request.is_some_and(|old| old != self.current_slide_request(old.page_index)) {
            if self.audience_slide.failed_current_page.take().is_some() {
                self.status_text = "Ready".to_owned();
            }
        }
        let context = self.cache_context(radius);
        self.render_cache
            .update_budget(policy.cache_budget(), context);
    }
    pub fn current_slide_request(&self, page_index: u32) -> crate::rendering::RenderRequest {
        crate::rendering::RenderRequest {
            page_index,
            width: self
                .render_sizing
                .page_width(self.page_aspects.get(&page_index).copied()),
            purpose: crate::rendering::RenderPurpose::CurrentSlide,
        }
    }

    pub fn cache_context(&self, radius: u32) -> Option<crate::rendering::CacheContext> {
        self.presentation
            .snapshot()
            .map(|snapshot| crate::rendering::CacheContext {
                current_index: snapshot.current_index,
                total_pages: snapshot.total_pages,
                presentation_radius: radius,
                current_render_width: self.render_sizing.current_width(),
                visible_render_width: self.current_slide_request(snapshot.current_index).width,
            })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PendingOpenState {
    pub session_id: RenderSessionId,
    pub path: PathBuf,
    pub requested_at: Instant,
    pub slow_status_shown: bool,
}

#[derive(Default)]
pub struct AudienceSlideState {
    pub last_good_current: Option<RenderedPage>,
    pub failed_current_page: Option<u32>,
}

#[derive(Default)]
pub struct ThumbnailState {
    pub total_pages: u32,
}

#[derive(Default)]
pub struct FileDialogState {
    pub open: bool,
    #[cfg(target_os = "linux")]
    pub result_receiver: Option<Receiver<Option<PathBuf>>>,
}

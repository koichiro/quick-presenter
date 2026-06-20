use std::path::PathBuf;

use crate::{
    black_screen::BlackScreenState, fullscreen::FullscreenState, notes::SpeakerNotes,
    pdf::PdfDocumentState, presentation::PresentationState, recent::RecentFileStore,
    recent::RecentFiles, render_scheduler::RenderScheduler, render_scheduler::RenderSessionTracker,
    rendering::RenderCache, timer::PresentationTimer, window_menu::WindowMenuState,
};

#[derive(Default)]
pub struct AppState {
    pub black_screen: BlackScreenState,
    pub fullscreen: FullscreenState,
    pub pdf: Option<PdfDocumentState>,
    pub render_cache: RenderCache,
    pub render_generation: u64,
    pub render_sessions: RenderSessionTracker,
    pub render_scheduler: Option<RenderScheduler>,
    pub thumbnails: ThumbnailState,
    pub pending_open_path: Option<PathBuf>,
    pub notes: SpeakerNotes,
    pub presentation: PresentationState,
    pub timer: PresentationTimer,
    pub window_menu: WindowMenuState,
    pub recent_files: RecentFiles,
    pub recent_store: Option<RecentFileStore>,
    pub status_text: String,
}

#[derive(Default)]
pub struct ThumbnailState {
    pub total_pages: u32,
}

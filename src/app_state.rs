use std::path::PathBuf;

use crate::{
    black_screen::BlackScreenState,
    fullscreen::FullscreenState,
    notes::SpeakerNotes,
    presentation::PresentationState,
    recent::RecentFileStore,
    recent::RecentFiles,
    render_scheduler::RenderScheduler,
    render_scheduler::RenderSessionTracker,
    rendering::{RenderCache, RenderedPage},
    timer::PresentationTimer,
    window_menu::WindowMenuState,
};

#[derive(Default)]
pub struct AppState {
    pub audience_slide: AudienceSlideState,
    pub black_screen: BlackScreenState,
    pub fullscreen: FullscreenState,
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
    pub recent_menu_paths: Vec<PathBuf>,
    pub recent_store: Option<RecentFileStore>,
    pub status_text: String,
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

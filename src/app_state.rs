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

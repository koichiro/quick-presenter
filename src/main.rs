#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

pub mod app_metadata;
pub mod app_state;
pub mod aspect;
pub mod black_screen;
pub mod cli;
pub mod clock;
mod control_app;
#[cfg(unix)]
mod control_smoke;
mod control_state;
pub mod diagnostics;
pub mod errors;
pub mod fullscreen;
pub mod gui_smoke;
pub mod hot_reload;
pub mod input;
#[cfg(target_os = "macos")]
pub mod macos_renderer;
#[cfg(target_os = "macos")]
pub mod macos_window;
pub mod notes;
pub mod pdf;
pub mod presentation;
pub mod recent;
pub mod render_controller;
pub mod render_scheduler;
pub mod render_sizing;
pub mod renderer_helper;
pub mod renderer_limits;
pub mod renderer_process;
pub mod renderer_protocol;
pub mod renderer_resources;
#[cfg(target_os = "linux")]
pub mod renderer_sandbox_linux;
pub mod renderer_supervision;
pub mod rendering;
pub mod session_controller;
pub mod settings_storage;
pub mod startup_state;
pub mod timer;
pub mod view_sync;
pub mod window_controller;
pub mod window_menu;
pub mod window_placement;
#[cfg(target_os = "windows")]
pub mod windows_window;

use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
use std::sync::mpsc;

use anyhow::Result;
use app_metadata::about_metadata;
use app_state::AppState;
#[cfg(test)]
use app_state::ThumbnailState;
use cli::{help_text, parse_startup_options, GuiSmokeOptions, StartupRequest};
use clock::current_clock_label;
use diagnostics::init_diagnostics;
use errors::PresenterMessage;
use hot_reload::{
    HotReloadPhase, PdfWatcher, PreparationOutcome, WatchSignal, WatchTarget,
    HOT_RELOAD_EVENT_POLL_INTERVAL,
};
use input::PresentationCommand;
use notes::SpeakerNotes;
use pdf::pdfium_runtime_version_label;
#[cfg(test)]
use pdf::PdfDocumentState;
use presentation::PageSnapshot;
#[cfg(test)]
use presentation::PresentationState;
use recent::{default_recent_file_store, RecentFileStore, RecentFiles, RecentMenuSnapshot};
use render_controller::CURRENT_RENDER_WIDTH;
use render_controller::{
    enqueue_render_plan_if_missing, presentation_preload_render_plan, thumbnail_render_plan,
    thumbnail_visible_range_render_plan, visible_page_render_plan,
};
#[cfg(test)]
use render_controller::{PREVIEW_RENDER_WIDTH, THUMBNAIL_RENDER_WIDTH};
use render_scheduler::{RenderEvent, RenderScheduler, RenderWorkerLifecycle};
#[cfg(test)]
use rendering::{presentation_preload_order, thumbnail_window_indices, RenderCache};
use rendering::{RenderPurpose, RenderRequest, RenderedPage};
use session_controller::{
    apply_session_command, begin_open_pdf_state, clear_render_reload_state,
    commit_page_render_failed_state, commit_page_rendered_state, commit_render_open_failed_state,
    commit_render_opened_state, commit_render_reloaded_state, commit_render_worker_failed_state,
    commit_speaker_notes_loaded_state, mark_pending_open_slow, pending_open_session_id,
    SLOW_OPEN_STATUS_TEXT,
};
use slint::{CloseRequestResponse, ComponentHandle, Model, Timer, TimerMode, Weak};
use timer::PresentationTimer;
use tracing::warn;
use view_sync::{
    apply_opening_state_to_windows, apply_snapshot_to_windows, set_presenter_message,
    thumbnail_current_row_index, thumbnail_model,
};
#[cfg(test)]
use view_sync::{black_slide_image, presenter_status_text};
use window_controller::{
    apply_macos_slide_window_chrome, fitted_slide_window_size, hide_slide_window,
    request_display_swap, restore_presenter_input_after_transient_ui, set_slide_fullscreen,
    show_presenter_window, show_slide_window, slide_titlebar_compensation_height,
    start_slide_chrome_sync, sync_slide_chrome, AppWindowRefs, AppWindows, DisplaySwapController,
    DisplaySwapOutcome, WindowRole,
};

slint::include_modules!();

const PRESENTATION_CACHE_RADIUS: u32 = 2;
const THUMBNAIL_CACHE_RADIUS: u32 = 8;
const THUMBNAIL_SCROLL_LOOKAHEAD: u32 = 4;
const RENDER_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(16);
const PENDING_OPEN_STATUS_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(target_os = "linux")]
const FILE_DIALOG_RESULT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const SLOW_OPEN_STATUS_DELAY: Duration = Duration::from_secs(2);
const SLIDE_WINDOW_MAX_WIDTH: f32 = 1024.0;
const SLIDE_WINDOW_MAX_HEIGHT: f32 = 720.0;
const PRESENTER_TIME_UPDATE_INTERVAL: Duration = Duration::from_millis(250);
const FILE_MENU_ACTION_DELAY: Duration = Duration::from_millis(150);
const WINDOW_MENU_ACTION_DELAY: Duration = Duration::from_millis(150);
#[cfg(target_os = "linux")]
const INITIAL_LINUX_WINDOW_SHOW_DELAY: Duration = Duration::from_millis(120);
#[cfg(target_os = "linux")]
const INITIAL_LINUX_PRESENTER_FRONT_DELAY: Duration = Duration::from_millis(80);

fn main() -> Result<()> {
    #[cfg(target_os = "macos")]
    if let Some(result) = macos_renderer::entry() {
        return result;
    }
    #[cfg(debug_assertions)]
    if std::env::args_os().nth(1).as_deref()
        == Some(std::ffi::OsStr::new("--renderer-scheduler-smoke"))
    {
        let paths: Vec<_> = std::env::args_os().skip(2).collect();
        anyhow::ensure!(paths.len() == 2, "scheduler smoke requires two PDF paths");
        return renderer_helper::scheduler_smoke(paths[0].clone().into(), paths[1].clone().into());
    }
    #[cfg(debug_assertions)]
    if std::env::args_os().nth(1).as_deref()
        == Some(std::ffi::OsStr::new("--renderer-recovery-smoke"))
    {
        let paths: Vec<_> = std::env::args_os().skip(2).collect();
        anyhow::ensure!(paths.len() == 2, "recovery smoke requires two PDF paths");
        return renderer_helper::recovery_smoke(paths[0].clone().into(), paths[1].clone().into());
    }
    if std::env::args_os().nth(1).as_deref()
        == Some(std::ffi::OsStr::new(renderer_helper::HELPER_ARGUMENT))
    {
        return renderer_helper::run();
    }
    let startup_request = parse_startup_options(std::env::args_os().skip(1))?;
    if startup_request == StartupRequest::Help {
        print!("{}", help_text(&startup_program_name()));
        return Ok(());
    }

    let StartupRequest::Run(startup_options) = startup_request else {
        unreachable!("help requests return before app startup");
    };
    let diagnostics = init_diagnostics(startup_options.log_file_path.clone())?;
    let startup_persistence = startup_state::StartupPersistence::for_options(
        &startup_options,
        startup_state::StartupStore::default_store,
    );
    if let Some(path) = startup_options.smoke_open_pdf_path {
        return smoke_open_pdf(path);
    }
    if let Some(options) = startup_options.gui_smoke {
        return run_gui_smoke(options);
    }

    let startup_persistence = startup_persistence.expect("normal GUI mode has startup persistence");
    let startup_pdf = startup_persistence.select_pdf(startup_options.pdf_path);
    let windows = AppWindows::new()?;
    windows
        .placement
        .initialize(startup_persistence.loaded.window_placement.clone());
    configure_linux_desktop_identity()?;
    configure_shortcut_modifiers(&windows);
    configure_presenter_menu_bar(&windows);
    let recent_store = default_recent_file_store();
    let recent_files = load_recent_files(recent_store.as_ref());
    let recent_menu_paths = recent_files.paths().to_vec();
    let now = Instant::now();
    let mut watcher_recovery = hot_reload::WatcherRecoveryState::default();
    let pdf_watcher = match PdfWatcher::new() {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            warn!(error = ?error, "PDF watcher is unavailable at startup");
            watcher_recovery.record_failure(now);
            None
        }
    };
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState {
        render_scheduler: Some(RenderScheduler::start()),
        pdf_watcher,
        watcher_recovery,
        recent_files,
        recent_menu_paths,
        recent_store,
        diagnostics_log_path: diagnostics.log_path().map(PathBuf::from),
        ..AppState::default()
    }));

    let _control_runtime = match control_app::ControlRuntime::install(windows.refs(), state.clone())
    {
        Ok(runtime) => Some(runtime),
        Err(error) => {
            warn!(error = %error, "Local presentation control is unavailable");
            None
        }
    };
    wire_callbacks(&windows, windows.refs(), state.clone());
    let _presenter_time_timer = start_presenter_time_updates(windows.refs(), state.clone());
    let _render_event_timer = start_render_event_updates(windows.refs(), state.clone());
    let _pending_open_status_timer =
        start_pending_open_status_updates(windows.refs(), state.clone());
    let _hot_reload_timer = start_hot_reload_updates(windows.refs(), state.clone());
    let _file_dialog_result_timer = start_file_dialog_result_updates(windows.refs(), state.clone());
    update_recent_file_menu(&windows.refs().presenter, &state.borrow().recent_files);
    apply_app_metadata(&windows.presenter);

    windows.apply_initial_positions();
    initialize_slide_window_size(&windows.refs());
    let window_refs = show_initial_windows(&windows)?;
    set_application_icon();
    remove_macos_native_about_menu_item();
    let _slide_chrome_sync_timer = start_slide_chrome_sync(window_refs.clone());
    let _render_sizing_timer = start_render_sizing_updates(window_refs.clone(), state.clone());
    window_controller::start_slide_placement(window_refs.clone());

    if let Some(pdf) = startup_pdf {
        load_startup_pdf(&windows.refs(), &state, pdf.path);
        if pdf.automatic {
            let session_id = pending_open_session_id(&state.borrow());
            state.borrow_mut().automatic_reopen = session_id;
        }
    }

    let event_loop_result = slint::run_event_loop();
    if event_loop_result.is_ok() {
        window_controller::capture_slide_placement(&windows.refs());
        let last_pdf = state
            .borrow()
            .active_document_path
            .as_deref()
            .and_then(|path| match startup_state::SavedPdf::capture(path) {
                Ok(saved) => Some(saved),
                Err(error) => {
                    warn!(error = %error, "could not capture active PDF path");
                    None
                }
            });
        let saved = startup_state::StartupState {
            window_placement: windows.placement.remembered.borrow().clone(),
            last_pdf,
        };
        if let Err(error) = startup_persistence.save_if_changed(saved) {
            warn!(error = %error, "could not save startup settings");
        }
    }
    event_loop_result?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn show_initial_windows(windows: &AppWindows) -> Result<AppWindowRefs> {
    let refs = windows.refs();
    let deferred_refs = refs.clone();
    Timer::single_shot(INITIAL_LINUX_WINDOW_SHOW_DELAY, move || {
        if let Err(err) = show_initial_windows_from_refs(&deferred_refs) {
            eprintln!("failed to show initial windows: {err:?}");
        }
    });
    Ok(refs)
}

#[cfg(not(target_os = "linux"))]
fn show_initial_windows(windows: &AppWindows) -> Result<AppWindowRefs> {
    windows.slide.show()?;
    let refs = windows.refs();
    apply_macos_slide_window_chrome(&refs);
    sync_slide_chrome(&refs);
    windows.presenter.show()?;
    Ok(refs)
}

#[cfg(target_os = "linux")]
fn show_initial_windows_from_refs(windows: &AppWindowRefs) -> Result<()> {
    if let Some(slide) = windows.slide.upgrade() {
        slide.show()?;
    }
    sync_slide_chrome(windows);

    let presenter_refs = windows.clone();
    Timer::single_shot(INITIAL_LINUX_PRESENTER_FRONT_DELAY, move || {
        if let Err(err) = show_initial_presenter_from_refs(&presenter_refs) {
            eprintln!("failed to show initial presenter window: {err:?}");
        }
    });
    Ok(())
}

#[cfg(target_os = "linux")]
fn show_initial_presenter_from_refs(windows: &AppWindowRefs) -> Result<()> {
    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.show()?;
    }
    stabilize_initial_presenter_layout(windows.clone());
    restore_presenter_input_after_transient_ui(windows.clone());
    Ok(())
}

#[cfg(target_os = "linux")]
fn stabilize_initial_presenter_layout(windows: AppWindowRefs) {
    for delay in [
        Duration::from_millis(0),
        Duration::from_millis(50),
        Duration::from_millis(150),
        Duration::from_millis(300),
    ] {
        let windows = windows.clone();
        Timer::single_shot(delay, move || {
            if let Some(presenter) = windows.presenter.upgrade() {
                let window = presenter.window();
                let size = window.size();
                window.set_size(size);
                window.request_redraw();
            }
        });
    }
}

fn initialize_slide_window_size(windows: &AppWindowRefs) {
    let size = default_slide_window_size();
    window_controller::set_slide_logical_size(
        windows,
        [f64::from(size.width), f64::from(size.height)],
    );
}

fn default_slide_window_size() -> slint::LogicalSize {
    fitted_slide_window_size(
        SLIDE_WINDOW_MAX_WIDTH,
        SLIDE_WINDOW_MAX_HEIGHT,
        aspect::DEFAULT_SLIDE_ASPECT_RATIO,
        0.0,
    )
}

fn configure_shortcut_modifiers(windows: &AppWindows) {
    // Slint maps physical Control to Meta on macOS, while Control means Command.
    let use_physical_control = use_physical_control_shortcuts();
    windows
        .presenter
        .set_use_physical_control_shortcuts(use_physical_control);
    windows
        .slide
        .set_use_physical_control_shortcuts(use_physical_control);
}

fn configure_presenter_menu_bar(windows: &AppWindows) {
    windows
        .presenter
        .set_use_native_menu_bar(use_native_presenter_menu_bar());
}

#[cfg(target_os = "linux")]
fn use_native_presenter_menu_bar() -> bool {
    false
}

#[cfg(not(target_os = "linux"))]
fn use_native_presenter_menu_bar() -> bool {
    true
}

#[cfg(all(unix, not(target_os = "macos")))]
fn configure_linux_desktop_identity() -> Result<()> {
    slint::set_xdg_app_id(app_metadata::LINUX_DESKTOP_APP_ID)?;
    Ok(())
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn configure_linux_desktop_identity() -> Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn use_physical_control_shortcuts() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
fn use_physical_control_shortcuts() -> bool {
    false
}

fn startup_program_name() -> String {
    std::env::args_os()
        .next()
        .and_then(|arg| {
            let path = PathBuf::from(arg);
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "quick-presenter".to_string())
}

fn smoke_open_pdf(path: PathBuf) -> Result<()> {
    const SMOKE_RENDER_WIDTH: i32 = 320;

    let group = renderer_helper::ProcessGroup::default();
    let mut helper = renderer_helper::HelperClient::spawn_for_document(&group, &path)?;
    let (title, page_count) = helper.open(path)?;
    #[cfg(debug_assertions)]
    if std::env::var_os("QUICK_PRESENTER_HELPER_TEST_ABORT_BROKER").is_some() {
        use std::io::Write;
        println!("renderer-helper-pid={}", helper.process_id());
        std::io::stdout().flush()?;
        std::process::abort();
    }
    let purpose = RenderPurpose::CurrentSlide;
    #[cfg(debug_assertions)]
    let purpose = if std::env::var_os("QUICK_PRESENTER_HELPER_TEST_AUXILIARY").is_some() {
        RenderPurpose::NextPreview
    } else {
        purpose
    };
    let page = helper.render(RenderRequest {
        page_index: 0,
        width: SMOKE_RENDER_WIDTH,
        purpose,
    })?;
    for index in 0..page_count {
        helper.notes_page(index)?;
    }
    println!(
        "Smoke open PDF succeeded through renderer helper: title=\"{}\" pages={} first_page={}x{} helper_pid={}",
        title,
        page_count,
        page.pixels.width(),
        page.pixels.height(),
        helper.process_id()
    );

    Ok(())
}

fn run_gui_smoke(options: GuiSmokeOptions) -> Result<()> {
    gui_smoke::run(options)
}

#[cfg(target_os = "macos")]
fn set_application_icon() {
    set_application_icon_now();
    Timer::single_shot(Duration::from_millis(0), set_application_icon_now);
    Timer::single_shot(Duration::from_millis(250), set_application_icon_now);
    Timer::single_shot(Duration::from_millis(1000), set_application_icon_now);
}

#[cfg(target_os = "macos")]
fn set_application_icon_now() {
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::{MainThreadMarker, NSData};
    use std::ffi::c_void;

    let Some(main_thread) = MainThreadMarker::new() else {
        warn!("failed to set application icon because the main thread marker is unavailable");
        return;
    };

    let icon_bytes = include_bytes!("../assets/icons/macos/QuickPresenter.icns");
    let icon_data = unsafe {
        NSData::dataWithBytes_length(icon_bytes.as_ptr().cast::<c_void>(), icon_bytes.len())
    };
    let Some(icon) = NSImage::initWithData(main_thread.alloc(), &icon_data) else {
        warn!("failed to decode application icon");
        return;
    };

    let app = NSApplication::sharedApplication(main_thread);
    unsafe {
        app.setApplicationIconImage(Some(&icon));
    }
}

#[cfg(not(target_os = "macos"))]
fn set_application_icon() {}

#[cfg(target_os = "macos")]
fn remove_macos_native_about_menu_item() {
    // Avoid mutating the native App menu at runtime. On macOS this can raise
    // Objective-C exceptions across the winit event loop boundary when the app
    // later changes the slide window geometry.
}

#[cfg(not(target_os = "macos"))]
fn remove_macos_native_about_menu_item() {}

#[allow(dead_code)]
#[cfg(test)]
struct RenderedPages {
    current: RenderedPage,
    next: Option<RenderedPage>,
}

#[allow(dead_code)]
#[cfg(test)]
struct PreparedPdfSession {
    loaded_path: PathBuf,
    doc: PdfDocumentState,
    notes: SpeakerNotes,
    presentation: PresentationState,
    snapshot: Option<PageSnapshot>,
    initial_slide_aspect_ratio: Option<f32>,
    initial_pages: Option<RenderedPages>,
    render_cache: RenderCache,
    status_text: String,
}

#[allow(dead_code)]
#[cfg(test)]
struct SynchronousPdfSession {
    doc: PdfDocumentState,
}

#[allow(dead_code)]
#[cfg(test)]
struct CommittedPdfSession {
    session: SynchronousPdfSession,
    loaded_path: PathBuf,
    snapshot: Option<PageSnapshot>,
    initial_slide_aspect_ratio: Option<f32>,
    initial_pages: Option<RenderedPages>,
}

fn wire_callbacks(windows: &AppWindows, refs: AppWindowRefs, state: Rc<RefCell<AppState>>) {
    let app = &windows.presenter;
    let display_swap_active = Rc::new(DisplaySwapController::default());

    wire_presenter_close_request(windows);

    let window_refs = refs.clone();
    let state_for_open = state.clone();
    app.on_open_pdf(move || {
        request_pdf_file_open(window_refs.clone(), state_for_open.clone());
    });

    wire_recent_file_callbacks(app, refs.clone(), state.clone());

    let presenter = refs.presenter.clone();
    app.on_fit_notes_font_size(move |has_notes, width, height, measurements| {
        let heights: Vec<f32> = measurements.iter().collect();
        let size = notes::fit_notes_font_size(has_notes, width, height, &heights);
        if let Some(presenter) = presenter.upgrade() {
            presenter.set_notes_font_size(f32::from(size));
        }
    });

    let window_refs = refs.clone();
    let state_for_previous = state.clone();
    app.on_previous_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_previous,
            PresentationCommand::PreviousPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_next = state.clone();
    app.on_next_page(move || {
        handle_presentation_command(&window_refs, &state_for_next, PresentationCommand::NextPage);
    });

    let window_refs = refs.clone();
    let state_for_first = state.clone();
    app.on_first_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_first,
            PresentationCommand::FirstPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_last = state.clone();
    app.on_last_page(move || {
        handle_presentation_command(&window_refs, &state_for_last, PresentationCommand::LastPage);
    });

    let window_refs = refs.clone();
    let state_for_jump = state.clone();
    app.on_jump_to_page(move |page_index| {
        let Ok(page_index) = u32::try_from(page_index) else {
            return;
        };
        handle_presentation_command(
            &window_refs,
            &state_for_jump,
            PresentationCommand::JumpToPage(page_index),
        );
    });

    let state_for_thumbnail_range = state.clone();
    app.on_request_thumbnail_range(move |first_index, last_index| {
        enqueue_thumbnail_visible_range(&state_for_thumbnail_range, first_index, last_index);
    });

    let window_refs = refs.clone();
    let state_for_black_screen = state.clone();
    app.on_toggle_black_screen(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_black_screen,
            PresentationCommand::ToggleBlackScreen,
        );
    });

    let window_refs = refs.clone();
    let state_for_keyboard_previous = state.clone();
    app.on_keyboard_previous_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_keyboard_previous,
            PresentationCommand::PreviousPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_keyboard_next = state.clone();
    app.on_keyboard_next_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_keyboard_next,
            PresentationCommand::NextPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_keyboard_first = state.clone();
    app.on_keyboard_first_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_keyboard_first,
            PresentationCommand::FirstPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_keyboard_last = state.clone();
    app.on_keyboard_last_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_keyboard_last,
            PresentationCommand::LastPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_presenter_swap = state.clone();
    let presenter_swap_active = display_swap_active.clone();
    app.on_swap_displays(move || {
        handle_display_swap(
            &window_refs,
            &state_for_presenter_swap,
            WindowRole::Presenter,
            presenter_swap_active.clone(),
        );
    });

    let window_refs = refs.clone();
    let state_for_toggle = state.clone();
    let swap_for_toggle = display_swap_active.clone();
    app.on_toggle_slide_fullscreen(move || {
        swap_for_toggle.cancel();
        let mut state = state_for_toggle.borrow_mut();
        let fullscreen = state.fullscreen.toggle_slide_fullscreen();
        set_slide_fullscreen(&window_refs, fullscreen);
    });

    wire_window_menu_callbacks(app, refs.clone(), state.clone());

    let window_refs = refs.clone();
    let state_for_presenter_exit = state.clone();
    let swap_for_presenter_exit = display_swap_active.clone();
    app.on_exit_slide_fullscreen(move || {
        swap_for_presenter_exit.cancel();
        handle_presentation_command(
            &window_refs,
            &state_for_presenter_exit,
            PresentationCommand::ExitSlideFullscreen,
        );
    });

    let window_refs = refs.clone();
    let state_for_slide_previous = state.clone();
    windows.slide.on_keyboard_previous_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_slide_previous,
            PresentationCommand::PreviousPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_slide_next = state.clone();
    windows.slide.on_keyboard_next_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_slide_next,
            PresentationCommand::NextPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_slide_first = state.clone();
    windows.slide.on_keyboard_first_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_slide_first,
            PresentationCommand::FirstPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_slide_last = state.clone();
    windows.slide.on_keyboard_last_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_slide_last,
            PresentationCommand::LastPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_slide_swap = state.clone();
    let slide_swap_active = display_swap_active.clone();
    windows.slide.on_swap_displays(move || {
        handle_display_swap(
            &window_refs,
            &state_for_slide_swap,
            WindowRole::Slide,
            slide_swap_active.clone(),
        );
    });

    let window_refs = refs.clone();
    let state_for_slide_black_screen = state.clone();
    windows.slide.on_toggle_black_screen(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_slide_black_screen,
            PresentationCommand::ToggleBlackScreen,
        );
    });

    let window_refs = refs.clone();
    let state_for_slide_toggle = state.clone();
    let swap_for_slide_toggle = display_swap_active.clone();
    windows.slide.on_toggle_slide_fullscreen(move || {
        swap_for_slide_toggle.cancel();
        let mut state = state_for_slide_toggle.borrow_mut();
        let fullscreen = state.fullscreen.toggle_slide_fullscreen();
        set_slide_fullscreen(&window_refs, fullscreen);
    });

    let window_refs = refs;
    let state_for_slide_exit = state;
    windows.slide.on_exit_fullscreen(move || {
        display_swap_active.cancel();
        handle_presentation_command(
            &window_refs,
            &state_for_slide_exit,
            PresentationCommand::ExitSlideFullscreen,
        );
    });
}

fn handle_display_swap(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    initiated_by: WindowRole,
    active: Rc<DisplaySwapController>,
) {
    let both_visible = {
        let state = state.borrow();
        state.window_menu.presenter_visible() && state.window_menu.slide_visible()
    };

    let outcome = if both_visible {
        request_display_swap(windows, initiated_by, active)
    } else {
        DisplaySwapOutcome::HiddenWindow
    };

    let message = match outcome {
        DisplaySwapOutcome::Applied | DisplaySwapOutcome::Busy => return,
        DisplaySwapOutcome::SameDisplay => "Connect a second display before switching screens.",
        DisplaySwapOutcome::HiddenWindow => "Show both windows before switching screens.",
        DisplaySwapOutcome::Unavailable => "Display switching is unavailable on this desktop.",
    };

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_status_text(message.into());
    }
}

fn wire_presenter_close_request(windows: &AppWindows) {
    let refs = windows.refs();
    windows.presenter.window().on_close_requested(move || {
        window_controller::capture_slide_placement(&refs);
        match slint::quit_event_loop() {
            Ok(()) => CloseRequestResponse::KeepWindowShown,
            Err(err) => {
                warn!(error = ?err, "failed to quit event loop from presenter close request");
                CloseRequestResponse::KeepWindowShown
            }
        }
    });
}

fn apply_app_metadata(app: &PresenterWindow) {
    let metadata = about_metadata(pdfium_runtime_version_label());

    app.set_about_app_name(metadata.app_name.into());
    app.set_about_version_label(metadata.app_version_label.into());
    app.set_about_license_id(metadata.app_license_id.into());
    app.set_about_license_summary(metadata.app_license_summary.into());
    app.set_about_pdfium_version_label(metadata.pdfium_version_label.into());
    app.set_about_pdfium_license_summary(metadata.pdfium_license_summary.into());
}

fn wire_window_menu_callbacks(
    presenter: &PresenterWindow,
    refs: AppWindowRefs,
    state: Rc<RefCell<AppState>>,
) {
    if let Some(slide) = refs.slide.upgrade() {
        let state = state.clone();
        slide.window().on_close_requested(move || {
            state.borrow_mut().window_menu.set_slide_visible(false);
            CloseRequestResponse::HideWindow
        });
    }

    let window_refs = refs.clone();
    let state_for_presenter_toggle = state.clone();
    presenter.on_show_presenter_window(move || {
        schedule_window_menu_action(
            window_refs.clone(),
            state_for_presenter_toggle.clone(),
            |windows, state| {
                show_presenter_window_from_menu(&windows, &state);
            },
        );
    });

    let window_refs = refs.clone();
    let state_for_presenter_front = state.clone();
    presenter.on_show_slide_window(move || {
        schedule_window_menu_action(
            window_refs.clone(),
            state_for_presenter_front.clone(),
            |windows, state| {
                show_slide_window_from_menu(&windows, &state);
            },
        );
    });

    let window_refs = refs.clone();
    let state_for_slide_front = state.clone();
    presenter.on_hide_slide_window(move || {
        schedule_window_menu_action(
            window_refs.clone(),
            state_for_slide_front.clone(),
            |windows, state| {
                hide_slide_window_from_menu(&windows, &state);
            },
        );
    });

    let window_refs = refs.clone();
    let state_for_presenter_front = state.clone();
    presenter.on_bring_presenter_window_to_front(move || {
        schedule_window_menu_action(
            window_refs.clone(),
            state_for_presenter_front.clone(),
            |windows, state| {
                bring_presenter_window_to_front(&windows, &state);
            },
        );
    });

    let window_refs = refs.clone();
    let state_for_slide_front = state.clone();
    presenter.on_bring_slide_window_to_front(move || {
        schedule_window_menu_action(
            window_refs.clone(),
            state_for_slide_front.clone(),
            |windows, state| {
                bring_slide_window_to_front(&windows, &state);
            },
        );
    });
}

fn schedule_window_menu_action(
    windows: AppWindowRefs,
    state: Rc<RefCell<AppState>>,
    action: impl FnOnce(AppWindowRefs, Rc<RefCell<AppState>>) + 'static,
) {
    Timer::single_shot(WINDOW_MENU_ACTION_DELAY, move || action(windows, state));
}

fn schedule_open_pdf(
    windows: AppWindowRefs,
    state: Rc<RefCell<AppState>>,
    path: PathBuf,
    error_context: &'static str,
) {
    Timer::single_shot(FILE_MENU_ACTION_DELAY, move || {
        let _ = error_context;
        begin_open_pdf(&windows, &state, path);
    });
}

fn wire_recent_file_callbacks(
    app: &PresenterWindow,
    refs: AppWindowRefs,
    state: Rc<RefCell<AppState>>,
) {
    let window_refs = refs.clone();
    let state_for_recent = state.clone();
    app.on_open_recent_file(move |index| open_recent_pdf(&window_refs, &state_for_recent, index));

    let presenter = refs.presenter;
    app.on_clear_recent_files(move || clear_recent_files(&presenter, &state));
}

fn handle_presentation_command(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    command: PresentationCommand,
) {
    if command == PresentationCommand::Close {
        apply_session_command(&mut state.borrow_mut(), command, Instant::now());
        view_sync::apply_closed_state_to_windows(windows);
        return;
    }
    let preload_snapshot = {
        let mut state = state.borrow_mut();
        let outcome = apply_session_command(&mut state, command, Instant::now());

        if let Some(fullscreen) = outcome.slide_fullscreen {
            set_slide_fullscreen(windows, fullscreen);
        }

        if let Some(snapshot) = outcome.snapshot {
            apply_snapshot_to_windows(windows, &state, &snapshot);
            enqueue_visible_page_renders(&state, &snapshot);
            Some(snapshot)
        } else {
            None
        }
    };

    if let Some(snapshot) = preload_snapshot {
        schedule_presentation_preload(state.clone(), snapshot);
        schedule_thumbnail_render(windows.clone(), state.clone());
    }
}

fn show_presenter_window_from_menu(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    state.borrow_mut().window_menu.set_presenter_visible(true);
    show_presenter_window(windows);
}

fn show_slide_window_from_menu(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    state.borrow_mut().window_menu.set_slide_visible(true);
    show_slide_window(windows);
}

fn hide_slide_window_from_menu(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    let show_presenter_first = {
        let mut state = state.borrow_mut();
        let show_presenter_first = !state.window_menu.presenter_visible();
        if show_presenter_first {
            state.window_menu.set_presenter_visible(true);
        }
        state.window_menu.set_slide_visible(false);
        show_presenter_first
    };

    if show_presenter_first {
        show_presenter_window(windows);
    }
    hide_slide_window(windows);
}

fn bring_presenter_window_to_front(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    {
        let mut state = state.borrow_mut();
        state.window_menu.set_presenter_visible(true);
    }
    show_presenter_window(windows);
}

fn bring_slide_window_to_front(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    {
        let mut state = state.borrow_mut();
        state.window_menu.set_slide_visible(true);
    }
    show_slide_window(windows);
}

#[cfg(target_os = "linux")]
fn request_pdf_file_open(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) {
    let (sender, receiver) = mpsc::channel();

    {
        let mut state = state.borrow_mut();
        if !begin_file_dialog_request(&mut state) {
            return;
        }
        state.file_dialog.result_receiver = Some(receiver);
    }

    let dialog = pdf_file_dialog_for_presenter(&windows);
    std::thread::spawn(move || {
        let _ = sender.send(dialog.pick_file());
    });
}

#[cfg(not(target_os = "linux"))]
fn request_pdf_file_open(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) {
    if let Some(path) = pick_pdf_file() {
        schedule_open_pdf(windows, state, path, "failed to open and render PDF");
    }
}

#[cfg(any(target_os = "linux", test))]
fn begin_file_dialog_request(state: &mut AppState) -> bool {
    if state.file_dialog.open {
        return false;
    }

    state.file_dialog.open = true;
    true
}

#[cfg(any(target_os = "linux", test))]
fn finish_file_dialog_request(state: &mut AppState) {
    state.file_dialog.open = false;
    #[cfg(target_os = "linux")]
    {
        state.file_dialog.result_receiver = None;
    }
}

#[cfg(not(target_os = "linux"))]
fn pick_pdf_file() -> Option<PathBuf> {
    pdf_file_dialog().pick_file()
}

fn pdf_file_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_title("Open PDF")
}

#[cfg(target_os = "linux")]
fn pdf_file_dialog_for_presenter(windows: &AppWindowRefs) -> rfd::FileDialog {
    let dialog = pdf_file_dialog();
    let Some(presenter) = windows.presenter.upgrade() else {
        return dialog;
    };

    dialog.set_parent(&presenter.window().window_handle())
}

fn load_startup_pdf(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>, path: PathBuf) {
    begin_open_pdf(windows, state, path);
}

pub(crate) fn begin_open_pdf(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
) {
    let path = if path.is_absolute() {
        path
    } else {
        match std::env::current_dir() {
            Ok(directory) => directory.join(path),
            Err(error) => {
                warn!(error = %error, "could not resolve absolute PDF path");
                path
            }
        }
    };
    let title = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("PDF")
        .to_owned();

    let had_open_deck = {
        let mut state = state.borrow_mut();
        let had_open_deck = state.presentation.snapshot().is_some();
        if !ensure_render_scheduler_for_open(&mut state) {
            state.status_text = "Rendering is still stopping. Try again in a moment.".to_owned();
            return;
        }

        state.hot_reload.begin_manual_open();
        let session_id = begin_open_pdf_state(&mut state, path.clone());
        state
            .render_scheduler
            .as_ref()
            .expect("render scheduler should exist after open readiness check")
            .open(session_id, path);
        had_open_deck
    };

    if had_open_deck {
        set_presenter_message(
            &windows.presenter,
            PresenterMessage::new("Opening PDF...", errors::MessageSeverity::Info),
        );
    } else {
        apply_opening_state_to_windows(windows, &title);
    }
}

fn ensure_render_scheduler_for_open(state: &mut AppState) -> bool {
    let Some(scheduler) = state.render_scheduler.as_ref() else {
        state.render_scheduler = Some(RenderScheduler::start());
        return true;
    };

    match scheduler.lifecycle() {
        RenderWorkerLifecycle::Running => true,
        RenderWorkerLifecycle::ShutdownRequested => false,
        RenderWorkerLifecycle::Stopped | RenderWorkerLifecycle::Failed => {
            if !scheduler.can_be_replaced() {
                return false;
            }
            if !scheduler.try_join_finished_worker() {
                return false;
            }
            state.render_scheduler = Some(RenderScheduler::start());
            true
        }
    }
}

#[allow(dead_code)]
#[cfg(test)]
pub(crate) fn open_and_render(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
) -> Result<()> {
    let prepared = prepare_pdf_session(path);
    let committed = {
        let mut state = state.borrow_mut();
        commit_prepared_pdf_session(&mut state, prepared)?
    };

    if let Some(aspect_ratio) = committed.initial_slide_aspect_ratio {
        fit_slide_window_to_aspect_ratio(windows, aspect_ratio);
    }

    if let (Some(snapshot), Some(rendered)) = (&committed.snapshot, committed.initial_pages) {
        {
            let state = state.borrow();
            apply_rendered_pages_to_windows(windows, &state, snapshot, rendered);
        }
        schedule_presentation_preload(state.clone(), snapshot.clone());
        schedule_thumbnail_render(windows.clone(), state.clone());
    }

    record_recent_pdf(&windows.presenter, state, committed.loaded_path);
    let _session = committed.session;

    Ok(())
}

#[allow(dead_code)]
#[cfg(test)]
fn prepare_pdf_session(path: PathBuf) -> Result<PreparedPdfSession> {
    prepare_pdf_session_with_initial_render(path, render_pages)
}

#[allow(dead_code)]
#[cfg(test)]
fn prepare_pdf_session_with_initial_render(
    path: PathBuf,
    render_initial_pages: impl FnOnce(
        &PdfDocumentState,
        &mut RenderCache,
        &PageSnapshot,
    ) -> Result<RenderedPages>,
) -> Result<PreparedPdfSession> {
    let loaded_path = path.clone();
    let doc = PdfDocumentState::open(path)?;
    let (notes, status_text) = match doc.speaker_notes() {
        Ok(notes) => (notes, "Ready".to_owned()),
        Err(err) => {
            warn!(error = ?err, "speaker notes unavailable");
            (
                SpeakerNotes::empty(),
                errors::speaker_notes_warning(&err).text().to_owned(),
            )
        }
    };
    let presentation = PresentationState::open_document(doc.title(), doc.page_count());
    let snapshot = presentation.snapshot();
    let mut render_cache = RenderCache::default();
    let initial_pages = snapshot
        .as_ref()
        .map(|snapshot| render_initial_pages(&doc, &mut render_cache, snapshot))
        .transpose()?;
    if let Some(snapshot) = snapshot.as_ref() {
        render_cache.retain_presentation_window(
            snapshot.current_index,
            snapshot.total_pages,
            PRESENTATION_CACHE_RADIUS,
        );
    }
    let initial_slide_aspect_ratio = initial_pages
        .as_ref()
        .map(|rendered| rendered.current.aspect_ratio);

    Ok(PreparedPdfSession {
        loaded_path,
        doc,
        notes,
        presentation,
        snapshot,
        initial_slide_aspect_ratio,
        initial_pages,
        render_cache,
        status_text,
    })
}

#[cfg(test)]
fn commit_prepared_pdf_session(
    state: &mut AppState,
    prepared: Result<PreparedPdfSession>,
) -> Result<CommittedPdfSession> {
    let prepared = prepared?;
    Ok(commit_prepared_pdf_session_state(state, prepared))
}

#[cfg(test)]
fn commit_prepared_pdf_session_state(
    state: &mut AppState,
    prepared: PreparedPdfSession,
) -> CommittedPdfSession {
    let PreparedPdfSession {
        loaded_path,
        doc,
        notes,
        presentation,
        snapshot,
        initial_slide_aspect_ratio,
        initial_pages,
        render_cache,
        status_text,
    } = prepared;

    state.render_cache = render_cache;
    commit_prepared_pdf_session_metadata(state, notes, presentation, status_text);

    CommittedPdfSession {
        session: SynchronousPdfSession { doc },
        loaded_path,
        snapshot,
        initial_slide_aspect_ratio,
        initial_pages,
    }
}

#[cfg(test)]
fn commit_prepared_pdf_session_metadata(
    state: &mut AppState,
    notes: SpeakerNotes,
    presentation: PresentationState,
    status_text: String,
) {
    state.render_generation = state.render_generation.wrapping_add(1);
    state.thumbnails = ThumbnailState {
        total_pages: presentation
            .snapshot()
            .map(|snapshot| snapshot.total_pages)
            .unwrap_or(0),
    };
    state.notes = notes;
    state.presentation = presentation;
    state.black_screen.set_active(false);
    state.timer.reset();
    state.status_text = status_text;
}

fn open_recent_pdf(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>, index: i32) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };

    let path = state
        .borrow()
        .recent_menu_paths
        .get(index)
        .map(PathBuf::from);

    let Some(path) = path else {
        return;
    };

    restore_presenter_input_after_transient_ui(windows.clone());

    schedule_open_pdf(
        windows.clone(),
        state.clone(),
        path,
        "failed to open recent PDF",
    );
}

fn load_recent_files(store: Option<&RecentFileStore>) -> RecentFiles {
    let Some(store) = store else {
        warn!("recent file storage is unavailable");
        return RecentFiles::new();
    };

    match store.load() {
        Ok(recent_files) => recent_files,
        Err(err) => {
            warn!(error = ?err, "failed to load recent files");
            RecentFiles::new()
        }
    }
}

fn record_recent_pdf(
    presenter: &Weak<PresenterWindow>,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
) {
    let snapshot = {
        let mut state = state.borrow_mut();
        state.recent_files.add(path);
        let snapshot = RecentMenuSnapshot::from_recent_files(&state.recent_files);
        state.recent_menu_paths = snapshot.paths().to_vec();

        if let Some(store) = state.recent_store.as_ref() {
            if let Err(err) = store.save(&state.recent_files) {
                warn!(error = ?err, "failed to save recent files");
            }
        }

        snapshot
    };

    update_recent_file_menu_snapshot(presenter, &snapshot);
}

fn clear_recent_files(presenter: &Weak<PresenterWindow>, state: &Rc<RefCell<AppState>>) {
    let snapshot = {
        let mut state = state.borrow_mut();
        state.recent_files.clear();
        let snapshot = RecentMenuSnapshot::from_recent_files(&state.recent_files);
        state.recent_menu_paths = snapshot.paths().to_vec();

        if let Some(store) = state.recent_store.as_ref() {
            if let Err(err) = store.save(&state.recent_files) {
                warn!(error = ?err, "failed to save cleared recent files");
            }
        }

        snapshot
    };

    update_recent_file_menu_snapshot(presenter, &snapshot);
}

fn update_recent_file_menu(presenter: &Weak<PresenterWindow>, recent_files: &RecentFiles) {
    let snapshot = RecentMenuSnapshot::from_recent_files(recent_files);
    update_recent_file_menu_snapshot(presenter, &snapshot);
}

fn update_recent_file_menu_snapshot(
    presenter: &Weak<PresenterWindow>,
    snapshot: &RecentMenuSnapshot,
) {
    let Some(presenter) = presenter.upgrade() else {
        return;
    };

    let labels = snapshot.labels();
    let enabled = snapshot.enabled();
    presenter.set_has_recent_files(snapshot.has_recent_files());
    presenter.set_recent_file_label_0(labels[0].clone().into());
    presenter.set_recent_file_label_1(labels[1].clone().into());
    presenter.set_recent_file_label_2(labels[2].clone().into());
    presenter.set_recent_file_label_3(labels[3].clone().into());
    presenter.set_recent_file_label_4(labels[4].clone().into());
    presenter.set_recent_file_0_enabled(enabled[0]);
    presenter.set_recent_file_1_enabled(enabled[1]);
    presenter.set_recent_file_2_enabled(enabled[2]);
    presenter.set_recent_file_3_enabled(enabled[3]);
    presenter.set_recent_file_4_enabled(enabled[4]);
}

fn fit_slide_window_to_aspect_ratio(windows: &AppWindowRefs, aspect_ratio: f32) {
    if windows.placement.preserve_size.get() {
        return;
    }
    if let Some(slide) = windows.slide.upgrade() {
        if slide.window().is_fullscreen() {
            return;
        }

        let compensation_height =
            slide_titlebar_compensation_height(slide.window().is_fullscreen());
        let size = fitted_slide_window_size(
            SLIDE_WINDOW_MAX_WIDTH,
            SLIDE_WINDOW_MAX_HEIGHT,
            aspect_ratio,
            compensation_height,
        );
        window_controller::set_slide_logical_size(
            windows,
            [f64::from(size.width), f64::from(size.height)],
        );
    }
}

#[cfg(test)]
fn fitted_slide_window_content_size(
    aspect_ratio: f32,
    titlebar_compensation_height: f32,
) -> (f32, f32) {
    let size = fitted_slide_window_size(
        SLIDE_WINDOW_MAX_WIDTH,
        SLIDE_WINDOW_MAX_HEIGHT,
        aspect_ratio,
        titlebar_compensation_height,
    );
    (size.width, size.height)
}

fn enqueue_visible_page_renders(state: &AppState, snapshot: &PageSnapshot) {
    enqueue_render_plan_if_missing(state, visible_page_render_plan(snapshot, state));
}

#[cfg(test)]
#[allow(dead_code)]
fn render_into_windows(
    windows: &AppWindowRefs,
    session: &SynchronousPdfSession,
    state: &mut AppState,
    snapshot: &PageSnapshot,
) -> Result<()> {
    let rendered = render_pages(&session.doc, &mut state.render_cache, snapshot)?;
    state.render_cache.retain_presentation_window(
        snapshot.current_index,
        snapshot.total_pages,
        PRESENTATION_CACHE_RADIUS,
    );

    apply_rendered_pages_to_windows(windows, state, snapshot, rendered);

    Ok(())
}

#[allow(dead_code)]
#[cfg(test)]
fn apply_rendered_pages_to_windows(
    windows: &AppWindowRefs,
    state: &AppState,
    snapshot: &PageSnapshot,
    rendered: RenderedPages,
) {
    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_current_page_image(rendered.current.image.clone());
        presenter.set_current_page_aspect_ratio(rendered.current.aspect_ratio);
        presenter.set_has_next_page(rendered.next.is_some());
        if let Some(next) = rendered.next.as_ref() {
            presenter.set_next_page_image(next.image.clone());
            presenter.set_next_page_aspect_ratio(next.aspect_ratio);
        }
        presenter.set_document_title(snapshot.title.clone().into());
        presenter.set_page_label(snapshot.page_label.clone().into());
        presenter.set_clock_time_label(current_clock_label().into());
        presenter.set_elapsed_time_label(state.timer.elapsed_label_at(Instant::now()).into());
        presenter.set_status_text(presenter_status_text(state).into());
        presenter.set_has_slide_progress(true);
        presenter.set_slide_progress_value(snapshot.progress_fraction());
        presenter.set_thumbnails(thumbnail_model(
            &state.thumbnails,
            &state.render_cache,
            snapshot.current_index,
        ));
        presenter.set_current_page_index(thumbnail_current_row_index(
            snapshot.total_pages,
            snapshot.current_index,
            THUMBNAIL_CACHE_RADIUS,
        ));

        let current_note = state.notes.note_for_page_index(snapshot.current_index);
        presenter.set_has_notes(current_note.is_some());
        presenter.set_notes_text(current_note.unwrap_or_default().into());
    }

    if let Some(slide) = windows.slide.upgrade() {
        slide.set_page_aspect_ratio(rendered.current.aspect_ratio);
        slide.set_page_image(if state.black_screen.is_active() {
            black_slide_image()
        } else {
            rendered.current.image
        });
    }
}

#[allow(dead_code)]
#[cfg(test)]
fn render_pages(
    doc: &PdfDocumentState,
    cache: &mut RenderCache,
    snapshot: &PageSnapshot,
) -> Result<RenderedPages> {
    Ok(RenderedPages {
        current: render_pdf_page_cached(
            doc,
            cache,
            RenderRequest {
                page_index: snapshot.current_index,
                width: CURRENT_RENDER_WIDTH,
                purpose: RenderPurpose::CurrentSlide,
            },
        )?,
        next: snapshot
            .next_index
            .map(|page_index| {
                render_pdf_page_cached(
                    doc,
                    cache,
                    RenderRequest {
                        page_index,
                        width: PREVIEW_RENDER_WIDTH,
                        purpose: RenderPurpose::NextPreview,
                    },
                )
            })
            .transpose()?,
    })
}

#[allow(dead_code)]
#[cfg(test)]
fn render_pdf_page_cached(
    doc: &PdfDocumentState,
    cache: &mut RenderCache,
    request: RenderRequest,
) -> Result<RenderedPage> {
    cache.get_or_render(request, |request| {
        let aspect_ratio = doc.page_aspect_ratio(request.page_index)?;
        let pixels = doc.render_page_pixels(request.page_index, request.width)?;
        let estimated_bytes = rendering::actual_render_bytes(&pixels);
        Ok(RenderedPage {
            image: slint::Image::from_rgba8(pixels),
            aspect_ratio,
            estimated_bytes,
        })
    })
}

fn schedule_thumbnail_render(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) {
    let generation = state.borrow().render_generation;

    Timer::single_shot(Duration::from_millis(0), move || {
        let state = state.borrow();
        if state.render_generation != generation {
            return;
        }

        let Some(snapshot) = state.presentation.snapshot() else {
            return;
        };

        enqueue_render_plan_if_missing(
            &state,
            thumbnail_render_plan(&snapshot, THUMBNAIL_CACHE_RADIUS),
        );

        if let Some(presenter) = windows.presenter.upgrade() {
            sync_thumbnail_model(&presenter, &state, &snapshot);
        }
    });
}

fn sync_thumbnail_model(presenter: &PresenterWindow, state: &AppState, snapshot: &PageSnapshot) {
    presenter.set_thumbnails(thumbnail_model(
        &state.thumbnails,
        &state.render_cache,
        snapshot.current_index,
    ));
    presenter.set_current_page_index(thumbnail_current_row_index(
        snapshot.total_pages,
        snapshot.current_index,
        THUMBNAIL_CACHE_RADIUS,
    ));
}

fn enqueue_thumbnail_visible_range(
    state: &Rc<RefCell<AppState>>,
    first_index: i32,
    last_index: i32,
) {
    let state = state.borrow();
    let Some(snapshot) = state.presentation.snapshot() else {
        return;
    };

    enqueue_render_plan_if_missing(
        &state,
        thumbnail_visible_range_render_plan(
            snapshot.total_pages,
            first_index,
            last_index,
            THUMBNAIL_SCROLL_LOOKAHEAD,
        ),
    );
}

#[allow(dead_code)]
#[cfg(test)]
fn render_thumbnail_window(
    session: &SynchronousPdfSession,
    state: &mut AppState,
    snapshot: &PageSnapshot,
) -> Result<()> {
    for page_index in thumbnail_window_indices(
        snapshot.current_index,
        snapshot.total_pages,
        THUMBNAIL_CACHE_RADIUS,
    ) {
        render_pdf_page_cached(
            &session.doc,
            &mut state.render_cache,
            RenderRequest {
                page_index,
                width: THUMBNAIL_RENDER_WIDTH,
                purpose: RenderPurpose::Thumbnail,
            },
        )?;
    }

    state.render_cache.retain_presentation_window(
        snapshot.current_index,
        snapshot.total_pages,
        PRESENTATION_CACHE_RADIUS,
    );

    Ok(())
}

fn schedule_presentation_preload(state: Rc<RefCell<AppState>>, snapshot: PageSnapshot) {
    let generation = state.borrow().render_generation;

    Timer::single_shot(Duration::from_millis(0), move || {
        let state = state.borrow();
        if state.render_generation != generation {
            return;
        }

        let Some(current_snapshot) = state.presentation.snapshot() else {
            return;
        };
        if current_snapshot.current_index != snapshot.current_index {
            return;
        }

        enqueue_render_plan_if_missing(
            &state,
            presentation_preload_render_plan(&snapshot, PRESENTATION_CACHE_RADIUS, &state),
        );
    });
}

#[allow(dead_code)]
#[cfg(test)]
fn preload_presentation_window(
    session: &SynchronousPdfSession,
    state: &mut AppState,
    snapshot: &PageSnapshot,
) -> Result<()> {
    for page_index in presentation_preload_order(
        snapshot.current_index,
        snapshot.total_pages,
        PRESENTATION_CACHE_RADIUS,
    ) {
        render_pdf_page_cached(
            &session.doc,
            &mut state.render_cache,
            RenderRequest {
                page_index,
                width: CURRENT_RENDER_WIDTH,
                purpose: RenderPurpose::CurrentSlide,
            },
        )?;
    }

    state.render_cache.retain_presentation_window(
        snapshot.current_index,
        snapshot.total_pages,
        PRESENTATION_CACHE_RADIUS,
    );

    Ok(())
}

fn start_render_event_updates(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) -> Timer {
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, RENDER_EVENT_POLL_INTERVAL, move || {
        let _ = drain_render_events(&windows, &state);
    });
    timer
}

fn start_render_sizing_updates(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) -> Timer {
    let timer = Timer::default();
    let mut samples = render_sizing::SizingSamples::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        let width = window_controller::slide_surface_width(&windows);
        let mut state = state.borrow_mut();
        let Some(policy) = samples.observe(width, state.render_sizing) else { return; };
        state.update_render_sizing(policy, PRESENTATION_CACHE_RADIUS);
        tracing::info!(physical_surface_width = ?width, render_width = policy.current_width(),
            cache_budget_bytes = policy.cache_budget().max_estimated_bytes, "slide render sizing changed");
        if let Some(snapshot) = state.presentation.snapshot() {
            enqueue_visible_page_renders(&state, &snapshot);
            enqueue_render_plan_if_missing(&state, presentation_preload_render_plan(
                &snapshot, PRESENTATION_CACHE_RADIUS, &state));
            apply_snapshot_to_windows(&windows, &state, &snapshot);
        }
    });
    timer
}

fn start_pending_open_status_updates(
    windows: AppWindowRefs,
    state: Rc<RefCell<AppState>>,
) -> Timer {
    let timer = Timer::default();
    timer.start(
        TimerMode::Repeated,
        PENDING_OPEN_STATUS_INTERVAL,
        move || {
            update_pending_open_status(&windows, &state, Instant::now());
        },
    );
    timer
}

fn start_hot_reload_updates(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) -> Timer {
    let timer = Timer::default();
    timer.start(
        TimerMode::Repeated,
        HOT_RELOAD_EVENT_POLL_INTERVAL,
        move || update_hot_reload(&windows, &state, Instant::now()),
    );
    timer
}

fn update_hot_reload(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>, now: Instant) {
    let signals = state
        .borrow()
        .pdf_watcher
        .as_ref()
        .map(PdfWatcher::drain)
        .unwrap_or_default();

    let mut refresh_snapshot = None;
    let mut presenter_message = None;
    {
        let mut state = state.borrow_mut();
        let mut watcher_failed = false;
        for signal in signals {
            if let WatchSignal::Failed(message) = &signal {
                warn!(error = %message, "PDF watcher failed");
                watcher_failed = true;
            } else {
                state.hot_reload.observe(&signal, now);
            }
        }

        if watcher_failed {
            state.pdf_watcher = None;
            state.watcher_recovery.record_failure(now);
            state.hot_reload.clear_success_notice();
            if state.active_document_path.is_some()
                && state.status_text != "Automatic reload is temporarily unavailable."
            {
                state.status_text = "Automatic reload is temporarily unavailable.".to_owned();
                presenter_message = Some(PresenterMessage::new(
                    "Automatic reload is temporarily unavailable.",
                    errors::MessageSeverity::Warning,
                ));
            }
        }

        if state.pdf_watcher.is_none() && state.watcher_recovery.retry_is_due(now) {
            let target = state.hot_reload.target().cloned();
            match create_watcher_for_target(target) {
                Ok(watcher) => {
                    state.pdf_watcher = Some(watcher);
                    state.watcher_recovery.record_success();
                    if state.status_text == "Automatic reload is temporarily unavailable." {
                        state.status_text = "Ready".to_owned();
                        refresh_snapshot = state.presentation.snapshot();
                    }
                }
                Err(error) => {
                    warn!(error = ?error, "failed to recover PDF watcher");
                    state.watcher_recovery.record_failure(now);
                }
            }
        }

        if let Some(revision) = state.hot_reload.due_revision(now) {
            let reload = state.presentation.snapshot().and_then(|snapshot| {
                state
                    .active_document_path
                    .clone()
                    .map(|path| (snapshot.current_index, path))
            });
            if let Some((current_page_index, path)) = reload {
                if state.render_scheduler.is_some() {
                    let session_id = state.render_sessions.begin_reload_session();
                    if state.hot_reload.begin_preparing(session_id, revision, now) {
                        state.render_scheduler.as_ref().unwrap().prepare_reload(
                            session_id,
                            path,
                            current_page_index,
                            CURRENT_RENDER_WIDTH,
                        );
                    }
                }
            }
        }

        if state.hot_reload.mark_preparing_slow(now) {
            state.status_text = "Reloading PDF; the current version remains visible.".to_owned();
            presenter_message = Some(PresenterMessage::new(
                "Reloading PDF; the current version remains visible.",
                errors::MessageSeverity::Info,
            ));
        }

        if state.hot_reload.take_expired_success_notice(now) && state.status_text == "PDF reloaded."
        {
            state.status_text = "Ready".to_owned();
            refresh_snapshot = state.presentation.snapshot();
        }
    }

    if let Some(message) = presenter_message {
        set_presenter_message(&windows.presenter, message);
    }
    if let Some(snapshot) = refresh_snapshot {
        apply_snapshot_to_windows(windows, &state.borrow(), &snapshot);
    }
}

fn create_watcher_for_target(target: Option<WatchTarget>) -> Result<PdfWatcher> {
    let mut watcher = PdfWatcher::new()?;
    watcher.replace_target(target)?;
    Ok(watcher)
}

fn replace_hot_reload_target(state: &mut AppState, path: PathBuf, now: Instant) -> Result<()> {
    let target = WatchTarget::new(path.clone())?;
    state.active_document_path = Some(path);
    state.hot_reload.replace_target(target.clone());

    let result = if let Some(watcher) = state.pdf_watcher.as_mut() {
        watcher.replace_target(Some(target))
    } else {
        create_watcher_for_target(Some(target)).map(|watcher| {
            state.pdf_watcher = Some(watcher);
        })
    };

    match result {
        Ok(()) => {
            state.watcher_recovery.record_success();
            Ok(())
        }
        Err(error) => {
            state.pdf_watcher = None;
            state.watcher_recovery.record_failure(now);
            Err(error)
        }
    }
}

#[cfg(target_os = "linux")]
fn start_file_dialog_result_updates(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) -> Timer {
    let timer = Timer::default();
    timer.start(
        TimerMode::Repeated,
        FILE_DIALOG_RESULT_POLL_INTERVAL,
        move || {
            update_file_dialog_result(&windows, &state);
        },
    );
    timer
}

#[cfg(not(target_os = "linux"))]
fn start_file_dialog_result_updates(
    _windows: AppWindowRefs,
    _state: Rc<RefCell<AppState>>,
) -> Timer {
    Timer::default()
}

#[cfg(target_os = "linux")]
fn update_file_dialog_result(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    let selected_path = {
        let mut state = state.borrow_mut();
        let Some(receiver) = state.file_dialog.result_receiver.as_ref() else {
            return;
        };

        match receiver.try_recv() {
            Ok(path) => {
                finish_file_dialog_request(&mut state);
                path
            }
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                finish_file_dialog_request(&mut state);
                None
            }
        }
    };

    restore_presenter_input_after_transient_ui(windows.clone());

    if let Some(path) = selected_path {
        schedule_open_pdf(
            windows.clone(),
            state.clone(),
            path,
            "failed to open and render PDF",
        );
    }
}

fn update_pending_open_status(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    now: Instant,
) {
    let should_show = {
        let mut state = state.borrow_mut();
        let Some(session_id) = pending_open_session_id(&state) else {
            return;
        };
        mark_pending_open_slow(&mut state, session_id, now, SLOW_OPEN_STATUS_DELAY)
    };

    if should_show {
        set_presenter_message(
            &windows.presenter,
            PresenterMessage::new(SLOW_OPEN_STATUS_TEXT, errors::MessageSeverity::Info),
        );
    }
}

#[derive(Default)]
pub(crate) struct RenderEventDrain {
    pub speaker_notes_loaded: bool,
    pub open_failed: bool,
    pub page_failed: bool,
    pub worker_failed: bool,
}

pub(crate) fn drain_render_events(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
) -> RenderEventDrain {
    let events = {
        let state = state.borrow();
        state
            .render_scheduler
            .as_ref()
            .map(RenderScheduler::drain_events)
            .unwrap_or_default()
    };

    let mut drain = RenderEventDrain::default();
    for event in events {
        match &event {
            RenderEvent::SpeakerNotesLoaded { .. } => drain.speaker_notes_loaded = true,
            RenderEvent::OpenFailed { .. } => drain.open_failed = true,
            RenderEvent::PageFailed { .. } => drain.page_failed = true,
            RenderEvent::WorkerFailed { .. } => drain.worker_failed = true,
            RenderEvent::Opened { .. }
            | RenderEvent::PageRendered { .. }
            | RenderEvent::ReloadPrepared { .. }
            | RenderEvent::ReloadPrepareFailed { .. } => {}
        }
        handle_render_event(windows, state, event);
    }
    drain
}

fn handle_render_event(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>, event: RenderEvent) {
    match event {
        RenderEvent::Opened {
            session_id,
            title,
            page_count,
            status_text,
        } => handle_render_opened(windows, state, session_id, title, page_count, status_text),
        RenderEvent::SpeakerNotesLoaded {
            session_id,
            notes,
            status_text,
        } => handle_speaker_notes_loaded(windows, state, session_id, notes, status_text),
        RenderEvent::OpenFailed {
            session_id,
            message,
        } => handle_render_open_failed(windows, state, session_id, message),
        RenderEvent::PageRendered {
            session_id,
            request,
            page,
            ..
        } => handle_page_rendered(windows, state, session_id, request, page.into()),
        RenderEvent::PageFailed {
            session_id,
            request,
            message,
            ..
        } => handle_page_render_failed(windows, state, session_id, request, message),
        RenderEvent::WorkerFailed {
            session_id,
            message,
        } => handle_render_worker_failed(windows, state, session_id, message),
        RenderEvent::ReloadPrepared {
            session_id,
            title,
            page_count,
            current_page_index,
            render_width,
            current_page,
        } => handle_reload_prepared(
            windows,
            state,
            session_id,
            title,
            page_count,
            current_page_index,
            current_page.into(),
            render_width,
        ),
        RenderEvent::ReloadPrepareFailed {
            session_id,
            message,
            retryable,
        } => handle_reload_prepare_failed(windows, state, session_id, message, retryable),
    }
}

fn handle_reload_prepared(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    session_id: render_scheduler::RenderSessionId,
    title: String,
    page_count: u32,
    current_page_index: u32,
    current_page: RenderedPage,
    render_width: i32,
) {
    let now = Instant::now();
    let snapshot = {
        let mut state = state.borrow_mut();
        let revision = match state.hot_reload.phase() {
            HotReloadPhase::Preparing {
                session_id: pending_session,
                revision,
            } if pending_session == session_id => revision,
            _ => {
                if let Some(scheduler) = state.render_scheduler.as_ref() {
                    scheduler.discard_reload(session_id);
                }
                clear_render_reload_state(&mut state, session_id);
                return;
            }
        };

        if !state.hot_reload.accepts_prepared(session_id, revision) {
            if let Some(scheduler) = state.render_scheduler.as_ref() {
                scheduler.discard_reload(session_id);
            }
            clear_render_reload_state(&mut state, session_id);
            state
                .hot_reload
                .finish_preparing(session_id, revision, true, now);
            return;
        }

        let Some(snapshot) = commit_render_reloaded_state(
            &mut state,
            session_id,
            title,
            page_count,
            current_page_index,
            current_page,
            render_width,
            PRESENTATION_CACHE_RADIUS,
        ) else {
            if let Some(scheduler) = state.render_scheduler.as_ref() {
                scheduler.discard_reload(session_id);
            }
            state
                .hot_reload
                .finish_preparing(session_id, revision, true, now);
            return;
        };

        state
            .hot_reload
            .finish_preparing(session_id, revision, true, now);
        state.hot_reload.start_success_notice(now);
        if let Some(scheduler) = state.render_scheduler.as_ref() {
            scheduler.commit_reload(session_id);
            scheduler.extract_speaker_notes(session_id);
        }
        apply_snapshot_to_windows(windows, &state, &snapshot);
        enqueue_visible_page_renders(&state, &snapshot);
        snapshot
    };

    schedule_presentation_preload(state.clone(), snapshot.clone());
    schedule_thumbnail_render(windows.clone(), state.clone());
}

fn handle_reload_prepare_failed(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    session_id: render_scheduler::RenderSessionId,
    message: String,
    retryable: bool,
) {
    let now = Instant::now();
    let outcome = {
        let mut state = state.borrow_mut();
        let revision = match state.hot_reload.phase() {
            HotReloadPhase::Preparing {
                session_id: pending_session,
                revision,
            } if pending_session == session_id => revision,
            _ => return,
        };
        clear_render_reload_state(&mut state, session_id);
        let outcome = state
            .hot_reload
            .finish_preparing_with_retry(session_id, revision, false, retryable, now);
        if outcome == PreparationOutcome::Failed {
            warn!(error = %message, "failed to reload PDF; keeping previous document");
            state.hot_reload.clear_success_notice();
            state.status_text = "Reload failed; showing the previous version.".to_owned();
        }
        outcome
    };

    if outcome == PreparationOutcome::Failed {
        set_presenter_message(
            &windows.presenter,
            PresenterMessage::new(
                "Reload failed; showing the previous version.",
                errors::MessageSeverity::Warning,
            ),
        );
    }
}

fn handle_render_opened(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    session_id: render_scheduler::RenderSessionId,
    title: String,
    page_count: u32,
    status_text: String,
) {
    let outcome = {
        let mut state = state.borrow_mut();
        let Some(outcome) =
            commit_render_opened_state(&mut state, session_id, title, page_count, status_text)
        else {
            return;
        };
        if let Some(snapshot) = outcome.snapshot.as_ref() {
            apply_snapshot_to_windows(windows, &state, snapshot);
            enqueue_visible_page_renders(&state, snapshot);
        }
        if let Some(scheduler) = state.render_scheduler.as_ref() {
            scheduler.extract_speaker_notes(session_id);
        }
        outcome
    };

    if let Some(path) = outcome.loaded_path {
        let watcher_failed = {
            let mut state = state.borrow_mut();
            match replace_hot_reload_target(&mut state, path.clone(), Instant::now()) {
                Ok(()) => false,
                Err(error) => {
                    warn!(error = ?error, "failed to watch active PDF");
                    state.hot_reload.finish_manual_open_without_replacement();
                    state.status_text = "Automatic reload is temporarily unavailable.".to_owned();
                    true
                }
            }
        };
        if watcher_failed {
            set_presenter_message(
                &windows.presenter,
                PresenterMessage::new(
                    "Automatic reload is temporarily unavailable.",
                    errors::MessageSeverity::Warning,
                ),
            );
        }
        record_recent_pdf(&windows.presenter, state, path);
    }

    if let Some(snapshot) = outcome.snapshot {
        schedule_presentation_preload(state.clone(), snapshot.clone());
        schedule_thumbnail_render(windows.clone(), state.clone());
    }
}

fn handle_speaker_notes_loaded(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    session_id: render_scheduler::RenderSessionId,
    notes: SpeakerNotes,
    status_text: String,
) {
    let notes_ready = status_text == "Ready";
    let mut state = state.borrow_mut();
    let status_text =
        if status_text == "Ready" && state.hot_reload.success_notice_active(Instant::now()) {
            state.status_text.clone()
        } else {
            if status_text != "Ready" {
                state.hot_reload.clear_success_notice();
            }
            status_text
        };
    if !commit_speaker_notes_loaded_state(&mut state, session_id, notes, status_text) {
        return;
    }

    state.control.notes_state = if notes_ready {
        quick_presenter::control::protocol::NotesState::Ready
    } else {
        quick_presenter::control::protocol::NotesState::Failed
    };
    if let Some(snapshot) = state.presentation.snapshot() {
        apply_snapshot_to_windows(windows, &state, &snapshot);
    }
}

fn handle_render_open_failed(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    session_id: render_scheduler::RenderSessionId,
    message: String,
) {
    warn!(error = %message, "failed to open PDF on render worker");
    let presenter_message = {
        let state = state.borrow();
        if state.automatic_reopen == Some(session_id) {
            PresenterMessage::new(
                "Could not reopen the previous PDF. Open a PDF to continue.",
                errors::MessageSeverity::Error,
            )
        } else {
            errors::presenter_error_message(
                &anyhow::anyhow!(message),
                state.diagnostics_log_path.as_deref(),
            )
        }
    };
    let accepted = {
        let mut state = state.borrow_mut();
        commit_render_open_failed_state(&mut state, session_id, presenter_message.text().to_owned())
    };
    if !accepted {
        return;
    }
    state
        .borrow_mut()
        .hot_reload
        .finish_manual_open_without_replacement();
    set_presenter_message(&windows.presenter, presenter_message);
}

fn handle_page_rendered(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    session_id: render_scheduler::RenderSessionId,
    request: RenderRequest,
    page: RenderedPage,
) {
    let outcome = {
        let mut state = state.borrow_mut();
        let Some(outcome) = commit_page_rendered_state(
            &mut state,
            session_id,
            request,
            page,
            PRESENTATION_CACHE_RADIUS,
        ) else {
            return;
        };
        if let Some(snapshot) = outcome.snapshot.as_ref() {
            if request == state.current_slide_request(snapshot.current_index) {
                let (cache_entries, cache_bytes) = state.render_cache.usage();
                tracing::info!(
                    page_index = request.page_index,
                    render_width = request.width,
                    cache_entries,
                    cache_bytes,
                    "visible slide render committed"
                );
            }
            if request.purpose == RenderPurpose::CurrentSlide
                && request.width == CURRENT_RENDER_WIDTH
            {
                // A baseline render discovered page geometry. Upgrade only if it fits.
                enqueue_visible_page_renders(&state, snapshot);
                enqueue_render_plan_if_missing(
                    &state,
                    presentation_preload_render_plan(snapshot, PRESENTATION_CACHE_RADIUS, &state),
                );
            }
            if request.purpose == RenderPurpose::Thumbnail {
                if let Some(presenter) = windows.presenter.upgrade() {
                    sync_thumbnail_model(&presenter, &state, snapshot);
                }
            } else {
                apply_snapshot_to_windows(windows, &state, snapshot);
            }
        }
        outcome
    };

    if let Some(aspect_ratio) = outcome.initial_fit_aspect_ratio {
        fit_slide_window_to_aspect_ratio(windows, aspect_ratio);
    }
}

fn handle_page_render_failed(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    session_id: render_scheduler::RenderSessionId,
    request: RenderRequest,
    message: String,
) {
    warn!(
        page_index = request.page_index,
        purpose = ?request.purpose,
        error = %message,
        "failed to render page on render worker"
    );

    let should_show_message = {
        let mut state = state.borrow_mut();
        let should_show = commit_page_render_failed_state(&mut state, session_id, request);
        if should_show {
            state.hot_reload.clear_success_notice();
        }
        should_show
    };
    if !should_show_message {
        return;
    }
    set_presenter_message(
        &windows.presenter,
        PresenterMessage::new(
            "Could not render this page. Try another PDF or page.",
            errors::MessageSeverity::Error,
        ),
    );
}

fn handle_render_worker_failed(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    session_id: Option<render_scheduler::RenderSessionId>,
    message: String,
) {
    warn!(error = %message, "render worker failed");
    let accepted = {
        let mut state = state.borrow_mut();
        let accepted = commit_render_worker_failed_app_state(&mut state, session_id);
        if accepted {
            state.hot_reload.clear_success_notice();
            state.hot_reload.cancel_pending_reload();
            state.hot_reload.finish_manual_open_without_replacement();
        }
        accepted
    };
    if !accepted {
        return;
    }
    let diagnostic_log_path = state.borrow().diagnostics_log_path.clone();
    set_presenter_message(
        &windows.presenter,
        errors::render_worker_failed_message(diagnostic_log_path.as_deref()),
    );
}

fn commit_render_worker_failed_app_state(
    state: &mut AppState,
    session_id: Option<render_scheduler::RenderSessionId>,
) -> bool {
    let accepted = commit_render_worker_failed_state(state, session_id);
    if accepted {
        if let Some(scheduler) = state.render_scheduler.as_ref() {
            scheduler.try_join_finished_worker();
        }
    }
    accepted
}

fn start_presenter_time_updates(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) -> Timer {
    let timer = Timer::default();
    timer.start(
        TimerMode::Repeated,
        PRESENTER_TIME_UPDATE_INTERVAL,
        move || update_presenter_time_labels(&windows.presenter, &state.borrow().timer),
    );
    timer
}

fn update_presenter_time_labels(presenter: &Weak<PresenterWindow>, timer: &PresentationTimer) {
    if let Some(presenter) = presenter.upgrade() {
        presenter.set_clock_time_label(current_clock_label().into());
        presenter.set_elapsed_time_label(timer.elapsed_label_at(Instant::now()).into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn fitted_slide_window_content_size_ignores_titlebar_compensation() {
        let (_, height) = fitted_slide_window_content_size(16.0 / 9.0, 28.0);

        assert_eq!(height, 576.0);
    }

    #[test]
    fn fitted_slide_window_content_size_keeps_full_height_without_titlebar_compensation() {
        let (_, height) = fitted_slide_window_content_size(16.0 / 9.0, 0.0);

        assert_eq!(height, 576.0);
    }

    #[test]
    fn default_slide_window_size_uses_standard_widescreen_aspect_ratio() {
        let size = default_slide_window_size();

        assert_eq!(size.width, 1024.0);
        assert_eq!(size.height, 576.0);
    }

    #[test]
    fn failed_prepared_session_does_not_replace_current_session() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Existing deck", 3),
            status_text: "Existing status".to_owned(),
            ..AppState::default()
        };
        state.presentation.next_page();
        state.black_screen.set_active(true);
        let original_snapshot = state.presentation.snapshot();
        let original_status = state.status_text.clone();
        let original_generation = state.render_generation;

        let result = commit_prepared_pdf_session(
            &mut state,
            Err(anyhow!("failed to render page 1: bitmap failure")),
        );

        assert!(result.is_err());
        assert_eq!(state.presentation.snapshot(), original_snapshot);
        assert_eq!(state.status_text, original_status);
        assert_eq!(state.render_generation, original_generation);
        assert!(state.black_screen.is_active());
    }

    #[test]
    fn worker_failure_keeps_render_scheduler_handle_until_replacement() {
        let mut state = AppState {
            render_scheduler: Some(RenderScheduler::without_worker_for_test()),
            ..AppState::default()
        };
        let session_id = begin_open_pdf_state(&mut state, PathBuf::from("deck.pdf"));
        commit_render_opened_state(
            &mut state,
            session_id,
            "Deck".to_owned(),
            2,
            "Ready".to_owned(),
        );

        assert!(commit_render_worker_failed_app_state(
            &mut state,
            Some(session_id)
        ));

        assert!(state.render_scheduler.is_some());
        assert_eq!(state.render_sessions.current_session(), None);
    }

    #[test]
    fn open_recreates_missing_render_scheduler() {
        let mut state = AppState::default();

        assert!(ensure_render_scheduler_for_open(&mut state));

        assert!(state.render_scheduler.is_some());
    }

    #[test]
    fn open_replaces_stopped_render_scheduler() {
        let mut state = AppState {
            render_scheduler: Some(RenderScheduler::without_worker_for_test()),
            ..AppState::default()
        };

        assert!(ensure_render_scheduler_for_open(&mut state));

        assert_eq!(
            state
                .render_scheduler
                .as_ref()
                .expect("scheduler should be recreated")
                .lifecycle(),
            RenderWorkerLifecycle::Running
        );
    }

    #[test]
    fn open_does_not_replace_scheduler_while_shutdown_is_pending() {
        let mut state = AppState {
            render_scheduler: Some(RenderScheduler::without_worker_with_lifecycle_for_test(
                RenderWorkerLifecycle::ShutdownRequested,
            )),
            ..AppState::default()
        };

        assert!(!ensure_render_scheduler_for_open(&mut state));
        assert_eq!(
            state
                .render_scheduler
                .as_ref()
                .expect("scheduler should remain present")
                .lifecycle(),
            RenderWorkerLifecycle::ShutdownRequested
        );
    }

    #[test]
    fn file_dialog_request_marks_dialog_open_once() {
        let mut state = AppState::default();

        assert!(begin_file_dialog_request(&mut state));
        assert!(state.file_dialog.open);

        assert!(!begin_file_dialog_request(&mut state));
        assert!(state.file_dialog.open);
    }

    #[test]
    fn finishing_file_dialog_request_clears_open_state() {
        let mut state = AppState::default();

        assert!(begin_file_dialog_request(&mut state));
        finish_file_dialog_request(&mut state);

        assert!(!state.file_dialog.open);
        assert!(begin_file_dialog_request(&mut state));
    }

    #[test]
    fn successful_prepared_session_metadata_replaces_current_session() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Existing deck", 3),
            status_text: "Existing status".to_owned(),
            render_generation: 41,
            ..AppState::default()
        };
        state.presentation.next_page();
        state.black_screen.set_active(true);

        commit_prepared_pdf_session_metadata(
            &mut state,
            SpeakerNotes::empty(),
            PresentationState::open_document("New deck", 2),
            "Ready".to_owned(),
        );

        assert_eq!(state.render_generation, 42);
        assert!(!state.black_screen.is_active());
        assert_eq!(state.status_text, "Ready");

        let snapshot = state
            .presentation
            .snapshot()
            .expect("committed presentation should have a first page");
        assert_eq!(snapshot.current_index, 0);
        assert_eq!(snapshot.page_label, "1 / 2");
        assert_eq!(snapshot.title, "New deck");
    }

    #[test]
    fn recording_recent_pdf_refreshes_visible_recent_menu_snapshot() {
        let first = PathBuf::from("/tmp/first.pdf");
        let second = PathBuf::from("/tmp/second.pdf");
        let state = Rc::new(RefCell::new(AppState {
            recent_files: RecentFiles::from_paths([first.clone(), second.clone()]),
            recent_menu_paths: vec![first.clone(), second.clone()],
            ..AppState::default()
        }));
        let presenter = Weak::<PresenterWindow>::default();

        record_recent_pdf(&presenter, &state, second.clone());

        let state = state.borrow();
        assert_eq!(state.recent_files.paths(), &[second.clone(), first.clone()]);
        assert_eq!(state.recent_menu_paths, vec![second, first]);
    }

    #[test]
    fn clearing_recent_files_clears_saved_paths_and_visible_recent_menu_snapshot() {
        let state = Rc::new(RefCell::new(AppState {
            recent_files: RecentFiles::from_paths([PathBuf::from("/tmp/first.pdf")]),
            recent_menu_paths: vec![PathBuf::from("/tmp/first.pdf")],
            ..AppState::default()
        }));
        let presenter = Weak::<PresenterWindow>::default();

        clear_recent_files(&presenter, &state);

        let state = state.borrow();
        assert!(state.recent_files.is_empty());
        assert!(state.recent_menu_paths.is_empty());
    }
}

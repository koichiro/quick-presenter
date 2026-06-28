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
pub mod errors;
pub mod fullscreen;
pub mod gui_smoke;
pub mod input;
#[cfg(target_os = "macos")]
pub mod macos_window;
pub mod notes;
pub mod pdf;
pub mod presentation;
pub mod recent;
pub mod render_controller;
pub mod render_scheduler;
pub mod rendering;
pub mod session_controller;
pub mod timer;
pub mod view_sync;
pub mod window_controller;
pub mod window_menu;
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

use anyhow::{bail, Result};
use app_metadata::about_metadata;
use app_state::AppState;
use app_state::ThumbnailState;
use cli::{help_text, parse_startup_options, GuiSmokeOptions, StartupRequest};
use clock::current_clock_label;
use errors::PresenterMessage;
use input::PresentationCommand;
use notes::SpeakerNotes;
use pdf::PdfDocumentState;
use presentation::PageSnapshot;
use presentation::PresentationState;
use recent::{default_recent_file_store, RecentFileStore, RecentFiles};
use render_controller::{
    enqueue_render_plan_if_missing, presentation_preload_render_plan, thumbnail_render_plan,
    thumbnail_visible_range_render_plan, visible_page_render_plan,
};
use render_controller::{CURRENT_RENDER_WIDTH, PREVIEW_RENDER_WIDTH, THUMBNAIL_RENDER_WIDTH};
use render_scheduler::{RenderEvent, RenderScheduler, RenderWorkerLifecycle};
use rendering::presentation_preload_order;
use rendering::RenderCache;
use rendering::{thumbnail_window_indices, RenderPurpose, RenderRequest, RenderedPage};
use session_controller::{
    apply_session_command, begin_open_pdf_state, commit_page_render_failed_state,
    commit_page_rendered_state, commit_render_open_failed_state, commit_render_opened_state,
    commit_render_worker_failed_state, commit_speaker_notes_loaded_state, mark_pending_open_slow,
    pending_open_session_id, SLOW_OPEN_STATUS_TEXT,
};
use slint::{CloseRequestResponse, ComponentHandle, Timer, TimerMode, Weak};
use timer::PresentationTimer;
use tracing::warn;
use tracing_subscriber::EnvFilter;
use view_sync::{
    apply_opening_state_to_windows, apply_snapshot_to_windows, set_presenter_message,
    thumbnail_current_row_index, thumbnail_model,
};
use view_sync::{black_slide_image, presenter_status_text};
use window_controller::{
    apply_macos_slide_window_chrome, fitted_slide_window_size, hide_slide_window,
    set_slide_fullscreen, show_presenter_window, show_slide_window,
    slide_titlebar_compensation_height, start_slide_chrome_sync, sync_slide_chrome, AppWindowRefs,
    AppWindows,
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

fn main() -> Result<()> {
    let startup_request = parse_startup_options(std::env::args_os().skip(1))?;
    if startup_request == StartupRequest::Help {
        print!("{}", help_text(&startup_program_name()));
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let StartupRequest::Run(startup_options) = startup_request else {
        unreachable!("help requests return before app startup");
    };
    if let Some(path) = startup_options.smoke_open_pdf_path {
        return smoke_open_pdf(path);
    }
    if let Some(options) = startup_options.gui_smoke {
        return run_gui_smoke(options);
    }

    let windows = AppWindows::new()?;
    configure_linux_desktop_identity()?;
    configure_shortcut_modifiers(&windows);
    let recent_store = default_recent_file_store();
    let recent_files = load_recent_files(recent_store.as_ref());
    let recent_menu_paths = recent_files.paths().to_vec();
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState {
        render_scheduler: Some(RenderScheduler::start()),
        recent_files,
        recent_menu_paths,
        recent_store,
        ..AppState::default()
    }));

    wire_callbacks(&windows, windows.refs(), state.clone());
    let _presenter_time_timer = start_presenter_time_updates(windows.refs(), state.clone());
    let _render_event_timer = start_render_event_updates(windows.refs(), state.clone());
    let _pending_open_status_timer =
        start_pending_open_status_updates(windows.refs(), state.clone());
    let _file_dialog_result_timer = start_file_dialog_result_updates(windows.refs(), state.clone());
    update_recent_file_menu(&windows.refs().presenter, &state.borrow().recent_files);
    apply_app_metadata(&windows.presenter);

    windows.apply_initial_positions();
    windows.slide.show()?;
    let window_refs = windows.refs();
    apply_macos_slide_window_chrome(&window_refs);
    sync_slide_chrome(&window_refs);
    windows.presenter.show()?;
    set_application_icon();
    remove_macos_native_about_menu_item();
    let _slide_chrome_sync_timer = start_slide_chrome_sync(window_refs);

    if let Some(path) = startup_options.pdf_path {
        load_startup_pdf(&windows.refs(), &state, path);
    }

    slint::run_event_loop()?;
    Ok(())
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

    let doc = PdfDocumentState::open(path)?;
    let page_count = doc.page_count();
    if page_count == 0 {
        bail!("smoke-open-pdf requires a PDF with at least one page");
    }

    let image = doc.render_page(0, SMOKE_RENDER_WIDTH)?;
    println!(
        "Smoke open PDF succeeded: title=\"{}\" pages={} first_page={}x{}",
        doc.title(),
        page_count,
        image.size().width,
        image.size().height
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
struct RenderedPages {
    current: RenderedPage,
    next: Option<RenderedPage>,
}

#[allow(dead_code)]
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
struct SynchronousPdfSession {
    doc: PdfDocumentState,
}

#[allow(dead_code)]
struct CommittedPdfSession {
    session: SynchronousPdfSession,
    loaded_path: PathBuf,
    snapshot: Option<PageSnapshot>,
    initial_slide_aspect_ratio: Option<f32>,
    initial_pages: Option<RenderedPages>,
}

fn wire_callbacks(windows: &AppWindows, refs: AppWindowRefs, state: Rc<RefCell<AppState>>) {
    let app = &windows.presenter;

    wire_presenter_close_request(windows);

    let window_refs = refs.clone();
    let state_for_open = state.clone();
    app.on_open_pdf(move || {
        request_pdf_file_open(window_refs.clone(), state_for_open.clone());
    });

    wire_recent_file_callbacks(app, refs.clone(), state.clone());

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
    let state_for_toggle = state.clone();
    app.on_toggle_slide_fullscreen(move || {
        let mut state = state_for_toggle.borrow_mut();
        let fullscreen = state.fullscreen.toggle_slide_fullscreen();
        set_slide_fullscreen(&window_refs, fullscreen);
    });

    wire_window_menu_callbacks(app, refs.clone(), state.clone());

    let window_refs = refs.clone();
    let state_for_presenter_exit = state.clone();
    app.on_exit_slide_fullscreen(move || {
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
    windows.slide.on_toggle_slide_fullscreen(move || {
        let mut state = state_for_slide_toggle.borrow_mut();
        let fullscreen = state.fullscreen.toggle_slide_fullscreen();
        set_slide_fullscreen(&window_refs, fullscreen);
    });

    let window_refs = refs;
    let state_for_slide_exit = state;
    windows.slide.on_exit_fullscreen(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_slide_exit,
            PresentationCommand::ExitSlideFullscreen,
        );
    });
}

fn wire_presenter_close_request(windows: &AppWindows) {
    windows
        .presenter
        .window()
        .on_close_requested(move || match slint::quit_event_loop() {
            Ok(()) => CloseRequestResponse::KeepWindowShown,
            Err(err) => {
                warn!(error = ?err, "failed to quit event loop from presenter close request");
                CloseRequestResponse::KeepWindowShown
            }
        });
}

fn apply_app_metadata(app: &PresenterWindow) {
    let metadata = about_metadata();

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
fn prepare_pdf_session(path: PathBuf) -> Result<PreparedPdfSession> {
    prepare_pdf_session_with_initial_render(path, render_pages)
}

#[allow(dead_code)]
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

fn commit_prepared_pdf_session(
    state: &mut AppState,
    prepared: Result<PreparedPdfSession>,
) -> Result<CommittedPdfSession> {
    let prepared = prepared?;
    Ok(commit_prepared_pdf_session_state(state, prepared))
}

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
    _presenter: &Weak<PresenterWindow>,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
) {
    let mut state = state.borrow_mut();
    state.recent_files.add(path);

    if let Some(store) = state.recent_store.as_ref() {
        if let Err(err) = store.save(&state.recent_files) {
            warn!(error = ?err, "failed to save recent files");
        }
    }
}

fn clear_recent_files(_presenter: &Weak<PresenterWindow>, state: &Rc<RefCell<AppState>>) {
    let mut state = state.borrow_mut();
    state.recent_files.clear();
    state.recent_menu_paths.clear();

    if let Some(store) = state.recent_store.as_ref() {
        if let Err(err) = store.save(&state.recent_files) {
            warn!(error = ?err, "failed to save cleared recent files");
        }
    }
}

fn update_recent_file_menu(presenter: &Weak<PresenterWindow>, recent_files: &RecentFiles) {
    let labels: Vec<String> = recent_files
        .paths()
        .iter()
        .map(|path| path.display().to_string())
        .collect();

    update_recent_file_menu_labels(presenter, labels);
}

fn update_recent_file_menu_labels(presenter: &Weak<PresenterWindow>, labels: Vec<String>) {
    let Some(presenter) = presenter.upgrade() else {
        return;
    };

    let has_recent_files = !labels.is_empty();
    presenter.set_has_recent_files(has_recent_files);

    let mut labels = labels.into_iter();
    let label_0 = labels
        .next()
        .unwrap_or_else(|| "No Recent Files".to_owned());
    let label_1 = labels.next().unwrap_or_default();
    let label_2 = labels.next().unwrap_or_default();
    let label_3 = labels.next().unwrap_or_default();
    let label_4 = labels.next().unwrap_or_default();

    presenter.set_recent_file_label_0(label_0.into());
    presenter.set_recent_file_label_1(label_1.clone().into());
    presenter.set_recent_file_label_2(label_2.clone().into());
    presenter.set_recent_file_label_3(label_3.clone().into());
    presenter.set_recent_file_label_4(label_4.clone().into());
    presenter.set_recent_file_0_enabled(has_recent_files);
    presenter.set_recent_file_1_enabled(!label_1.is_empty());
    presenter.set_recent_file_2_enabled(!label_2.is_empty());
    presenter.set_recent_file_3_enabled(!label_3.is_empty());
    presenter.set_recent_file_4_enabled(!label_4.is_empty());
}

fn fit_slide_window_to_aspect_ratio(windows: &AppWindowRefs, aspect_ratio: f32) {
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
        let (width, height) = (size.width, size.height);
        slide.set_slide_window_width(width);
        slide.set_slide_window_height(height);
        slide.window().set_size(size);
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
    enqueue_render_plan_if_missing(state, visible_page_render_plan(snapshot));
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
            presentation_preload_render_plan(&snapshot, PRESENTATION_CACHE_RADIUS),
        );
    });
}

#[allow(dead_code)]
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
            RenderEvent::Opened { .. } | RenderEvent::PageRendered { .. } => {}
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
    let mut state = state.borrow_mut();
    if !commit_speaker_notes_loaded_state(&mut state, session_id, notes, status_text) {
        return;
    }

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
    let accepted = {
        let mut state = state.borrow_mut();
        commit_render_open_failed_state(&mut state, session_id)
    };
    if !accepted {
        return;
    }
    set_presenter_message(
        &windows.presenter,
        PresenterMessage::new(
            "Could not open PDF. Choose another file.",
            errors::MessageSeverity::Error,
        ),
    );
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
        commit_page_render_failed_state(&mut state, session_id, request)
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
        commit_render_worker_failed_app_state(&mut state, session_id)
    };
    if !accepted {
        return;
    }
    set_presenter_message(
        &windows.presenter,
        PresenterMessage::new(
            "Rendering stopped. Open the PDF again.",
            errors::MessageSeverity::Error,
        ),
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
    fn recording_recent_pdf_keeps_visible_recent_menu_snapshot_stable() {
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
        assert_eq!(state.recent_files.paths(), &[second, first.clone()]);
        assert_eq!(
            state.recent_menu_paths,
            vec![first, PathBuf::from("/tmp/second.pdf")]
        );
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

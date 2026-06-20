pub mod app_metadata;
pub mod app_state;
pub mod aspect;
pub mod black_screen;
pub mod cli;
pub mod clock;
pub mod errors;
pub mod fullscreen;
pub mod input;
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

use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::{bail, Result};
use app_metadata::about_metadata;
use app_state::AppState;
#[cfg(test)]
use app_state::ThumbnailState;
use cli::{help_text, parse_startup_options, StartupRequest};
use clock::current_clock_label;
use errors::PresenterMessage;
use input::PresentationCommand;
use notes::SpeakerNotes;
use pdf::PdfDocumentState;
use presentation::PageSnapshot;
#[cfg(test)]
use presentation::PresentationState;
use recent::{default_recent_file_store, RecentFileStore, RecentFiles};
use render_controller::{
    enqueue_render_plan_if_missing, presentation_preload_render_plan, thumbnail_render_plan,
    visible_page_render_plan,
};
#[cfg(test)]
use render_controller::{CURRENT_RENDER_WIDTH, PREVIEW_RENDER_WIDTH, THUMBNAIL_RENDER_WIDTH};
use render_scheduler::{RenderEvent, RenderScheduler};
#[cfg(test)]
use rendering::RenderCache;
#[cfg(test)]
use rendering::RenderPurpose;
#[cfg(test)]
use rendering::{presentation_preload_order, thumbnail_window_indices};
use rendering::{RenderRequest, RenderedPage};
use session_controller::{
    apply_session_command, begin_open_pdf_state, commit_page_render_failed_state,
    commit_page_rendered_state, commit_render_open_failed_state, commit_render_opened_state,
};
use slint::{CloseRequestResponse, ComponentHandle, Timer, TimerMode, Weak};
use timer::PresentationTimer;
use tracing::warn;
use tracing_subscriber::EnvFilter;
use view_sync::{
    apply_opening_state_to_windows, apply_snapshot_to_windows, presenter_page_index,
    recent_file_menu_labels, set_presenter_message, thumbnail_model,
};
#[cfg(test)]
use view_sync::{black_slide_image, presenter_status_text};
use window_controller::{
    apply_macos_slide_window_chrome, fitted_slide_window_size, hide_presenter_window,
    hide_slide_window, set_slide_fullscreen, show_presenter_window, show_slide_window,
    slide_titlebar_compensation_height, start_slide_chrome_sync, sync_slide_chrome, AppWindowRefs,
    AppWindows,
};

slint::include_modules!();

const PRESENTATION_CACHE_RADIUS: u32 = 2;
const THUMBNAIL_CACHE_RADIUS: u32 = 8;
const RENDER_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(16);
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

    let windows = AppWindows::new()?;
    let recent_store = default_recent_file_store();
    let recent_files = load_recent_files(recent_store.as_ref());
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState {
        render_scheduler: Some(RenderScheduler::start()),
        recent_files,
        recent_store,
        ..AppState::default()
    }));

    wire_callbacks(&windows, windows.refs(), state.clone());
    let _presenter_time_timer = start_presenter_time_updates(windows.refs(), state.clone());
    let _render_event_timer = start_render_event_updates(windows.refs(), state.clone());
    update_recent_file_menu(&windows.refs().presenter, &state.borrow().recent_files);
    apply_app_metadata(&windows.presenter);

    windows.apply_initial_positions();
    windows.slide.show()?;
    apply_macos_slide_window_chrome();
    let window_refs = windows.refs();
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

fn startup_program_name() -> String {
    std::env::args_os()
        .next()
        .and_then(|arg| {
            let path = PathBuf::from(arg);
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "qp".to_string())
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
    if running_from_macos_app_bundle() {
        return;
    }

    // Slint/muda always adds the native App > About item on macOS when a MenuBar exists.
    // Quick Presenter uses its own Help > About dialog so PDFium licensing is visible.
    remove_macos_native_about_menu_item_now();
    Timer::single_shot(
        Duration::from_millis(0),
        remove_macos_native_about_menu_item_now,
    );
    Timer::single_shot(
        Duration::from_millis(250),
        remove_macos_native_about_menu_item_now,
    );
    Timer::single_shot(
        Duration::from_millis(1000),
        remove_macos_native_about_menu_item_now,
    );
}

#[cfg(target_os = "macos")]
fn remove_macos_native_about_menu_item_now() {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::MainThreadMarker;

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(main_thread);
    let Some(main_menu) = app.mainMenu() else {
        return;
    };

    for index in 0..main_menu.numberOfItems() {
        let Some(menu_item) = main_menu.itemAtIndex(index) else {
            continue;
        };
        let Some(submenu) = menu_item.submenu() else {
            continue;
        };
        let Some(first_item) = submenu.itemAtIndex(0) else {
            continue;
        };

        if first_item.title().to_string().starts_with("About ") && is_macos_app_menu(&submenu) {
            submenu.removeItemAtIndex(0);
            if submenu.numberOfItems() > 0 {
                submenu.removeItemAtIndex(0);
            }
            return;
        }
    }
}

#[cfg(target_os = "macos")]
fn is_macos_app_menu(menu: &objc2_app_kit::NSMenu) -> bool {
    let mut has_services = false;
    let mut has_hide = false;

    for index in 0..menu.numberOfItems() {
        let Some(item) = menu.itemAtIndex(index) else {
            continue;
        };
        let title = item.title().to_string();
        has_services |= title == "Services";
        has_hide |= title.starts_with("Hide ");
    }

    has_services && has_hide
}

#[cfg(target_os = "macos")]
fn running_from_macos_app_bundle() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };

    exe.parent()
        .and_then(std::path::Path::file_name)
        .and_then(|name| name.to_str())
        == Some("MacOS")
        && exe
            .parent()
            .and_then(std::path::Path::parent)
            .and_then(std::path::Path::file_name)
            .and_then(|name| name.to_str())
            == Some("Contents")
}

#[cfg(not(target_os = "macos"))]
fn remove_macos_native_about_menu_item() {}

#[cfg(test)]
#[allow(dead_code)]
struct RenderedPages {
    current: RenderedPage,
    next: Option<RenderedPage>,
}

#[cfg(test)]
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

#[cfg(test)]
#[allow(dead_code)]
struct CommittedPdfSession {
    loaded_path: PathBuf,
    snapshot: Option<PageSnapshot>,
    initial_slide_aspect_ratio: Option<f32>,
    initial_pages: Option<RenderedPages>,
}

fn wire_callbacks(windows: &AppWindows, refs: AppWindowRefs, state: Rc<RefCell<AppState>>) {
    let app = &windows.presenter;

    wire_presenter_close_request(windows, refs.clone(), state.clone());

    let window_refs = refs.clone();
    let state_for_open = state.clone();
    app.on_open_pdf(move || {
        if let Some(path) = pick_pdf_file() {
            schedule_open_pdf(
                window_refs.clone(),
                state_for_open.clone(),
                path,
                "failed to open and render PDF",
            );
        }
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

fn wire_presenter_close_request(
    windows: &AppWindows,
    refs: AppWindowRefs,
    state: Rc<RefCell<AppState>>,
) {
    windows.presenter.window().on_close_requested(move || {
        close_presentation_session_from_presenter(&refs, &state);
        CloseRequestResponse::HideWindow
    });
}

fn close_presentation_session_from_presenter(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
) {
    {
        let mut state = state.borrow_mut();
        state.window_menu.close_presentation_session();
        state.fullscreen.exit_slide_fullscreen();
    }

    set_slide_fullscreen(windows, false);
    hide_slide_window(windows);
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
    let state_for_slide_toggle = state.clone();
    presenter.on_hide_presenter_window(move || {
        schedule_window_menu_action(
            window_refs.clone(),
            state_for_slide_toggle.clone(),
            |windows, state| {
                hide_presenter_window_from_menu(&windows, &state);
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

fn hide_presenter_window_from_menu(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    let show_slide_first = {
        let mut state = state.borrow_mut();
        let show_slide_first = !state.window_menu.slide_visible();
        if show_slide_first {
            state.window_menu.set_slide_visible(true);
        }
        state.window_menu.set_presenter_visible(false);
        show_slide_first
    };

    if show_slide_first {
        show_slide_window(windows);
    }
    hide_presenter_window(windows);
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

fn pick_pdf_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_title("Open PDF")
        .pick_file()
}

fn load_startup_pdf(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>, path: PathBuf) {
    begin_open_pdf(windows, state, path);
}

fn begin_open_pdf(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>, path: PathBuf) {
    let title = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("PDF")
        .to_owned();

    let session_id = {
        let mut state = state.borrow_mut();
        let session_id = begin_open_pdf_state(&mut state, path.clone());
        if let Some(scheduler) = state.render_scheduler.as_ref() {
            scheduler.open(session_id, path);
        }
        session_id
    };

    let _ = session_id;
    apply_opening_state_to_windows(windows, &title);
}

#[cfg(test)]
#[allow(dead_code)]
fn open_and_render(
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

    Ok(())
}

#[cfg(test)]
#[allow(dead_code)]
fn prepare_pdf_session(path: PathBuf) -> Result<PreparedPdfSession> {
    prepare_pdf_session_with_initial_render(path, render_pages)
}

#[cfg(test)]
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

    state.pdf = Some(doc);
    state.render_cache = render_cache;
    commit_prepared_pdf_session_metadata(state, notes, presentation, status_text);

    CommittedPdfSession {
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
        .recent_files
        .paths()
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
    presenter.set_recent_file_labels(recent_file_menu_labels(labels));
}

fn fit_slide_window_to_aspect_ratio(windows: &AppWindowRefs, aspect_ratio: f32) {
    if let Some(slide) = windows.slide.upgrade() {
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
    state: &mut AppState,
    snapshot: &PageSnapshot,
) -> Result<()> {
    let Some(doc) = state.pdf.as_ref() else {
        anyhow::bail!("missing open PDF for current presentation");
    };
    let rendered = render_pages(doc, &mut state.render_cache, snapshot)?;
    state.render_cache.retain_presentation_window(
        snapshot.current_index,
        snapshot.total_pages,
        PRESENTATION_CACHE_RADIUS,
    );

    apply_rendered_pages_to_windows(windows, state, snapshot, rendered);

    Ok(())
}

#[cfg(test)]
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
        presenter.set_current_page_index(presenter_page_index(snapshot.current_index));
        presenter.set_thumbnails(thumbnail_model(
            &state.thumbnails,
            &state.render_cache,
            snapshot.current_index,
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

#[cfg(test)]
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

#[cfg(test)]
#[allow(dead_code)]
fn render_pdf_page_cached(
    doc: &PdfDocumentState,
    cache: &mut RenderCache,
    request: RenderRequest,
) -> Result<RenderedPage> {
    cache.get_or_render(request, |request| {
        let aspect_ratio = doc.page_aspect_ratio(request.page_index)?;
        Ok(RenderedPage {
            image: doc.render_page(request.page_index, request.width)?,
            aspect_ratio,
            estimated_bytes: rendering::estimated_render_bytes(request.width, aspect_ratio),
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
            presenter.set_current_page_index(presenter_page_index(snapshot.current_index));
            presenter.set_thumbnails(thumbnail_model(
                &state.thumbnails,
                &state.render_cache,
                snapshot.current_index,
            ));
        }
    });
}

#[cfg(test)]
#[allow(dead_code)]
fn render_thumbnail_window(state: &mut AppState, snapshot: &PageSnapshot) -> Result<()> {
    let Some(doc) = state.pdf.as_ref() else {
        return Ok(());
    };

    for page_index in thumbnail_window_indices(
        snapshot.current_index,
        snapshot.total_pages,
        THUMBNAIL_CACHE_RADIUS,
    ) {
        render_pdf_page_cached(
            doc,
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

#[cfg(test)]
#[allow(dead_code)]
fn preload_presentation_window(state: &mut AppState, snapshot: &PageSnapshot) -> Result<()> {
    let Some(doc) = state.pdf.as_ref() else {
        return Ok(());
    };

    for page_index in presentation_preload_order(
        snapshot.current_index,
        snapshot.total_pages,
        PRESENTATION_CACHE_RADIUS,
    ) {
        render_pdf_page_cached(
            doc,
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
        drain_render_events(&windows, &state)
    });
    timer
}

fn drain_render_events(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    let events = {
        let state = state.borrow();
        state
            .render_scheduler
            .as_ref()
            .map(RenderScheduler::drain_events)
            .unwrap_or_default()
    };

    for event in events {
        handle_render_event(windows, state, event);
    }
}

fn handle_render_event(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>, event: RenderEvent) {
    match event {
        RenderEvent::Opened {
            session_id,
            title,
            page_count,
            notes,
            status_text,
        } => handle_render_opened(
            windows,
            state,
            session_id,
            title,
            page_count,
            notes,
            status_text,
        ),
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
    }
}

fn handle_render_opened(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    session_id: render_scheduler::RenderSessionId,
    title: String,
    page_count: u32,
    notes: SpeakerNotes,
    status_text: String,
) {
    let outcome = {
        let mut state = state.borrow_mut();
        let Some(outcome) = commit_render_opened_state(
            &mut state,
            session_id,
            title,
            page_count,
            notes,
            status_text,
        ) else {
            return;
        };
        if let Some(snapshot) = outcome.snapshot.as_ref() {
            apply_snapshot_to_windows(windows, &state, snapshot);
            enqueue_visible_page_renders(&state, snapshot);
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
            apply_snapshot_to_windows(windows, &state, snapshot);
        }
        outcome
    };

    if let Some(aspect_ratio) = outcome.fit_aspect_ratio {
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
    fn fitted_slide_window_content_size_subtracts_titlebar_compensation_from_height() {
        let (_, height) = fitted_slide_window_content_size(16.0 / 9.0, 28.0);

        assert_eq!(height, 548.0);
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
        assert!(state.pdf.is_none());
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
}

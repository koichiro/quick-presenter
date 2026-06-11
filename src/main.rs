pub mod app_metadata;
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
pub mod rendering;
pub mod timer;
pub mod window_menu;

use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::{bail, Result};
use app_metadata::about_metadata;
use aspect::fitted_logical_size_within;
use black_screen::BlackScreenState;
use cli::parse_startup_options;
use clock::current_clock_label;
use errors::{presenter_error_message, speaker_notes_warning, PresenterMessage};
use fullscreen::FullscreenState;
use input::{apply_presentation_command, PresentationCommand};
use notes::SpeakerNotes;
use pdf::PdfDocumentState;
use presentation::{PageSnapshot, PresentationState};
use recent::{default_recent_file_store, RecentFileStore, RecentFiles};
use rendering::{
    presentation_preload_order, RenderCache, RenderPurpose, RenderRequest, RenderedPage,
};
use slint::{
    ComponentHandle, LogicalPosition, LogicalSize, ModelRc, Rgba8Pixel, SharedPixelBuffer,
    SharedString, Timer, TimerMode, VecModel, Weak,
};
use timer::{leaves_first_page, PresentationTimer};
use tracing::{error, warn};
use tracing_subscriber::EnvFilter;
use window_menu::WindowMenuState;

slint::include_modules!();

const CURRENT_RENDER_WIDTH: i32 = 1600;
const PREVIEW_RENDER_WIDTH: i32 = 600;
const PRESENTATION_CACHE_RADIUS: u32 = 2;
const PRESENTER_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(80.0, 80.0);
const SLIDE_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(180.0, 140.0);
const SLIDE_WINDOW_MAX_WIDTH: f32 = 1024.0;
const SLIDE_WINDOW_MAX_HEIGHT: f32 = 720.0;
const PRESENTER_TIME_UPDATE_INTERVAL: Duration = Duration::from_millis(250);
const FILE_MENU_ACTION_DELAY: Duration = Duration::from_millis(150);
const WINDOW_MENU_ACTION_DELAY: Duration = Duration::from_millis(150);
const PRESENTER_WINDOW_TITLE: &str = "Quick Presenter";
const SLIDE_WINDOW_TITLE: &str = "Quick Presenter - Slide";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let startup_options = parse_startup_options(std::env::args_os().skip(1))?;
    let windows = AppWindows::new()?;
    let recent_store = default_recent_file_store();
    let recent_files = load_recent_files(recent_store.as_ref());
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState {
        recent_files,
        recent_store,
        ..AppState::default()
    }));

    wire_callbacks(&windows, windows.refs(), state.clone());
    let _presenter_time_timer = start_presenter_time_updates(windows.refs(), state.clone());
    update_recent_file_menu(&windows.refs().presenter, &state.borrow().recent_files);
    apply_app_metadata(&windows.presenter);

    windows.apply_initial_positions();
    windows.slide.show()?;
    windows.presenter.show()?;
    set_application_icon();
    remove_macos_native_about_menu_item();

    if let Some(path) = startup_options.pdf_path {
        load_startup_pdf(&windows.refs(), &state, path);
    }

    slint::run_event_loop()?;
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

struct AppWindows {
    presenter: PresenterWindow,
    slide: SlideWindow,
}

impl AppWindows {
    fn new() -> Result<Self> {
        Ok(Self {
            presenter: PresenterWindow::new()?,
            slide: SlideWindow::new()?,
        })
    }

    fn refs(&self) -> AppWindowRefs {
        AppWindowRefs {
            presenter: self.presenter.as_weak(),
            slide: self.slide.as_weak(),
        }
    }

    fn apply_initial_positions(&self) {
        self.presenter
            .window()
            .set_position(PRESENTER_WINDOW_POSITION);
        self.slide.window().set_position(SLIDE_WINDOW_POSITION);
    }
}

#[derive(Clone)]
struct AppWindowRefs {
    presenter: Weak<PresenterWindow>,
    slide: Weak<SlideWindow>,
}

#[derive(Default)]
struct AppState {
    black_screen: BlackScreenState,
    fullscreen: FullscreenState,
    pdf: Option<PdfDocumentState>,
    render_cache: RenderCache,
    render_generation: u64,
    notes: SpeakerNotes,
    presentation: PresentationState,
    timer: PresentationTimer,
    window_menu: WindowMenuState,
    recent_files: RecentFiles,
    recent_store: Option<RecentFileStore>,
    status_text: String,
}

struct RenderedPages {
    current: RenderedPage,
    next: Option<RenderedPage>,
}

fn wire_callbacks(windows: &AppWindows, refs: AppWindowRefs, state: Rc<RefCell<AppState>>) {
    let app = &windows.presenter;

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
        if let Err(err) = open_and_render(&windows, &state, path) {
            error!(error = ?err, "{}", error_context);
            set_presenter_message(&windows.presenter, presenter_error_message(&err));
        }
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
    if command == PresentationCommand::ExitSlideFullscreen {
        exit_slide_fullscreen(windows, state);
        return;
    }

    if command == PresentationCommand::ToggleBlackScreen {
        toggle_black_screen(windows, state);
        return;
    }

    let preload_snapshot = {
        let mut state = state.borrow_mut();
        let before = state.presentation.snapshot();
        apply_presentation_command(&mut state.presentation, command);
        let after = state.presentation.snapshot();
        maybe_start_elapsed_timer(command, before.as_ref(), after.as_ref(), &mut state.timer);

        if let Some(snapshot) = after {
            match render_into_windows(windows, &mut state, &snapshot) {
                Ok(()) => Some(snapshot),
                Err(err) => {
                    error!(error = ?err, "failed to render presentation page");
                    set_presenter_message(&windows.presenter, presenter_error_message(&err));
                    None
                }
            }
        } else {
            None
        }
    };

    if let Some(snapshot) = preload_snapshot {
        schedule_presentation_preload(state.clone(), snapshot);
    }
}

fn toggle_black_screen(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    let preload_snapshot = {
        let mut state = state.borrow_mut();
        state.black_screen.toggle();

        if let Some(snapshot) = state.presentation.snapshot() {
            match render_into_windows(windows, &mut state, &snapshot) {
                Ok(()) => Some(snapshot),
                Err(err) => {
                    error!(error = ?err, "failed to render presentation page");
                    set_presenter_message(&windows.presenter, presenter_error_message(&err));
                    None
                }
            }
        } else {
            None
        }
    };

    if let Some(snapshot) = preload_snapshot {
        schedule_presentation_preload(state.clone(), snapshot);
    }
}

fn maybe_start_elapsed_timer(
    command: PresentationCommand,
    before: Option<&PageSnapshot>,
    after: Option<&PageSnapshot>,
    timer: &mut PresentationTimer,
) {
    if command != PresentationCommand::ExitSlideFullscreen
        && !timer.is_running()
        && leaves_first_page(
            before.map(|snapshot| snapshot.current_index),
            after.map(|snapshot| snapshot.current_index),
        )
    {
        timer.start(Instant::now());
    }
}

fn exit_slide_fullscreen(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    let mut state = state.borrow_mut();
    let fullscreen = state.fullscreen.exit_slide_fullscreen();
    set_slide_fullscreen(windows, fullscreen);
}

fn set_slide_fullscreen(windows: &AppWindowRefs, fullscreen: bool) {
    if let Some(slide) = windows.slide.upgrade() {
        slide.window().set_fullscreen(fullscreen);
    }

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_slide_fullscreen(fullscreen);
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

fn show_presenter_window(windows: &AppWindowRefs) {
    #[cfg(target_os = "macos")]
    if show_macos_window(PRESENTER_WINDOW_TITLE) {
        return;
    }

    if let Some(presenter) = windows.presenter.upgrade() {
        if let Err(err) = presenter.show() {
            warn!(error = ?err, "failed to show presenter window");
        }
    }
}

fn show_slide_window(windows: &AppWindowRefs) {
    #[cfg(target_os = "macos")]
    if show_macos_window(SLIDE_WINDOW_TITLE) {
        return;
    }

    if let Some(slide) = windows.slide.upgrade() {
        if let Err(err) = slide.show() {
            warn!(error = ?err, "failed to show slide window");
        }
    }
}

fn hide_presenter_window(windows: &AppWindowRefs) {
    #[cfg(target_os = "macos")]
    if hide_macos_window(PRESENTER_WINDOW_TITLE) {
        return;
    }

    if let Some(presenter) = windows.presenter.upgrade() {
        if let Err(err) = presenter.hide() {
            warn!(error = ?err, "failed to hide presenter window");
        }
    }
}

fn hide_slide_window(windows: &AppWindowRefs) {
    #[cfg(target_os = "macos")]
    if hide_macos_window(SLIDE_WINDOW_TITLE) {
        return;
    }

    if let Some(slide) = windows.slide.upgrade() {
        if let Err(err) = slide.hide() {
            warn!(error = ?err, "failed to hide slide window");
        }
    }
}

#[cfg(target_os = "macos")]
fn show_macos_window(title: &str) -> bool {
    with_macos_window(title, |app, window| {
        app.activate();
        window.deminiaturize(None);
        window.makeKeyAndOrderFront(None);
    })
}

#[cfg(target_os = "macos")]
fn hide_macos_window(title: &str) -> bool {
    with_macos_window(title, |_, window| {
        window.orderOut(None);
    })
}

#[cfg(target_os = "macos")]
fn with_macos_window(
    title: &str,
    action: impl FnOnce(&objc2_app_kit::NSApplication, &objc2_app_kit::NSWindow),
) -> bool {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::MainThreadMarker;

    let Some(main_thread) = MainThreadMarker::new() else {
        return false;
    };

    let app = NSApplication::sharedApplication(main_thread);
    let windows = app.windows();

    for window in windows.iter() {
        if window.title().to_string() == title {
            action(&app, &window);
            return true;
        }
    }

    false
}

fn pick_pdf_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_title("Open PDF")
        .pick_file()
}

fn load_startup_pdf(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>, path: PathBuf) {
    if let Err(err) = open_and_render(windows, state, path) {
        error!(error = ?err, "failed to open startup PDF");
        set_presenter_message(&windows.presenter, presenter_error_message(&err));
    }
}

fn open_and_render(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
) -> Result<()> {
    let loaded_path = path.clone();
    let doc = PdfDocumentState::open(path)?;
    let (notes, status_text) = match doc.speaker_notes() {
        Ok(notes) => (notes, "Ready".to_owned()),
        Err(err) => {
            warn!(error = ?err, "speaker notes unavailable");
            (
                SpeakerNotes::empty(),
                speaker_notes_warning(&err).text().to_owned(),
            )
        }
    };
    let presentation = PresentationState::open_document(doc.title(), doc.page_count());
    let snapshot = presentation.snapshot();
    let initial_slide_aspect_ratio = snapshot
        .as_ref()
        .map(|snapshot| doc.page_aspect_ratio(snapshot.current_index))
        .transpose()?;

    {
        let mut state = state.borrow_mut();
        state.pdf = Some(doc);
        state.render_cache.clear();
        state.render_generation = state.render_generation.wrapping_add(1);
        state.notes = notes;
        state.presentation = presentation;
        state.black_screen.set_active(false);
        state.timer.reset();
        state.status_text = status_text;
    }

    if let Some(aspect_ratio) = initial_slide_aspect_ratio {
        fit_slide_window_to_aspect_ratio(windows, aspect_ratio);
    }

    if let Some(snapshot) = snapshot {
        {
            let mut state = state.borrow_mut();
            render_into_windows(windows, &mut state, &snapshot)?;
        }
        schedule_presentation_preload(state.clone(), snapshot);
    }

    record_recent_pdf(&windows.presenter, state, loaded_path);

    Ok(())
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

    let label_model = ModelRc::new(Rc::new(VecModel::from(
        labels
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));

    presenter.set_has_recent_files(has_recent_files);
    presenter.set_recent_file_labels(label_model);
}

fn fit_slide_window_to_aspect_ratio(windows: &AppWindowRefs, aspect_ratio: f32) {
    if let Some(slide) = windows.slide.upgrade() {
        let size = fitted_logical_size_within(
            SLIDE_WINDOW_MAX_WIDTH,
            SLIDE_WINDOW_MAX_HEIGHT,
            aspect_ratio,
        );
        let width = size.width.round();
        let height = size.height.round();
        slide.set_slide_window_width(width);
        slide.set_slide_window_height(height);
        slide.window().set_size(LogicalSize::new(width, height));
    }
}

fn render_into_windows(
    windows: &AppWindowRefs,
    state: &mut AppState,
    snapshot: &PageSnapshot,
) -> Result<()> {
    let Some(doc) = state.pdf.as_ref() else {
        bail!("missing open PDF for current presentation");
    };
    let rendered = render_pages(doc, &mut state.render_cache, snapshot)?;
    state.render_cache.retain_presentation_window(
        snapshot.current_index,
        snapshot.total_pages,
        PRESENTATION_CACHE_RADIUS,
    );

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

    Ok(())
}

fn presenter_status_text(state: &AppState) -> String {
    if state.black_screen.is_active() {
        "Black screen active. Audience slide is hidden.".to_owned()
    } else {
        state.status_text.clone()
    }
}

fn black_slide_image() -> slint::Image {
    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 9;

    let mut pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
    for alpha in pixels.iter_mut().skip(3).step_by(4) {
        *alpha = 255;
    }
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&pixels, WIDTH, HEIGHT);
    slint::Image::from_rgba8(buffer)
}

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

fn render_pdf_page_cached(
    doc: &PdfDocumentState,
    cache: &mut RenderCache,
    request: RenderRequest,
) -> Result<RenderedPage> {
    cache.get_or_render(request, |request| {
        Ok(RenderedPage {
            image: doc.render_page(request.page_index, request.width)?,
            aspect_ratio: doc.page_aspect_ratio(request.page_index)?,
        })
    })
}

fn schedule_presentation_preload(state: Rc<RefCell<AppState>>, snapshot: PageSnapshot) {
    let generation = state.borrow().render_generation;

    Timer::single_shot(Duration::from_millis(0), move || {
        let mut state = state.borrow_mut();
        if state.render_generation != generation {
            return;
        }

        let Some(current_snapshot) = state.presentation.snapshot() else {
            return;
        };
        if current_snapshot.current_index != snapshot.current_index {
            return;
        }

        if let Err(err) = preload_presentation_window(&mut state, &snapshot) {
            warn!(error = ?err, "failed to preload nearby presentation pages");
        }
    });
}

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

fn set_presenter_message(weak: &Weak<PresenterWindow>, message: PresenterMessage) {
    if let Some(app) = weak.upgrade() {
        app.set_status_text(message.text().into());
    }
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

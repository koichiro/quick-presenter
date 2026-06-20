use std::time::Duration;

use anyhow::Result;
use slint::{ComponentHandle, LogicalPosition, LogicalSize, Timer, TimerMode, Weak};
use tracing::warn;

use crate::{PresenterWindow, SlideWindow};

const PRESENTER_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(80.0, 80.0);
const SLIDE_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(180.0, 140.0);
const SLIDE_TITLEBAR_COMPENSATION_HEIGHT: f32 = 28.0;
#[cfg(target_os = "macos")]
const SLIDE_WINDOW_TITLE: &str = "Quick Presenter - Slide";
#[cfg(target_os = "macos")]
const PRESENTER_WINDOW_TITLE: &str = "Quick Presenter";

pub struct AppWindows {
    pub presenter: PresenterWindow,
    pub slide: SlideWindow,
}

impl AppWindows {
    pub fn new() -> Result<Self> {
        Ok(Self {
            presenter: PresenterWindow::new()?,
            slide: SlideWindow::new()?,
        })
    }

    pub fn refs(&self) -> AppWindowRefs {
        AppWindowRefs {
            presenter: self.presenter.as_weak(),
            slide: self.slide.as_weak(),
        }
    }

    pub fn apply_initial_positions(&self) {
        self.presenter
            .window()
            .set_position(PRESENTER_WINDOW_POSITION);
        self.slide.window().set_position(SLIDE_WINDOW_POSITION);
    }
}

#[derive(Clone)]
pub struct AppWindowRefs {
    pub presenter: Weak<PresenterWindow>,
    pub slide: Weak<SlideWindow>,
}

#[cfg(target_os = "macos")]
pub fn apply_macos_slide_window_chrome() {
    apply_macos_slide_window_chrome_now();
    Timer::single_shot(
        Duration::from_millis(0),
        apply_macos_slide_window_chrome_now,
    );
    Timer::single_shot(
        Duration::from_millis(250),
        apply_macos_slide_window_chrome_now,
    );
    Timer::single_shot(
        Duration::from_millis(1000),
        apply_macos_slide_window_chrome_now,
    );
}

#[cfg(target_os = "macos")]
fn apply_macos_slide_window_chrome_now() {
    use objc2_app_kit::{NSWindowStyleMask, NSWindowTitleVisibility};

    with_macos_window(SLIDE_WINDOW_TITLE, |_, window| {
        window.setStyleMask(window.styleMask() | NSWindowStyleMask::FullSizeContentView);
        window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        window.setTitlebarAppearsTransparent(true);
    });
}

#[cfg(not(target_os = "macos"))]
pub fn apply_macos_slide_window_chrome() {}

pub fn start_slide_chrome_sync(windows: AppWindowRefs) -> Timer {
    let timer = Timer::default();
    #[cfg(target_os = "macos")]
    {
        timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
            sync_slide_chrome(&windows);
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = windows;

    timer
}

pub fn sync_slide_chrome(windows: &AppWindowRefs) {
    #[cfg(target_os = "macos")]
    apply_macos_slide_window_chrome();

    if let Some(slide) = windows.slide.upgrade() {
        let compensation_height =
            slide_titlebar_compensation_height(slide.window().is_fullscreen());
        slide.set_titlebar_compensation_height(compensation_height);
    }
}

pub fn slide_titlebar_compensation_height(fullscreen: bool) -> f32 {
    if cfg!(target_os = "macos") && !fullscreen {
        SLIDE_TITLEBAR_COMPENSATION_HEIGHT
    } else {
        0.0
    }
}

pub fn set_slide_fullscreen(windows: &AppWindowRefs, fullscreen: bool) {
    if let Some(slide) = windows.slide.upgrade() {
        slide.window().set_fullscreen(fullscreen);
        slide.set_titlebar_compensation_height(slide_titlebar_compensation_height(fullscreen));
    }

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_slide_fullscreen(fullscreen);
    }
}

pub fn show_presenter_window(windows: &AppWindowRefs) {
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

pub fn show_slide_window(windows: &AppWindowRefs) {
    #[cfg(target_os = "macos")]
    if show_macos_window(SLIDE_WINDOW_TITLE) {
        apply_macos_slide_window_chrome();
        return;
    }

    if let Some(slide) = windows.slide.upgrade() {
        if let Err(err) = slide.show() {
            warn!(error = ?err, "failed to show slide window");
        } else {
            apply_macos_slide_window_chrome();
        }
    }
}

pub fn hide_presenter_window(windows: &AppWindowRefs) {
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

pub fn hide_slide_window(windows: &AppWindowRefs) {
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

pub fn fitted_slide_window_size(
    max_width: f32,
    max_height: f32,
    aspect_ratio: f32,
    titlebar_compensation_height: f32,
) -> LogicalSize {
    let size = crate::aspect::fitted_logical_size_within(max_width, max_height, aspect_ratio);
    let width = size.width.round();
    let height = (size.height.round() - titlebar_compensation_height).max(1.0);

    LogicalSize::new(width, height)
}

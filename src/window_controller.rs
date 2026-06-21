#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::time::Duration;

use anyhow::Result;
#[cfg(target_os = "macos")]
use slint::TimerMode;
use slint::{ComponentHandle, LogicalPosition, LogicalSize, Timer, Weak};
use tracing::warn;

use crate::{PresenterWindow, SlideWindow};

const PRESENTER_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(80.0, 80.0);
const SLIDE_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(180.0, 140.0);
#[cfg(any(target_os = "macos", test))]
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
pub fn apply_macos_slide_window_chrome(windows: &AppWindowRefs) {
    let Some(slide) = windows.slide.upgrade() else {
        return;
    };

    if should_apply_slide_chrome(slide.window().is_fullscreen()) {
        crate::macos_window::apply_slide_chrome(slide.window(), SLIDE_WINDOW_TITLE);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn apply_macos_slide_window_chrome(_windows: &AppWindowRefs) {}

#[cfg(target_os = "windows")]
pub fn apply_windows_slide_window_chrome(windows: &AppWindowRefs) {
    let Some(slide) = windows.slide.upgrade() else {
        return;
    };

    crate::windows_window::apply_slide_chrome(slide.window());
}

#[cfg(not(target_os = "windows"))]
pub fn apply_windows_slide_window_chrome(_windows: &AppWindowRefs) {}

pub fn start_slide_chrome_sync(windows: AppWindowRefs) -> Timer {
    let timer = Timer::default();
    #[cfg(target_os = "macos")]
    {
        timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
            sync_slide_chrome(&windows);
        });
    }
    #[cfg(target_os = "windows")]
    {
        let first_retry = windows.clone();
        let second_retry = windows.clone();
        Timer::single_shot(Duration::from_millis(0), move || {
            sync_slide_chrome(&first_retry);
        });
        Timer::single_shot(Duration::from_millis(250), move || {
            sync_slide_chrome(&second_retry);
        });
        Timer::single_shot(Duration::from_millis(1000), move || {
            sync_slide_chrome(&windows);
        });
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = windows;

    timer
}

pub fn sync_slide_chrome(windows: &AppWindowRefs) {
    #[cfg(target_os = "macos")]
    apply_macos_slide_window_chrome(windows);
    #[cfg(target_os = "windows")]
    apply_windows_slide_window_chrome(windows);

    if let Some(slide) = windows.slide.upgrade() {
        let compensation_height =
            slide_titlebar_compensation_height(slide.window().is_fullscreen());
        slide.set_titlebar_compensation_height(compensation_height);
    }
}

pub fn slide_titlebar_compensation_height(fullscreen: bool) -> f32 {
    if cfg!(target_os = "macos") && !fullscreen {
        #[cfg(target_os = "macos")]
        {
            return SLIDE_TITLEBAR_COMPENSATION_HEIGHT;
        }
    }

    0.0
}

pub fn should_apply_slide_chrome(fullscreen: bool) -> bool {
    cfg!(target_os = "macos") && !fullscreen
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
    if let Some(presenter) = windows.presenter.upgrade() {
        if crate::macos_window::show_window(presenter.window(), PRESENTER_WINDOW_TITLE) {
            return;
        }
    }

    if let Some(presenter) = windows.presenter.upgrade() {
        if let Err(err) = presenter.show() {
            warn!(error = ?err, "failed to show presenter window");
        }
    }
}

pub fn show_slide_window(windows: &AppWindowRefs) {
    #[cfg(target_os = "macos")]
    if let Some(slide) = windows.slide.upgrade() {
        if crate::macos_window::show_window(slide.window(), SLIDE_WINDOW_TITLE) {
            apply_macos_slide_window_chrome(windows);
            return;
        }
    }

    if let Some(slide) = windows.slide.upgrade() {
        if let Err(err) = slide.show() {
            warn!(error = ?err, "failed to show slide window");
        } else {
            sync_slide_chrome(windows);
        }
    }
}

pub fn hide_presenter_window(windows: &AppWindowRefs) {
    #[cfg(target_os = "macos")]
    if let Some(presenter) = windows.presenter.upgrade() {
        if crate::macos_window::hide_window(presenter.window(), PRESENTER_WINDOW_TITLE) {
            return;
        }
    }

    if let Some(presenter) = windows.presenter.upgrade() {
        if let Err(err) = presenter.hide() {
            warn!(error = ?err, "failed to hide presenter window");
        }
    }
}

pub fn hide_slide_window(windows: &AppWindowRefs) {
    #[cfg(target_os = "macos")]
    if let Some(slide) = windows.slide.upgrade() {
        if crate::macos_window::hide_window(slide.window(), SLIDE_WINDOW_TITLE) {
            return;
        }
    }

    if let Some(slide) = windows.slide.upgrade() {
        if let Err(err) = slide.hide() {
            warn!(error = ?err, "failed to hide slide window");
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_chrome_is_applied_only_while_windowed() {
        assert_eq!(should_apply_slide_chrome(false), cfg!(target_os = "macos"));
        assert!(!should_apply_slide_chrome(true));
    }

    #[test]
    fn titlebar_compensation_matches_slide_chrome_state() {
        assert_eq!(
            slide_titlebar_compensation_height(false),
            if cfg!(target_os = "macos") {
                SLIDE_TITLEBAR_COMPENSATION_HEIGHT
            } else {
                0.0
            }
        );
        assert_eq!(slide_titlebar_compensation_height(true), 0.0);
    }
}

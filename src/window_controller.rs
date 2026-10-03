use std::{cell::Cell, rc::Rc, time::Duration};

use anyhow::Result;
use slint::winit_030::{winit, WinitWindowAccessor};
#[cfg(target_os = "macos")]
use slint::TimerMode;
use slint::{ComponentHandle, LogicalPosition, LogicalSize, Timer, Weak};
use tracing::{info, warn};

use crate::{PresenterWindow, SlideWindow};

const PRESENTER_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(80.0, 80.0);
const SLIDE_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(180.0, 140.0);
const DISPLAY_SWAP_VERIFY_DELAYS: [Duration; 3] = [
    Duration::from_millis(150),
    Duration::from_millis(500),
    Duration::from_millis(1000),
];
const DISPLAY_SWAP_FAILURE_MESSAGE: &str = "Displays changed; check presenter and slide placement.";
#[cfg(any(target_os = "linux", test))]
const LINUX_PRESENTER_INPUT_RECOVERY_DELAYS: [Duration; 3] = [
    Duration::from_millis(0),
    Duration::from_millis(50),
    Duration::from_millis(200),
];
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowRole {
    Presenter,
    Slide,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplaySwapOutcome {
    Applied,
    Busy,
    SameDisplay,
    HiddenWindow,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PixelPoint {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PixelSize {
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DisplayGeometry {
    id: usize,
    origin: PixelPoint,
    size: PixelSize,
    scale_factor: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WindowGeometry {
    display_id: usize,
    outer_position: PixelPoint,
    outer_size: PixelSize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowPlacement {
    display_id: usize,
    outer_position: PixelPoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DisplaySwapPlan {
    presenter: WindowPlacement,
    slide: WindowPlacement,
}

#[derive(Clone)]
struct NativeWindowSnapshot {
    monitor: winit::monitor::MonitorHandle,
    outer_position: PixelPoint,
    outer_size: PixelSize,
    fullscreen: bool,
}

#[derive(Clone)]
struct NativeSwapPlan {
    presenter_target: winit::monitor::MonitorHandle,
    presenter_position: PixelPoint,
    slide_target: winit::monitor::MonitorHandle,
    slide_position: PixelPoint,
    slide_fullscreen: bool,
}

pub fn request_display_swap(
    windows: &AppWindowRefs,
    initiated_by: WindowRole,
    active: Rc<Cell<bool>>,
) -> DisplaySwapOutcome {
    if active.get() {
        return DisplaySwapOutcome::Busy;
    }

    let plan = match build_native_swap_plan(windows) {
        Ok(plan) => plan,
        Err(outcome) => return outcome,
    };

    active.set(true);
    if !apply_native_swap(windows, &plan) {
        active.set(false);
        return DisplaySwapOutcome::Unavailable;
    }

    restore_window_focus(windows, initiated_by);
    schedule_display_swap_verification(windows.clone(), plan, initiated_by, active, 0);
    DisplaySwapOutcome::Applied
}

fn build_native_swap_plan(windows: &AppWindowRefs) -> Result<NativeSwapPlan, DisplaySwapOutcome> {
    let presenter = windows
        .presenter
        .upgrade()
        .ok_or(DisplaySwapOutcome::Unavailable)?;
    let slide = windows
        .slide
        .upgrade()
        .ok_or(DisplaySwapOutcome::Unavailable)?;

    let presenter_snapshot =
        native_window_snapshot(presenter.window()).ok_or(DisplaySwapOutcome::Unavailable)?;
    let slide_snapshot =
        native_window_snapshot(slide.window()).ok_or(DisplaySwapOutcome::Unavailable)?;

    if presenter_snapshot.monitor == slide_snapshot.monitor {
        return Err(DisplaySwapOutcome::SameDisplay);
    }

    let monitors = presenter
        .window()
        .with_winit_window(|window| window.available_monitors().collect::<Vec<_>>())
        .ok_or(DisplaySwapOutcome::Unavailable)?;
    if monitors.len() < 2 {
        return Err(DisplaySwapOutcome::SameDisplay);
    }

    let presenter_display_id = monitors
        .iter()
        .position(|monitor| *monitor == presenter_snapshot.monitor)
        .ok_or(DisplaySwapOutcome::Unavailable)?;
    let slide_display_id = monitors
        .iter()
        .position(|monitor| *monitor == slide_snapshot.monitor)
        .ok_or(DisplaySwapOutcome::Unavailable)?;
    let displays = monitors
        .iter()
        .enumerate()
        .map(|(id, monitor)| display_geometry(id, monitor))
        .collect::<Option<Vec<_>>>()
        .ok_or(DisplaySwapOutcome::Unavailable)?;

    let plan = plan_display_swap(
        WindowGeometry {
            display_id: presenter_display_id,
            outer_position: presenter_snapshot.outer_position,
            outer_size: presenter_snapshot.outer_size,
        },
        WindowGeometry {
            display_id: slide_display_id,
            outer_position: slide_snapshot.outer_position,
            outer_size: slide_snapshot.outer_size,
        },
        &displays,
    )
    .ok_or(DisplaySwapOutcome::Unavailable)?;

    Ok(NativeSwapPlan {
        presenter_target: monitors[plan.presenter.display_id].clone(),
        presenter_position: plan.presenter.outer_position,
        slide_target: monitors[plan.slide.display_id].clone(),
        slide_position: plan.slide.outer_position,
        slide_fullscreen: slide_snapshot.fullscreen,
    })
}

fn native_window_snapshot(window: &slint::Window) -> Option<NativeWindowSnapshot> {
    window
        .with_winit_window(|native| {
            if native_window_placement_is_unsupported(native) {
                return None;
            }

            let monitor = native.current_monitor()?;
            let fullscreen = native.fullscreen().is_some();
            let position = native
                .outer_position()
                .ok()
                .or_else(|| fullscreen.then(|| monitor.position()))?;
            let size = native.outer_size();

            Some(NativeWindowSnapshot {
                monitor,
                outer_position: PixelPoint {
                    x: position.x,
                    y: position.y,
                },
                outer_size: PixelSize {
                    width: size.width,
                    height: size.height,
                },
                fullscreen,
            })
        })
        .flatten()
}

#[cfg(target_os = "linux")]
fn native_window_placement_is_unsupported(window: &winit::window::Window) -> bool {
    use winit::platform::wayland::WindowExtWayland;

    window.xdg_toplevel().is_some()
}

#[cfg(not(target_os = "linux"))]
fn native_window_placement_is_unsupported(_window: &winit::window::Window) -> bool {
    false
}

fn display_geometry(id: usize, monitor: &winit::monitor::MonitorHandle) -> Option<DisplayGeometry> {
    let origin = monitor.position();
    let size = monitor.size();
    let scale_factor = monitor.scale_factor();
    if size.width == 0 || size.height == 0 || !scale_factor.is_finite() || scale_factor <= 0.0 {
        return None;
    }

    Some(DisplayGeometry {
        id,
        origin: PixelPoint {
            x: origin.x,
            y: origin.y,
        },
        size: PixelSize {
            width: size.width,
            height: size.height,
        },
        scale_factor,
    })
}

fn plan_display_swap(
    presenter: WindowGeometry,
    slide: WindowGeometry,
    displays: &[DisplayGeometry],
) -> Option<DisplaySwapPlan> {
    if presenter.display_id == slide.display_id || displays.len() < 2 {
        return None;
    }

    let presenter_source = find_display(displays, presenter.display_id)?;
    let slide_source = find_display(displays, slide.display_id)?;

    Some(DisplaySwapPlan {
        presenter: WindowPlacement {
            display_id: slide_source.id,
            outer_position: map_window_position(presenter, presenter_source, slide_source)?,
        },
        slide: WindowPlacement {
            display_id: presenter_source.id,
            outer_position: map_window_position(slide, slide_source, presenter_source)?,
        },
    })
}

fn find_display(displays: &[DisplayGeometry], id: usize) -> Option<DisplayGeometry> {
    displays.iter().copied().find(|display| display.id == id)
}

fn map_window_position(
    window: WindowGeometry,
    source: DisplayGeometry,
    target: DisplayGeometry,
) -> Option<PixelPoint> {
    if source.size.width == 0
        || source.size.height == 0
        || target.size.width == 0
        || target.size.height == 0
        || !source.scale_factor.is_finite()
        || !target.scale_factor.is_finite()
        || source.scale_factor <= 0.0
        || target.scale_factor <= 0.0
    {
        return None;
    }

    let source_width = f64::from(source.size.width);
    let source_height = f64::from(source.size.height);
    let center_x = f64::from(window.outer_position.x - source.origin.x)
        + f64::from(window.outer_size.width) / 2.0;
    let center_y = f64::from(window.outer_position.y - source.origin.y)
        + f64::from(window.outer_size.height) / 2.0;
    let relative_x = (center_x / source_width).clamp(0.0, 1.0);
    let relative_y = (center_y / source_height).clamp(0.0, 1.0);
    let scale_ratio = target.scale_factor / source.scale_factor;
    let target_window_width = f64::from(window.outer_size.width) * scale_ratio;
    let target_window_height = f64::from(window.outer_size.height) * scale_ratio;
    let target_x = f64::from(target.origin.x) + relative_x * f64::from(target.size.width)
        - target_window_width / 2.0;
    let target_y = f64::from(target.origin.y) + relative_y * f64::from(target.size.height)
        - target_window_height / 2.0;

    Some(PixelPoint {
        x: clamp_axis(
            target_x,
            target.origin.x,
            target.size.width,
            target_window_width,
        ),
        y: clamp_axis(
            target_y,
            target.origin.y,
            target.size.height,
            target_window_height,
        ),
    })
}

fn clamp_axis(value: f64, origin: i32, display_size: u32, window_size: f64) -> i32 {
    let minimum = f64::from(origin);
    let maximum = if window_size >= f64::from(display_size) {
        minimum
    } else {
        minimum + f64::from(display_size) - window_size
    };
    value.clamp(minimum, maximum).round() as i32
}

fn apply_native_swap(windows: &AppWindowRefs, plan: &NativeSwapPlan) -> bool {
    let Some(presenter) = windows.presenter.upgrade() else {
        return false;
    };
    let Some(slide) = windows.slide.upgrade() else {
        return false;
    };
    if !presenter.window().has_winit_window() || !slide.window().has_winit_window() {
        return false;
    }

    let presenter_applied = presenter
        .window()
        .with_winit_window(|window| {
            window.set_outer_position(winit::dpi::PhysicalPosition::new(
                plan.presenter_position.x,
                plan.presenter_position.y,
            ));
        })
        .is_some();
    let slide_applied = slide
        .window()
        .with_winit_window(|window| {
            if plan.slide_fullscreen {
                window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
                    plan.slide_target.clone(),
                ))));
            } else {
                window.set_outer_position(winit::dpi::PhysicalPosition::new(
                    plan.slide_position.x,
                    plan.slide_position.y,
                ));
            }
        })
        .is_some();

    presenter_applied && slide_applied
}

fn schedule_display_swap_verification(
    windows: AppWindowRefs,
    plan: NativeSwapPlan,
    initiated_by: WindowRole,
    active: Rc<Cell<bool>>,
    attempt: usize,
) {
    let delay = DISPLAY_SWAP_VERIFY_DELAYS[attempt];
    Timer::single_shot(delay, move || {
        if display_swap_matches(&windows, &plan) {
            active.set(false);
            sync_slide_chrome(&windows);
            restore_window_focus(&windows, initiated_by);
            info!("presenter and slide displays switched");
            return;
        }

        let next_attempt = attempt + 1;
        if next_attempt < DISPLAY_SWAP_VERIFY_DELAYS.len()
            && target_monitors_are_available(&windows, &plan)
            && apply_native_swap(&windows, &plan)
        {
            schedule_display_swap_verification(windows, plan, initiated_by, active, next_attempt);
            return;
        }

        warn!("display swap did not settle before the verification deadline");
        recover_windows_to_available_displays(&windows, plan.slide_fullscreen);
        sync_slide_chrome(&windows);
        restore_window_focus(&windows, initiated_by);
        if let Some(presenter) = windows.presenter.upgrade() {
            presenter.set_status_text(DISPLAY_SWAP_FAILURE_MESSAGE.into());
        }
        active.set(false);
    });
}

fn display_swap_matches(windows: &AppWindowRefs, plan: &NativeSwapPlan) -> bool {
    window_is_on_monitor(&windows.presenter, &plan.presenter_target)
        && window_is_on_monitor(&windows.slide, &plan.slide_target)
}

fn window_is_on_monitor<T: ComponentHandle>(
    window: &Weak<T>,
    expected: &winit::monitor::MonitorHandle,
) -> bool {
    let Some(window) = window.upgrade() else {
        return false;
    };

    window
        .window()
        .with_winit_window(|native| {
            native
                .current_monitor()
                .is_some_and(|monitor| monitor == *expected)
        })
        .unwrap_or(false)
}

fn target_monitors_are_available(windows: &AppWindowRefs, plan: &NativeSwapPlan) -> bool {
    let Some(presenter) = windows.presenter.upgrade() else {
        return false;
    };

    presenter
        .window()
        .with_winit_window(|window| {
            let available = window.available_monitors().collect::<Vec<_>>();
            available.contains(&plan.presenter_target) && available.contains(&plan.slide_target)
        })
        .unwrap_or(false)
}

fn recover_windows_to_available_displays(windows: &AppWindowRefs, slide_fullscreen: bool) {
    let Some(presenter) = windows.presenter.upgrade() else {
        return;
    };
    let fallback = presenter.window().with_winit_window(|window| {
        window
            .primary_monitor()
            .or_else(|| window.available_monitors().next())
    });
    let Some(Some(fallback)) = fallback else {
        return;
    };

    if !window_has_available_monitor(presenter.window()) {
        let origin = fallback.position();
        let _ = presenter.window().with_winit_window(|window| {
            window.set_outer_position(winit::dpi::PhysicalPosition::new(
                origin.x.saturating_add(40),
                origin.y.saturating_add(40),
            ));
        });
    }

    let Some(slide) = windows.slide.upgrade() else {
        return;
    };
    if !window_has_available_monitor(slide.window()) {
        let origin = fallback.position();
        let _ = slide.window().with_winit_window(|window| {
            if slide_fullscreen {
                window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
                    fallback.clone(),
                ))));
            } else {
                window.set_outer_position(winit::dpi::PhysicalPosition::new(
                    origin.x.saturating_add(80),
                    origin.y.saturating_add(80),
                ));
            }
        });
    }
}

fn window_has_available_monitor(window: &slint::Window) -> bool {
    window
        .with_winit_window(|native| {
            let current = native.current_monitor();
            current.is_some_and(|current| native.available_monitors().any(|item| item == current))
        })
        .unwrap_or(false)
}

fn restore_window_focus(windows: &AppWindowRefs, role: WindowRole) {
    match role {
        WindowRole::Presenter => {
            if let Some(presenter) = windows.presenter.upgrade() {
                let _ = presenter
                    .window()
                    .with_winit_window(|window| window.focus_window());
                presenter.invoke_focus();
            }
        }
        WindowRole::Slide => {
            if let Some(slide) = windows.slide.upgrade() {
                let _ = slide
                    .window()
                    .with_winit_window(|window| window.focus_window());
                slide.invoke_focus();
            }
        }
    }
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

#[cfg(target_os = "linux")]
pub fn restore_presenter_input_after_transient_ui(windows: AppWindowRefs) {
    for delay in presenter_input_recovery_delays() {
        let windows = windows.clone();
        Timer::single_shot(delay, move || restore_presenter_input_now(&windows));
    }
}

#[cfg(not(target_os = "linux"))]
pub fn restore_presenter_input_after_transient_ui(_windows: AppWindowRefs) {}

#[cfg(target_os = "linux")]
fn restore_presenter_input_now(windows: &AppWindowRefs) {
    show_presenter_window(windows);

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.invoke_focus();
    }
}

#[cfg(any(target_os = "linux", test))]
fn presenter_input_recovery_delays() -> [Duration; 3] {
    LINUX_PRESENTER_INPUT_RECOVERY_DELAYS
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
    _titlebar_compensation_height: f32,
) -> LogicalSize {
    let size = crate::aspect::fitted_logical_size_within(max_width, max_height, aspect_ratio);
    let width = size.width.round();
    let height = size.height.round().max(1.0);

    LogicalSize::new(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(
        id: usize,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> DisplayGeometry {
        DisplayGeometry {
            id,
            origin: PixelPoint { x, y },
            size: PixelSize { width, height },
            scale_factor,
        }
    }

    fn window(display_id: usize, x: i32, y: i32, width: u32, height: u32) -> WindowGeometry {
        WindowGeometry {
            display_id,
            outer_position: PixelPoint { x, y },
            outer_size: PixelSize { width, height },
        }
    }

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

    #[test]
    fn presenter_input_recovery_retries_immediately_and_after_transient_ui_focus_settles() {
        assert_eq!(
            presenter_input_recovery_delays(),
            [
                Duration::from_millis(0),
                Duration::from_millis(50),
                Duration::from_millis(200)
            ]
        );
    }

    #[test]
    fn display_swap_exchanges_only_the_two_occupied_displays() {
        let displays = [
            display(0, 0, 0, 1000, 1000, 1.0),
            display(1, 1000, 0, 2000, 1000, 1.0),
            display(2, 3000, 0, 1000, 1000, 1.0),
        ];

        let plan = plan_display_swap(
            window(0, 250, 250, 500, 500),
            window(2, 3250, 250, 500, 500),
            &displays,
        )
        .unwrap();

        assert_eq!(plan.presenter.display_id, 2);
        assert_eq!(
            plan.presenter.outer_position,
            PixelPoint { x: 3250, y: 250 }
        );
        assert_eq!(plan.slide.display_id, 0);
        assert_eq!(plan.slide.outer_position, PixelPoint { x: 250, y: 250 });
    }

    #[test]
    fn display_swap_preserves_normalized_window_center() {
        let displays = [
            display(0, 0, 0, 1000, 1000, 1.0),
            display(1, 1000, 0, 2000, 1000, 1.0),
        ];

        let plan = plan_display_swap(
            window(0, 250, 250, 500, 500),
            window(1, 1500, 250, 1000, 500),
            &displays,
        )
        .unwrap();

        assert_eq!(
            plan.presenter.outer_position,
            PixelPoint { x: 1750, y: 250 }
        );
        assert_eq!(plan.slide.outer_position, PixelPoint { x: 0, y: 250 });
    }

    #[test]
    fn display_swap_accounts_for_scale_factor_changes() {
        let displays = [
            display(0, 0, 0, 1000, 1000, 1.0),
            display(1, 1000, 0, 2000, 2000, 2.0),
        ];

        let plan = plan_display_swap(
            window(0, 250, 250, 500, 500),
            window(1, 1500, 500, 1000, 1000),
            &displays,
        )
        .unwrap();

        assert_eq!(
            plan.presenter.outer_position,
            PixelPoint { x: 1500, y: 500 }
        );
        assert_eq!(plan.slide.outer_position, PixelPoint { x: 250, y: 250 });
    }

    #[test]
    fn display_swap_clamps_windows_to_destination_bounds() {
        let displays = [
            display(0, 0, 0, 1000, 1000, 1.0),
            display(1, 1000, 0, 800, 600, 1.0),
        ];

        let plan = plan_display_swap(
            window(0, -400, -300, 900, 700),
            window(1, 1750, 550, 200, 100),
            &displays,
        )
        .unwrap();

        assert_eq!(plan.presenter.outer_position, PixelPoint { x: 1000, y: 0 });
        assert_eq!(plan.slide.outer_position, PixelPoint { x: 800, y: 900 });
    }

    #[test]
    fn display_swap_aligns_oversized_window_to_destination_origin() {
        let displays = [
            display(0, 0, 0, 1000, 1000, 1.0),
            display(1, 1000, 100, 600, 400, 1.0),
        ];

        let plan = plan_display_swap(
            window(0, 0, 0, 900, 700),
            window(1, 1100, 200, 300, 200),
            &displays,
        )
        .unwrap();

        assert_eq!(
            plan.presenter.outer_position,
            PixelPoint { x: 1000, y: 100 }
        );
    }

    #[test]
    fn display_swap_rejects_same_display_and_invalid_geometry() {
        let valid = [display(0, 0, 0, 1000, 1000, 1.0)];
        assert_eq!(
            plan_display_swap(
                window(0, 0, 0, 500, 500),
                window(0, 100, 100, 500, 500),
                &valid,
            ),
            None
        );

        let invalid = [
            display(0, 0, 0, 1000, 1000, 1.0),
            display(1, 1000, 0, 1000, 1000, 0.0),
        ];
        assert_eq!(
            plan_display_swap(
                window(0, 0, 0, 500, 500),
                window(1, 1000, 0, 500, 500),
                &invalid,
            ),
            None
        );
    }
}

use std::{cell::Cell, rc::Rc, time::Duration};

use anyhow::Result;
use slint::winit_030::{winit, WinitWindowAccessor};
#[cfg(target_os = "macos")]
use slint::TimerMode;
use slint::{ComponentHandle, LogicalPosition, LogicalSize, Timer, Weak};
use tracing::{info, warn};

use crate::window_placement::{
    plan_restore, DisplayRect, PlacementRecord, RestorePlan, SlidePlacementController,
};
use crate::{PresenterWindow, SlideWindow};

const PRESENTER_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(80.0, 80.0);

/// Slint already returns physical client pixels. Do not apply scale a second time.
pub fn slide_surface_width(windows: &AppWindowRefs) -> Option<u32> {
    let slide = windows.slide.upgrade()?;
    let window = slide.window();
    let size = window.size();
    let scale = window.scale_factor();
    (window.is_visible() && size.width > 0 && size.height > 0 && scale.is_finite() && scale > 0.0)
        .then_some(size.width)
}
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
    pub placement: Rc<SlidePlacementController>,
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

#[derive(Default)]
pub struct DisplaySwapController {
    generation: Cell<u64>,
    active: Cell<bool>,
}

impl DisplaySwapController {
    fn begin(&self) -> u64 {
        self.cancel();
        self.active.set(true);
        self.generation.get()
    }

    pub fn cancel(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        self.active.set(false);
    }

    fn is_current(&self, generation: u64) -> bool {
        self.active.get() && self.generation.get() == generation
    }
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
    active: Rc<DisplaySwapController>,
) -> DisplaySwapOutcome {
    if active.active.get() {
        return DisplaySwapOutcome::Busy;
    }

    let plan = match build_native_swap_plan(windows) {
        Ok(plan) => plan,
        Err(outcome) => return outcome,
    };

    capture_slide_placement(windows);
    windows.placement.suspend();
    let generation = active.begin();
    if !target_monitors_are_available(windows, &plan) || !apply_native_swap(windows, &plan) {
        active.cancel();
        windows.placement.cancel();
        return DisplaySwapOutcome::Unavailable;
    }

    restore_window_focus(windows, initiated_by);
    schedule_display_swap_verification(windows.clone(), plan, initiated_by, active, generation, 0);
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
            let (position, position_scale) = match native.outer_position() {
                Ok(position) => (position, native.scale_factor()),
                Err(_) if fullscreen => (monitor.position(), monitor.scale_factor()),
                Err(_) => return None,
            };
            let size = native.outer_size();

            Some(NativeWindowSnapshot {
                monitor,
                outer_position: desktop_point(position, position_scale, cfg!(target_os = "macos")),
                outer_size: desktop_size(size, native.scale_factor(), cfg!(target_os = "macos")),
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
        origin: desktop_point(origin, scale_factor, cfg!(target_os = "macos")),
        size: desktop_size(size, scale_factor, cfg!(target_os = "macos")),
        scale_factor: if cfg!(target_os = "macos") {
            1.0
        } else {
            scale_factor
        },
    })
}

// Winit macOS scales global desktop coordinates with each window/monitor's
// own backing scale. Normalize to Cocoa points before combining rectangles.
fn desktop_point(
    position: winit::dpi::PhysicalPosition<i32>,
    scale: f64,
    logical: bool,
) -> PixelPoint {
    let divisor = if logical { scale } else { 1.0 };
    PixelPoint {
        x: (f64::from(position.x) / divisor).round() as i32,
        y: (f64::from(position.y) / divisor).round() as i32,
    }
}

fn desktop_size(size: winit::dpi::PhysicalSize<u32>, scale: f64, logical: bool) -> PixelSize {
    let divisor = if logical { scale } else { 1.0 };
    PixelSize {
        width: (f64::from(size.width) / divisor).round() as u32,
        height: (f64::from(size.height) / divisor).round() as u32,
    }
}

fn set_desktop_position(window: &winit::window::Window, position: PixelPoint) {
    #[cfg(target_os = "macos")]
    window.set_outer_position(winit::dpi::LogicalPosition::new(position.x, position.y));
    #[cfg(not(target_os = "macos"))]
    window.set_outer_position(winit::dpi::PhysicalPosition::new(position.x, position.y));
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
    let center_x = f64::from(window.outer_position.x) - f64::from(source.origin.x)
        + f64::from(window.outer_size.width) / 2.0;
    let center_y = f64::from(window.outer_position.y) - f64::from(source.origin.y)
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
            set_desktop_position(window, plan.presenter_position);
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
                set_desktop_position(window, plan.slide_position);
            }
        })
        .is_some();

    presenter_applied && slide_applied
}

fn schedule_display_swap_verification(
    windows: AppWindowRefs,
    plan: NativeSwapPlan,
    initiated_by: WindowRole,
    active: Rc<DisplaySwapController>,
    generation: u64,
    attempt: usize,
) {
    let delay = DISPLAY_SWAP_VERIFY_DELAYS[attempt];
    Timer::single_shot(delay, move || {
        // Fullscreen commands supersede an in-flight placement request. Old
        // timers must not retry, recover, steal focus, or finish a newer swap.
        if !active.is_current(generation) {
            return;
        }
        if display_swap_matches(&windows, &plan) {
            finish_placement_swap(&windows, &plan);
            active.cancel();
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
            schedule_display_swap_verification(
                windows,
                plan,
                initiated_by,
                active,
                generation,
                next_attempt,
            );
            return;
        }

        warn!("display swap did not settle before the verification deadline");
        recover_windows_to_available_displays(&windows, plan.slide_fullscreen);
        sync_slide_chrome(&windows);
        restore_window_focus(&windows, initiated_by);
        if let Some(presenter) = windows.presenter.upgrade() {
            presenter.set_status_text(DISPLAY_SWAP_FAILURE_MESSAGE.into());
        }
        active.cancel();
        windows.placement.cancel();
        schedule_placement_capture(windows);
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
        let origin = desktop_point(
            fallback.position(),
            fallback.scale_factor(),
            cfg!(target_os = "macos"),
        );
        let _ = presenter.window().with_winit_window(|window| {
            set_desktop_position(window, origin);
        });
    }

    let Some(slide) = windows.slide.upgrade() else {
        return;
    };
    if !window_has_available_monitor(slide.window()) {
        let origin = desktop_point(
            fallback.position(),
            fallback.scale_factor(),
            cfg!(target_os = "macos"),
        );
        let _ = slide.window().with_winit_window(|window| {
            if slide_fullscreen {
                window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
                    fallback.clone(),
                ))));
            } else {
                set_desktop_position(window, origin);
            }
        });
    }
}

fn window_has_available_monitor(window: &slint::Window) -> bool {
    window
        .with_winit_window(|native| {
            let Ok(position) = native.outer_position() else {
                return false;
            };
            let displays = native
                .available_monitors()
                .enumerate()
                .filter_map(|(id, monitor)| display_geometry(id, &monitor))
                .collect::<Vec<_>>();
            window_controls_are_reachable(
                desktop_point(position, native.scale_factor(), cfg!(target_os = "macos")),
                desktop_size(
                    native.outer_size(),
                    native.scale_factor(),
                    cfg!(target_os = "macos"),
                ),
                &displays,
            )
        })
        .unwrap_or(false)
}

fn window_controls_are_reachable(
    position: PixelPoint,
    size: PixelSize,
    displays: &[DisplayGeometry],
) -> bool {
    // A nearest-monitor handle (notably on Windows) says nothing about whether
    // the window is onscreen. Require a usable strip at its top instead.
    let required_width = i64::from(size.width.min(100));
    let required_height = i64::from(size.height.min(28));
    if required_width == 0 || required_height == 0 {
        return false;
    }
    displays.iter().any(|display| {
        let left = i64::from(position.x).max(i64::from(display.origin.x));
        let right = (i64::from(position.x) + i64::from(size.width))
            .min(i64::from(display.origin.x) + i64::from(display.size.width));
        let top = i64::from(position.y).max(i64::from(display.origin.y));
        let bottom = (i64::from(position.y) + required_height)
            .min(i64::from(display.origin.y) + i64::from(display.size.height));
        right - left >= required_width && bottom - top >= required_height
    })
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
            placement: Rc::new(SlidePlacementController::default()),
        })
    }

    pub fn refs(&self) -> AppWindowRefs {
        AppWindowRefs {
            presenter: self.presenter.as_weak(),
            slide: self.slide.as_weak(),
            placement: self.placement.clone(),
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
    pub placement: Rc<SlidePlacementController>,
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
    capture_slide_placement(windows);
    windows.placement.cancel();
    if let Some(slide) = windows.slide.upgrade() {
        slide.window().set_fullscreen(fullscreen);
        slide.set_titlebar_compensation_height(slide_titlebar_compensation_height(fullscreen));
    }

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_slide_fullscreen(fullscreen);
    }
}

pub fn show_presenter_window(windows: &AppWindowRefs) {
    if let Some(presenter) = windows.presenter.upgrade() {
        if let Err(err) = presenter.show() {
            warn!(error = ?err, "failed to show presenter window");
            return;
        }
        #[cfg(target_os = "macos")]
        crate::macos_window::show_window(presenter.window(), PRESENTER_WINDOW_TITLE);
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
    if let Some(slide) = windows.slide.upgrade() {
        // Restore Slint's rendering lifecycle before raising the native window.
        // A close request hides through Slint, so native ordering alone leaves
        // the adapter hidden and exposes the previous backing image.
        if let Err(err) = slide.show() {
            warn!(error = ?err, "failed to show slide window");
            return;
        }
        windows.placement.visible.set(true);
        schedule_show_recovery(windows.clone());
        sync_slide_chrome(windows);
        #[cfg(target_os = "macos")]
        crate::macos_window::show_window(slide.window(), SLIDE_WINDOW_TITLE);
        slide.window().request_redraw();
    }
}

pub fn hide_presenter_window(windows: &AppWindowRefs) {
    if let Some(presenter) = windows.presenter.upgrade() {
        if let Err(err) = presenter.hide() {
            warn!(error = ?err, "failed to hide presenter window");
        }
    }
}

pub fn hide_slide_window(windows: &AppWindowRefs) {
    capture_slide_placement(windows);
    windows.placement.cancel();
    if let Some(slide) = windows.slide.upgrade() {
        if let Err(err) = slide.hide() {
            warn!(error = ?err, "failed to hide slide window");
        } else {
            windows.placement.visible.set(false);
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

    #[test]
    fn fullscreen_command_invalidates_all_old_verification_ticks() {
        let controller = DisplaySwapController::default();
        let old = controller.begin();
        assert!(controller.is_current(old));
        controller.cancel();
        assert!(!controller.is_current(old));
        // Even after another X request, an old tick cannot reapply fullscreen,
        // recover placement, steal focus, or complete the new operation.
        let new = controller.begin();
        assert!(!controller.is_current(old));
        assert!(controller.is_current(new));
        controller.cancel();
        assert!(!controller.is_current(new));
    }

    #[test]
    fn macos_mixed_dpi_rectangles_share_logical_desktop_coordinates() {
        use winit::dpi::{PhysicalPosition, PhysicalSize};
        // A Retina screen left of the primary uses its own scale for its
        // origin; window coordinates instead use the window's backing scale.
        let left = DisplayGeometry {
            id: 0,
            origin: desktop_point(PhysicalPosition::new(-2000, 400), 2.0, true),
            size: desktop_size(PhysicalSize::new(2000, 1600), 2.0, true),
            scale_factor: 1.0,
        };
        let right = display(1, 0, 200, 1000, 800, 1.0);
        let presenter = WindowGeometry {
            display_id: 0,
            outer_position: desktop_point(PhysicalPosition::new(-1500, 800), 2.0, true),
            outer_size: desktop_size(PhysicalSize::new(1000, 800), 2.0, true),
        };
        let slide = window(1, 250, 400, 500, 400);
        let plan = plan_display_swap(presenter, slide, &[left, right]).unwrap();
        assert_eq!(plan.presenter.outer_position, PixelPoint { x: 250, y: 400 });
        assert_eq!(plan.slide.outer_position, PixelPoint { x: -750, y: 400 });
        let back = plan_display_swap(
            window(1, 250, 400, 500, 400),
            window(0, -750, 400, 500, 400),
            &[left, right],
        )
        .unwrap();
        assert_eq!(back.presenter.outer_position, presenter.outer_position);
        assert_eq!(back.slide.outer_position, slide.outer_position);
        // Physical-coordinate platforms must not divide global coordinates.
        assert_eq!(
            desktop_point(PhysicalPosition::new(-1500, 800), 2.0, false),
            PixelPoint { x: -1500, y: 800 }
        );
    }

    #[test]
    fn recovery_uses_visible_controls_not_nearest_monitor_identity() {
        let displays = [
            display(0, -1000, 0, 1000, 800, 1.0),
            display(1, 0, 0, 1000, 800, 1.0),
        ];
        let reachable = |x, y| {
            window_controls_are_reachable(
                PixelPoint { x, y },
                PixelSize {
                    width: 500,
                    height: 400,
                },
                &displays,
            )
        };
        assert!(reachable(-750, 200));
        assert!(reachable(900, 200));
        assert!(!reachable(901, 200));
        assert!(!reachable(2000, 200));
        assert!(!reachable(-2000, 200));
        assert!(!reachable(0, -50)); // Body visible, controls inaccessible.
        assert!(!reachable(0, 790));
        assert!(!window_controls_are_reachable(
            PixelPoint { x: 0, y: 0 },
            PixelSize {
                width: 0,
                height: 400
            },
            &displays
        ));
        assert!(!window_controls_are_reachable(
            PixelPoint {
                x: i32::MAX,
                y: i32::MAX
            },
            PixelSize {
                width: u32::MAX,
                height: u32::MAX
            },
            &displays
        ));
    }

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

fn placement_display(monitor: &winit::monitor::MonitorHandle) -> Option<DisplayRect> {
    let geometry = display_geometry(0, monitor)?;
    Some(DisplayRect {
        origin: [geometry.origin.x, geometry.origin.y],
        extent: [geometry.size.width, geometry.size.height],
        scale: monitor.scale_factor(),
    })
}

pub fn set_slide_logical_size(windows: &AppWindowRefs, size: [f64; 2]) {
    if let Some(slide) = windows.slide.upgrade() {
        slide.set_slide_window_width(size[0] as f32);
        slide.set_slide_window_height(size[1] as f32);
        slide
            .window()
            .set_size(LogicalSize::new(size[0] as f32, size[1] as f32));
    }
}

pub fn capture_slide_placement(windows: &AppWindowRefs) {
    let controller = &windows.placement;
    if !controller.enabled.get() || controller.busy.get() || !controller.visible.get() {
        return;
    }
    let Some(slide) = windows.slide.upgrade() else {
        return;
    };
    if !window_has_available_monitor(slide.window()) {
        return;
    }
    let record = slide
        .window()
        .with_winit_window(|native| {
            if native_window_placement_is_unsupported(native)
                || native.is_visible() == Some(false)
                || !controller.can_capture(
                    native.fullscreen().is_some(),
                    native.is_minimized() == Some(true),
                )
            {
                return None;
            }
            let monitor = native.current_monitor()?;
            let display = placement_display(&monitor)?;
            let position = desktop_point(
                native.outer_position().ok()?,
                native.scale_factor(),
                cfg!(target_os = "macos"),
            );
            let outer = desktop_size(
                native.outer_size(),
                native.scale_factor(),
                cfg!(target_os = "macos"),
            );
            let inner = native.inner_size().to_logical::<f64>(native.scale_factor());
            let center = [
                ((f64::from(position.x) - f64::from(display.origin[0])
                    + f64::from(outer.width) / 2.0)
                    / f64::from(display.extent[0]))
                .clamp(0.0, 1.0),
                ((f64::from(position.y) - f64::from(display.origin[1])
                    + f64::from(outer.height) / 2.0)
                    / f64::from(display.extent[1]))
                .clamp(0.0, 1.0),
            ];
            Some(PlacementRecord {
                platform: crate::window_placement::platform().into(),
                source_display: display,
                relative_center: center,
                logical_inner_size: [inner.width, inner.height],
            })
        })
        .flatten();
    if let Some(record) = record {
        controller.capture(record);
    }
}

fn schedule_placement_capture(windows: AppWindowRefs) {
    if !windows.placement.enabled.get() {
        return;
    }
    let generation = windows.placement.capture_generation.get().wrapping_add(1);
    windows.placement.capture_generation.set(generation);
    Timer::single_shot(Duration::from_millis(250), move || {
        if windows.placement.capture_generation.get() == generation {
            capture_slide_placement(&windows);
        }
    });
}

pub fn start_slide_placement(windows: AppWindowRefs) {
    if !windows.placement.enabled.get() {
        return;
    }
    if let Some(slide) = windows.slide.upgrade() {
        let event_windows = windows.clone();
        slide.window().on_winit_window_event(move |_, event| {
            use winit::event::WindowEvent;
            match event {
                WindowEvent::Moved(_)
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. } => {
                    schedule_placement_capture(event_windows.clone());
                }
                WindowEvent::MouseInput {
                    state: winit::event::ElementState::Pressed,
                    ..
                }
                | WindowEvent::KeyboardInput { .. }
                    if event_windows.placement.restoring.get() =>
                {
                    event_windows.placement.cancel();
                    schedule_placement_capture(event_windows.clone());
                }
                WindowEvent::CloseRequested => {
                    capture_slide_placement(&event_windows);
                    event_windows.placement.cancel();
                    event_windows.placement.visible.set(false);
                }
                _ => {}
            }
            slint::winit_030::EventResult::Propagate
        });
    }
    let generation = windows.placement.suspend();
    windows.placement.restoring.set(true);
    Timer::single_shot(Duration::ZERO, move || {
        prepare_startup_placement(windows, generation, 0)
    });
}

#[derive(Clone)]
struct NativeRestorePlan {
    target: winit::monitor::MonitorHandle,
    planned: RestorePlan,
    logical_outer_size: [f64; 2],
}

fn build_restore_plan(
    windows: &AppWindowRefs,
    saved: Option<&PlacementRecord>,
) -> Option<NativeRestorePlan> {
    let slide = windows.slide.upgrade()?;
    let presenter_monitor = windows.presenter.upgrade().and_then(|presenter| {
        presenter
            .window()
            .with_winit_window(|native| native.current_monitor())
            .flatten()
    });
    slide
        .window()
        .with_winit_window(|native| {
            if native_window_placement_is_unsupported(native) {
                return None;
            }
            let monitors: Vec<_> = native
                .available_monitors()
                .filter(|m| placement_display(m).is_some())
                .collect();
            let displays: Vec<_> = monitors.iter().filter_map(placement_display).collect();
            let fallback_monitor = presenter_monitor
                .filter(|m| monitors.contains(m))
                .or_else(|| native.primary_monitor().filter(|m| monitors.contains(m)));
            let fallback = fallback_monitor
                .and_then(|m| monitors.iter().position(|v| *v == m))
                .unwrap_or(0);
            let outer = native.outer_size().to_logical::<f64>(native.scale_factor());
            let inner = native.inner_size().to_logical::<f64>(native.scale_factor());
            let frame = [
                (outer.width - inner.width).max(0.0),
                (outer.height - inner.height).max(0.0),
            ];
            let planned = plan_restore(
                saved,
                &displays,
                fallback,
                frame,
                [inner.width, inner.height],
            )?;
            Some(NativeRestorePlan {
                target: monitors.get(planned.target)?.clone(),
                logical_outer_size: [
                    planned.logical_inner_size[0] + frame[0],
                    planned.logical_inner_size[1] + frame[1],
                ],
                planned,
            })
        })
        .flatten()
}

fn apply_restore_plan(windows: &AppWindowRefs, plan: &NativeRestorePlan) -> bool {
    let Some(slide) = windows.slide.upgrade() else {
        return false;
    };
    let available = slide
        .window()
        .with_winit_window(|native| {
            !native_window_placement_is_unsupported(native)
                && native.available_monitors().any(|m| m == plan.target)
        })
        .unwrap_or(false);
    if !available {
        return false;
    }
    let size_settled = slide
        .window()
        .with_winit_window(|native| {
            let inner = native.inner_size().to_logical::<f64>(native.scale_factor());
            (inner.width - plan.planned.logical_inner_size[0]).abs() < 2.0
                && (inner.height - plan.planned.logical_inner_size[1]).abs() < 2.0
        })
        .unwrap_or(false);
    // A position-only retry must not restart native content/frame resizing.
    if !size_settled {
        set_slide_logical_size(windows, plan.planned.logical_inner_size);
    }
    slide
        .window()
        .with_winit_window(|native| {
            set_desktop_position(
                native,
                PixelPoint {
                    x: plan.planned.position[0],
                    y: plan.planned.position[1],
                },
            );
        })
        .is_some()
}

fn prepare_startup_placement(windows: AppWindowRefs, generation: u64, attempt: usize) {
    if !windows.placement.current(generation) {
        return;
    }
    let plan = build_restore_plan(&windows, windows.placement.remembered.borrow().as_ref());
    let Some(plan) = plan else {
        if attempt < DISPLAY_SWAP_VERIFY_DELAYS.len() {
            Timer::single_shot(DISPLAY_SWAP_VERIFY_DELAYS[attempt], move || {
                prepare_startup_placement(windows, generation, attempt + 1)
            });
        } else {
            // Unsupported backends retain the stored section without using its size.
            windows.placement.preserve_size.set(false);
            windows.placement.cancel();
        }
        return;
    };
    // A first launch keeps the existing position when it is already usable.
    if windows.placement.remembered.borrow().is_none() {
        if let Some(slide) = windows.slide.upgrade() {
            if window_has_available_monitor(slide.window()) {
                windows.placement.cancel();
                capture_slide_placement(&windows);
                return;
            }
        }
    }
    if apply_restore_plan(&windows, &plan) {
        verify_startup_placement(windows, plan, generation, 0);
    } else {
        finish_restore_fallback(windows, generation);
    }
}

fn verify_startup_placement(
    windows: AppWindowRefs,
    plan: NativeRestorePlan,
    generation: u64,
    attempt: usize,
) {
    Timer::single_shot(DISPLAY_SWAP_VERIFY_DELAYS[attempt], move || {
        if !windows.placement.current(generation) {
            return;
        }
        let Some(slide) = windows.slide.upgrade() else {
            windows.placement.cancel();
            return;
        };
        let available = slide
            .window()
            .with_winit_window(|native| native.available_monitors().any(|m| m == plan.target))
            .unwrap_or(false);
        if !available {
            finish_restore_fallback(windows, generation);
            return;
        }
        let settled = slide
            .window()
            .with_winit_window(|native| {
                let position = native.outer_position().ok()?;
                let position =
                    desktop_point(position, native.scale_factor(), cfg!(target_os = "macos"));
                let inner = native.inner_size().to_logical::<f64>(native.scale_factor());
                let outer = native.outer_size().to_logical::<f64>(native.scale_factor());
                Some((
                    position,
                    [inner.width, inner.height],
                    [outer.width, outer.height],
                ))
            })
            .flatten();
        if window_has_available_monitor(slide.window()) {
            if let Some((position, size, outer)) = settled {
                let moved_elsewhere =
                    (i64::from(position.x) - i64::from(plan.planned.position[0])).abs() > 32
                        || (i64::from(position.y) - i64::from(plan.planned.position[1])).abs() > 32;
                let size_settled = (size[0] - plan.planned.logical_inner_size[0]).abs() < 2.0
                    && (size[1] - plan.planned.logical_inner_size[1]).abs() < 2.0;
                let frame_settled = (outer[0] - plan.logical_outer_size[0]).abs() < 2.0
                    && (outer[1] - plan.logical_outer_size[1]).abs() < 2.0;
                if moved_elsewhere || (size_settled && frame_settled) {
                    // Prefer an externally moved usable window over delayed correction.
                    windows.placement.cancel();
                    sync_slide_chrome(&windows);
                    capture_slide_placement(&windows);
                    return;
                }
            }
        }
        if attempt + 1 < DISPLAY_SWAP_VERIFY_DELAYS.len() {
            // Recompute frame compensation from actual native geometry.
            let saved = windows.placement.remembered.borrow().clone();
            if let Some(corrected) = build_restore_plan(&windows, saved.as_ref()) {
                if apply_restore_plan(&windows, &corrected) {
                    verify_startup_placement(windows, corrected, generation, attempt + 1);
                    return;
                }
            }
        }
        finish_restore_fallback(windows, generation);
    });
}

fn finish_restore_fallback(windows: AppWindowRefs, generation: u64) {
    if !windows.placement.current(generation) {
        return;
    }
    if let Some(slide) = windows.slide.upgrade() {
        if !window_has_available_monitor(slide.window()) {
            if let Some(plan) = build_restore_plan(&windows, None) {
                apply_restore_plan(&windows, &plan);
            }
        }
    }
    windows.placement.cancel();
    schedule_placement_capture(windows);
}

fn finish_placement_swap(windows: &AppWindowRefs, plan: &NativeSwapPlan) {
    windows.placement.cancel();
    if !windows.placement.enabled.get() {
        return;
    }
    if plan.slide_fullscreen {
        if let Some(target) = placement_display(&plan.slide_target) {
            let mut record =
                windows
                    .placement
                    .remembered
                    .borrow()
                    .clone()
                    .unwrap_or(PlacementRecord {
                        platform: crate::window_placement::platform().into(),
                        source_display: target.clone(),
                        relative_center: [0.5; 2],
                        logical_inner_size: [1024.0, 576.0],
                    });
            record.retarget(target);
            if record.valid() {
                *windows.placement.remembered.borrow_mut() = Some(record);
            }
        }
    } else {
        capture_slide_placement(windows);
    }
}

fn schedule_show_recovery(windows: AppWindowRefs) {
    if !windows.placement.enabled.get() {
        return;
    }
    Timer::single_shot(Duration::ZERO, move || {
        let Some(slide) = windows.slide.upgrade() else {
            return;
        };
        if !window_has_available_monitor(slide.window()) && !windows.placement.busy.get() {
            if slide.window().is_fullscreen() {
                if let Some(plan) = build_restore_plan(&windows, None) {
                    let _ = slide.window().with_winit_window(|native| {
                        native.set_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
                            plan.target,
                        ))))
                    });
                }
            } else if let Some(plan) = build_restore_plan(&windows, None) {
                apply_restore_plan(&windows, &plan);
            }
        }
        schedule_placement_capture(windows);
    });
}

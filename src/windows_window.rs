use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{winit_030::WinitWindowAccessor, ComponentHandle, Window};
use tracing::{debug, info, warn};
use windows_sys::Win32::{
    Foundation::{COLORREF, HWND},
    Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    },
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
};

const SLIDE_CHROME_BACKGROUND: COLORREF = colorref(0x11, 0x13, 0x15);
const SLIDE_CHROME_TEXT: COLORREF = colorref(0x9a, 0xa0, 0xa6);

#[derive(Debug, PartialEq)]
struct WorkAreaFit {
    position: [i32; 2],
    inner_size: [u32; 2],
}

fn fit_geometry(
    work: [i32; 4],
    position: [i32; 2],
    inner: [u32; 2],
    frame: [u32; 2],
    minimum: [u32; 2],
) -> Option<WorkAreaFit> {
    let mut fit = WorkAreaFit {
        position,
        inner_size: inner,
    };
    for axis in 0..2 {
        let extent = u32::try_from(i64::from(work[axis + 2]) - i64::from(work[axis])).ok()?;
        let available = extent.checked_sub(frame[axis])?;
        // Never make presentation controls smaller than their usable minimum.
        if minimum[axis] == 0 || available < minimum[axis] {
            return None;
        }
        fit.inner_size[axis] = inner[axis].max(minimum[axis]).min(available);
        let maximum =
            i64::from(work[axis + 2]) - i64::from(fit.inner_size[axis]) - i64::from(frame[axis]);
        fit.position[axis] = i64::from(position[axis]).clamp(i64::from(work[axis]), maximum) as i32;
    }
    Some(fit)
}

/// Keep startup geometry inside the current monitor's taskbar-free area.
pub fn fit_presenter_to_work_area(presenter: &crate::PresenterWindow) {
    let window = presenter.window();
    let Some(hwnd) = hwnd_from_slint_window(window) else {
        return;
    };
    // SAFETY: MONITORINFO contains plain C fields; cbSize identifies its layout.
    let mut monitor: MONITORINFO = unsafe { std::mem::zeroed() };
    monitor.cbSize = size_of::<MONITORINFO>() as u32;
    // SAFETY: hwnd is owned by this live Slint window and monitor is writable.
    if unsafe {
        GetMonitorInfoW(
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
    } == 0
    {
        warn!(error = %std::io::Error::last_os_error(), "could not query presenter monitor work area");
        return;
    }
    let Some((position, outer, inner)) = window
        .with_winit_window(|native| {
            native
                .outer_position()
                .ok()
                .map(|position| (position, native.outer_size(), native.inner_size()))
        })
        .flatten()
    else {
        return;
    };
    let scale = window.scale_factor();
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let work = [
        monitor.rcWork.left,
        monitor.rcWork.top,
        monitor.rcWork.right,
        monitor.rcWork.bottom,
    ];
    let frame = [
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    ];
    let minimum = [
        (800.0 * scale).ceil() as u32,
        (presenter.get_presenter_minimum_height() * scale).ceil() as u32,
    ];
    let _ = window.with_winit_window(|native| {
        native.set_min_inner_size(Some(slint::winit_030::winit::dpi::PhysicalSize::new(
            minimum[0], minimum[1],
        )));
    });
    if window.is_maximized() || window.is_fullscreen() {
        return;
    }
    let Some(fit) = fit_geometry(
        work,
        [position.x, position.y],
        [inner.width, inner.height],
        frame,
        minimum,
    ) else {
        warn!(
            ?work,
            ?minimum,
            "presenter minimum does not fit monitor work area"
        );
        return;
    };
    info!(
        ?work,
        ?frame,
        scale,
        ?position,
        ?inner,
        ?fit,
        "fitted presenter to monitor work area"
    );
    if fit.inner_size != [inner.width, inner.height] {
        window.set_size(slint::PhysicalSize::new(
            fit.inner_size[0],
            fit.inner_size[1],
        ));
    }
    if fit.position != [position.x, position.y] {
        let _ = window.with_winit_window(|native| {
            native.set_outer_position(slint::winit_030::winit::dpi::PhysicalPosition::new(
                fit.position[0],
                fit.position[1],
            ))
        });
    }
}

pub fn apply_slide_chrome(window: &Window) {
    let Some(hwnd) = hwnd_from_slint_window(window) else {
        return;
    };

    set_dwm_color(hwnd, DWMWA_CAPTION_COLOR, SLIDE_CHROME_BACKGROUND);
    set_dwm_color(hwnd, DWMWA_BORDER_COLOR, SLIDE_CHROME_BACKGROUND);
    set_dwm_color(hwnd, DWMWA_TEXT_COLOR, SLIDE_CHROME_TEXT);
}

const fn colorref(red: u8, green: u8, blue: u8) -> COLORREF {
    (red as COLORREF) | ((green as COLORREF) << 8) | ((blue as COLORREF) << 16)
}

fn hwnd_from_slint_window(window: &Window) -> Option<HWND> {
    let slint_window_handle = window.window_handle();
    let window_handle = slint_window_handle.window_handle().ok()?;
    let RawWindowHandle::Win32(win32) = window_handle.as_raw() else {
        return None;
    };

    Some(win32.hwnd.get() as HWND)
}

fn set_dwm_color(hwnd: HWND, attribute: i32, color: COLORREF) {
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute as u32,
            (&color as *const COLORREF).cast(),
            size_of::<COLORREF>() as u32,
        )
    };

    if result < 0 {
        debug!(
            hresult = result,
            attribute, "failed to apply Windows slide window chrome color"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_area_fit_includes_taskbar_frame_and_dpi_scaled_minimum() {
        for (scale, frame) in [(1.0, [16, 56]), (1.25, [18, 72])] {
            let size = [(1024.0 * scale) as u32, (768.0 * scale) as u32];
            let minimum = [(800.0 * scale) as u32, (560.0 * scale) as u32];
            let plan = fit_geometry([0, 0, 1920, 1032], [100, 100], size, frame, minimum).unwrap();
            for axis in 0..2 {
                assert!(plan.inner_size[axis] >= minimum[axis]);
                assert!(plan.position[axis] >= 0);
                assert!(
                    plan.position[axis] as u32 + plan.inner_size[axis] + frame[axis]
                        <= [1920, 1032][axis]
                );
            }
        }
    }

    #[test]
    fn work_area_fit_clamps_oversized_and_offscreen_windows_on_negative_monitor() {
        let plan = fit_geometry(
            [-1920, -100, 0, 932],
            [500, 2000],
            [3000, 2000],
            [18, 72],
            [1000, 760],
        )
        .unwrap();
        assert_eq!(
            plan,
            WorkAreaFit {
                position: [-1920, -100],
                inner_size: [1902, 960]
            }
        );
        assert_eq!(
            fit_geometry([0, 0, 800, 600], [0, 0], [800, 608], [18, 72], [800, 608]),
            None
        );
        assert_eq!(
            fit_geometry([0, 0, 0, 0], [0, 0], [800, 608], [18, 72], [800, 608]),
            None
        );
    }

    #[test]
    fn colorref_uses_windows_bgr_order() {
        assert_eq!(colorref(0x11, 0x13, 0x15), 0x0015_1311);
    }
}

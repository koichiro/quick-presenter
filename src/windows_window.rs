use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::Window;
use tracing::debug;
use windows_sys::Win32::{
    Foundation::{COLORREF, HWND},
    Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    },
};

const SLIDE_CHROME_BACKGROUND: COLORREF = colorref(0x11, 0x13, 0x15);
const SLIDE_CHROME_TEXT: COLORREF = colorref(0x9a, 0xa0, 0xa6);

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
    fn colorref_uses_windows_bgr_order() {
        assert_eq!(colorref(0x11, 0x13, 0x15), 0x0015_1311);
    }
}

use objc2_app_kit::{NSApplication, NSView, NSWindow, NSWindowStyleMask, NSWindowTitleVisibility};
use objc2_foundation::MainThreadMarker;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::Window;

pub fn apply_slide_chrome(window: &Window, fallback_title: &str) {
    with_window(window, fallback_title, |_, window| {
        window.setStyleMask(window.styleMask() | NSWindowStyleMask::FullSizeContentView);
        window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        window.setTitlebarAppearsTransparent(true);
    });
}

pub fn show_window(window: &Window, fallback_title: &str) -> bool {
    with_window(window, fallback_title, |app, window| {
        app.activate();
        window.deminiaturize(None);
        window.makeKeyAndOrderFront(None);
    })
}

pub fn hide_window(window: &Window, fallback_title: &str) -> bool {
    with_window(window, fallback_title, |_, window| {
        window.orderOut(None);
    })
}

fn with_window(
    window: &Window,
    fallback_title: &str,
    action: impl FnOnce(&NSApplication, &NSWindow),
) -> bool {
    let Some(main_thread) = MainThreadMarker::new() else {
        return false;
    };
    let app = NSApplication::sharedApplication(main_thread);

    if let Some(native_window) = ns_window_from_slint_window(window) {
        action(&app, &native_window);
        return true;
    }

    // Fallback only covers the short interval before Slint exposes a native
    // handle. Normal macOS window operations should use the stable window handle.
    with_titled_window(&app, fallback_title, action)
}

fn ns_window_from_slint_window(window: &Window) -> Option<objc2::rc::Retained<NSWindow>> {
    let slint_window_handle = window.window_handle();
    let window_handle = slint_window_handle.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = window_handle.as_raw() else {
        return None;
    };

    let ns_view = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    ns_view.window()
}

fn with_titled_window(
    app: &NSApplication,
    title: &str,
    action: impl FnOnce(&NSApplication, &NSWindow),
) -> bool {
    let windows = app.windows();

    for window in windows.iter() {
        if window.title().to_string() == title {
            action(app, &window);
            return true;
        }
    }

    false
}

//! Observe actual AppKit transition notifications without replacing Winit's delegate.
use std::cell::Cell;

use anyhow::{Context, Result};
use objc2::{define_class, msg_send, rc::Retained, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSView, NSWindowDidEnterFullScreenNotification, NSWindowDidExitFullScreenNotification,
    NSWindowWillEnterFullScreenNotification, NSWindowWillExitFullScreenNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::ComponentHandle;

#[derive(Default)]
struct Counts([Cell<u32>; 4]);

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the observer stays on
    // the UI thread and has no custom Objective-C deallocation implementation.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Counts]
    struct Observer;
    // SAFETY: NSObjectProtocol adds no requirements.
    unsafe impl NSObjectProtocol for Observer {}
    impl Observer {
        // SAFETY: NSNotificationCenter invokes this selector with a notification.
        #[unsafe(method(observed:))]
        fn observed(&self, notification: &NSNotification) {
            let name = notification.name();
            // SAFETY: AppKit's notification names are immutable process globals.
            let names = unsafe { [NSWindowWillEnterFullScreenNotification,
                NSWindowDidEnterFullScreenNotification, NSWindowWillExitFullScreenNotification,
                NSWindowDidExitFullScreenNotification] };
            for (index, expected) in names.iter().enumerate() {
                if &*name == *expected {
                    let count = &self.ivars().0[index];
                    count.set(count.get() + 1);
                }
            }
        }
    }
);

pub(super) struct Transitions {
    center: Retained<NSNotificationCenter>,
    observer: Retained<Observer>,
}

impl Transitions {
    pub(super) fn new(slide: &crate::SlideWindow) -> Result<Self> {
        let handle = slide.window().window_handle();
        let handle = handle.window_handle()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            anyhow::bail!("fullscreen smoke requires an AppKit window");
        };
        // SAFETY: The Slint adapter owns this live view; smoke runs on the UI thread.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let window = view
            .window()
            .context("fullscreen smoke window is missing")?;
        let this = Observer::alloc(view.mtm()).set_ivars(Counts::default());
        // SAFETY: NSObject init initializes the allocated observer.
        let observer: Retained<Observer> = unsafe { msg_send![super(this), init] };
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: The selector signature matches and Drop unregisters before release.
        unsafe {
            center.addObserver_selector_name_object(
                &observer,
                sel!(observed:),
                None,
                Some(&window),
            );
        }
        Ok(Self { center, observer })
    }

    pub(super) fn counts(&self) -> [u32; 4] {
        self.observer.ivars().0.each_ref().map(Cell::get)
    }
}

impl Drop for Transitions {
    fn drop(&mut self) {
        // SAFETY: This unregisters this exact retained observer on the UI thread.
        unsafe {
            self.center.removeObserver(&self.observer);
        }
    }
}

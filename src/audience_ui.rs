//! UI-thread adapter for the optional audience subsystem.
use crate::{
    app_state::AppState,
    audience::{self, LocalAudienceSession, SessionStatus},
    window_controller::{AppWindowRefs, AppWindows},
    PresenterWindow,
};
use slint::{ComponentHandle, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode, Weak};
use std::{cell::RefCell, net::Ipv4Addr, rc::Rc, time::Duration};

pub struct AudienceUi {
    session: RefCell<Option<LocalAudienceSession>>,
    timer: Timer,
    addresses: RefCell<Vec<Ipv4Addr>>,
    stopping: RefCell<bool>,
    address_index: RefCell<usize>,
    last_url: RefCell<String>,
}

impl AudienceUi {
    pub fn install(windows: &AppWindows, state: Rc<RefCell<AppState>>) -> Rc<Self> {
        let presenter = &windows.presenter;
        let ui = Rc::new(Self {
            session: RefCell::new(None),
            timer: Timer::default(),
            addresses: RefCell::new(Vec::new()),
            stopping: RefCell::new(false),
            address_index: RefCell::new(0),
            last_url: RefCell::new(String::new()),
        });
        let refs = windows.refs();
        let guide_state = state.clone();
        presenter.on_audience_toggle_guide(move || {
            let visible = !guide_state.borrow().audience_join_visible;
            set_join_visible(&refs, &guide_state, visible);
            if visible {
                crate::show_slide_window_from_menu(&refs, &guide_state);
            }
        });
        presenter.set_audience_address(ui.address_label().into());
        let weak = Rc::downgrade(&ui);
        let window = presenter.as_weak();
        presenter.on_audience_refresh_addresses(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if ui.session.borrow().is_some() {
                return;
            }
            match audience::local_addresses() {
                Ok(addresses) => {
                    *ui.addresses.borrow_mut() = addresses;
                    *ui.address_index.borrow_mut() = 0;
                    if let Some(window) = window.upgrade() {
                        window.set_audience_address(ui.address_label().into());
                        window.set_audience_can_start(!ui.addresses.borrow().is_empty());
                    }
                }
                Err(error) => {
                    if let Some(window) = window.upgrade() {
                        window.set_audience_status(
                            format!("Cannot discover LAN addresses: {error}").into(),
                        );
                        window.set_audience_can_start(false);
                    }
                }
            }
        });
        let weak = Rc::downgrade(&ui);
        let window = presenter.as_weak();
        presenter.on_audience_next_address(move || {
            if let Some(ui) = weak.upgrade() {
                if ui.session.borrow().is_none() && !ui.addresses.borrow().is_empty() {
                    let mut index = ui.address_index.borrow_mut();
                    *index = (*index + 1) % ui.addresses.borrow().len();
                    drop(index);
                    if let Some(window) = window.upgrade() {
                        window.set_audience_address(ui.address_label().into());
                    }
                }
            }
        });
        let weak = Rc::downgrade(&ui);
        let window = presenter.as_weak();
        let refs = windows.refs();
        presenter.on_audience_toggle_session(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if let Some(session) = ui.session.borrow().as_ref() {
                *ui.stopping.borrow_mut() = true;
                session.request_stop();
                set_join_visible(&refs, &state, false);
                clear_join_metadata(&refs);
                if let Some(window) = window.upgrade() {
                    window.set_audience_status("Stopping…".into());
                    window.set_audience_url("".into());
                    window.set_audience_code("".into());
                    window.set_audience_qr(Default::default());
                }
                return;
            }
            let Some(&address) = ui.addresses.borrow().get(*ui.address_index.borrow()) else {
                return;
            };
            match LocalAudienceSession::start(address) {
                Ok(session) => {
                    *ui.stopping.borrow_mut() = false;
                    *ui.session.borrow_mut() = Some(session);
                    if let Some(window) = window.upgrade() {
                        window.set_audience_active(true);
                        window.set_audience_status("Starting…".into());
                    }
                    let weak = Rc::downgrade(&ui);
                    let window = window.clone();
                    let refs = refs.clone();
                    let state = state.clone();
                    ui.timer
                        .start(TimerMode::Repeated, Duration::from_millis(100), move || {
                            if let Some(ui) = weak.upgrade() {
                                ui.refresh(&window);
                                sync_join_metadata(&refs);
                                if !window.upgrade().is_some_and(|w| w.get_audience_active()) {
                                    set_join_visible(&refs, &state, false);
                                }
                            }
                        });
                }
                Err(error) => {
                    if let Some(window) = window.upgrade() {
                        window.set_audience_status(
                            format!("Cannot start audience server: {error}").into(),
                        );
                    }
                }
            }
        });
        presenter.invoke_audience_refresh_addresses();
        ui
    }

    fn address_label(&self) -> String {
        self.addresses
            .borrow()
            .get(*self.address_index.borrow())
            .map(ToString::to_string)
            .unwrap_or_else(|| "No IPv4 LAN address. Connect to Wi-Fi or Ethernet.".into())
    }

    fn refresh(&self, window: &Weak<PresenterWindow>) {
        let Some(window) = window.upgrade() else {
            return;
        };
        let snapshot = self
            .session
            .borrow()
            .as_ref()
            .map(LocalAudienceSession::snapshot);
        let Some(snapshot) = snapshot else {
            return;
        };
        window.set_audience_count(snapshot.connections.min(i32::MAX as usize) as i32);
        match snapshot.status {
            SessionStatus::Running { url, code } => {
                // A pending stop must not republish an invalid join URL.
                if *self.stopping.borrow() {
                    return;
                }
                window.set_audience_code(code.clone().into());
                window.set_audience_status(
                    format!("Connected: {} · Session {code}", snapshot.connections).into(),
                );
                if *self.last_url.borrow() != url {
                    match qr_image(&url) {
                        Ok(image) => {
                            window.set_audience_qr(image);
                            window.set_audience_url(url.clone().into());
                            *self.last_url.borrow_mut() = url;
                        }
                        Err(error) => {
                            window.set_audience_status(
                                format!("QR generation failed: {error}").into(),
                            );
                            window.set_audience_url(url.into());
                        }
                    }
                }
            }
            SessionStatus::Starting => {}
            SessionStatus::Stopped | SessionStatus::Failed(_) => {
                if !self
                    .session
                    .borrow()
                    .as_ref()
                    .is_some_and(LocalAudienceSession::is_finished)
                {
                    return;
                }
                let message = match snapshot.status {
                    SessionStatus::Failed(error) => format!("Audience unavailable: {error}"),
                    _ => "Stopped".into(),
                };
                self.session.borrow_mut().take();
                self.timer.stop();
                self.last_url.borrow_mut().clear();
                window.set_audience_active(false);
                window.set_audience_status(message.into());
                window.set_audience_url("".into());
                window.set_audience_code("".into());
                window.set_audience_qr(Default::default());
                window.set_audience_count(0);
            }
        }
    }

    pub fn shutdown(&self) {
        self.timer.stop();
        if let Some(mut session) = self.session.borrow_mut().take() {
            session.shutdown();
        }
    }
}

pub fn set_join_visible(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>, visible: bool) {
    state.borrow_mut().audience_join_visible = visible;
    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_audience_guide_visible(visible);
    }
    if let Some(slide) = windows.slide.upgrade() {
        slide.set_audience_guide_visible(visible);
        slide.set_black_screen_active(state.borrow().black_screen.is_active());
    }
    sync_join_metadata(windows);
}

fn sync_join_metadata(windows: &AppWindowRefs) {
    if let (Some(presenter), Some(slide)) = (windows.presenter.upgrade(), windows.slide.upgrade()) {
        slide.set_audience_qr(presenter.get_audience_qr());
        slide.set_audience_url(presenter.get_audience_url());
        slide.set_audience_code(presenter.get_audience_code());
    }
}

fn clear_join_metadata(windows: &AppWindowRefs) {
    if let Some(slide) = windows.slide.upgrade() {
        slide.set_audience_qr(Default::default());
        slide.set_audience_url("".into());
        slide.set_audience_code("".into());
    }
}

pub(crate) fn qr_image(url: &str) -> Result<slint::Image, qrcode::types::QrError> {
    let qr = qrcode::QrCode::new(url)?;
    let scale = 4;
    let side = (qr.width() + 8) * scale;
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(side as u32, side as u32);
    for y in 0..side {
        for x in 0..side {
            let module_x = x / scale;
            let module_y = y / scale;
            let black = module_x >= 4
                && module_y >= 4
                && module_x < qr.width() + 4
                && module_y < qr.width() + 4
                && qr[(module_x - 4, module_y - 4)] == qrcode::Color::Dark;
            let value = if black { 0 } else { 255 };
            buffer.make_mut_slice()[y * side + x] = Rgba8Pixel {
                r: value,
                g: value,
                b: value,
                a: 255,
            };
        }
    }
    Ok(slint::Image::from_rgba8(buffer))
}

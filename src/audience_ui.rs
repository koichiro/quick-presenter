//! UI-thread adapter for the optional audience subsystem.
use crate::{
    app_state::AppState,
    audience::{self, LocalAudienceSession, SessionStatus},
    audience_events::{AudienceEvent, ReactionKind},
    audience_overlay::OverlayEngine,
    window_controller::{AppWindowRefs, AppWindows},
    PresenterWindow, ReactionOverlayItem,
};
use slint::{
    ComponentHandle, Model, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode, VecModel, Weak,
};
use std::{
    cell::RefCell,
    collections::VecDeque,
    net::Ipv4Addr,
    rc::Rc,
    time::{Duration, Instant},
};

pub struct AudienceUi {
    session: RefCell<Option<LocalAudienceSession>>,
    timer: Timer,
    addresses: RefCell<Vec<Ipv4Addr>>,
    stopping: RefCell<bool>,
    address_index: RefCell<usize>,
    last_url: RefCell<String>,
    recent: RefCell<VecDeque<String>>,
    windows: AppWindowRefs,
    overlay: RefCell<OverlayEngine>,
    overlay_timer: Timer,
    overlay_model: Rc<VecModel<ReactionOverlayItem>>,
    weak_self: RefCell<std::rc::Weak<Self>>,
}

impl AudienceUi {
    pub fn install(windows: &AppWindows, state: Rc<RefCell<AppState>>) -> Rc<Self> {
        Self::install_with_address_source(windows, state, audience::local_addresses)
    }

    pub(crate) fn install_for_smoke(
        windows: &AppWindows,
        state: Rc<RefCell<AppState>>,
    ) -> Rc<Self> {
        Self::install_with_address_source(windows, state, || Ok(vec![Ipv4Addr::LOCALHOST]))
    }

    fn install_with_address_source(
        windows: &AppWindows,
        state: Rc<RefCell<AppState>>,
        addresses: fn() -> std::io::Result<Vec<Ipv4Addr>>,
    ) -> Rc<Self> {
        let presenter = &windows.presenter;
        let ui = Rc::new(Self {
            session: RefCell::new(None),
            timer: Timer::default(),
            addresses: RefCell::new(Vec::new()),
            stopping: RefCell::new(false),
            address_index: RefCell::new(0),
            last_url: RefCell::new(String::new()),
            recent: RefCell::new(VecDeque::new()),
            windows: windows.refs(),
            overlay: RefCell::new(OverlayEngine::default()),
            overlay_timer: Timer::default(),
            overlay_model: Rc::new(VecModel::default()),
            weak_self: RefCell::new(std::rc::Weak::new()),
        });
        *ui.weak_self.borrow_mut() = Rc::downgrade(&ui);
        windows
            .slide
            .set_audience_overlay(ui.overlay_model.clone().into());
        let weak = Rc::downgrade(&ui);
        windows.slide.on_audience_clear_overlay(move || {
            if let Some(ui) = weak.upgrade() {
                ui.clear_overlay();
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
            match addresses() {
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
            if *ui.stopping.borrow() {
                return;
            }
            if let Some(session) = ui.session.borrow().as_ref() {
                *ui.stopping.borrow_mut() = true;
                session.request_stop();
                state.borrow_mut().audience_join_available = false;
                set_join_visible(&refs, &state, false);
                clear_join_metadata(&refs);
                if let Some(window) = window.upgrade() {
                    ui.clear_reactions(&window);
                }
                if let Some(window) = window.upgrade() {
                    window.set_audience_busy(true);
                    window.set_audience_status("Stopping…".into());
                    window.set_audience_url("".into());
                    window.set_audience_code("".into());
                    window.set_audience_qr(Default::default());
                }
                return;
            }
            let Some(&address) = ui.addresses.borrow().get(*ui.address_index.borrow()) else {
                if let Some(window) = window.upgrade() {
                    window.set_audience_status(
                        "No LAN address. Connect to Wi-Fi or Ethernet, then refresh addresses."
                            .into(),
                    );
                }
                return;
            };
            match LocalAudienceSession::start(address) {
                Ok(session) => {
                    if let Some(window) = window.upgrade() {
                        ui.clear_reactions(&window);
                        window.set_audience_reaction_count(0);
                    }
                    *ui.stopping.borrow_mut() = false;
                    *ui.session.borrow_mut() = Some(session);
                    state.borrow_mut().audience_join_available = true;
                    if let Some(window) = window.upgrade() {
                        window.set_audience_active(true);
                        window.set_audience_busy(true);
                        window.set_audience_status("Starting…".into());
                    }
                    let weak = Rc::downgrade(&ui);
                    let window = window.clone();
                    let timer_refs = refs.clone();
                    let timer_state = state.clone();
                    ui.timer
                        .start(TimerMode::Repeated, Duration::from_millis(100), move || {
                            if let Some(ui) = weak.upgrade() {
                                ui.refresh(&window);
                                sync_join_metadata(&timer_refs);
                                if !window.upgrade().is_some_and(|w| w.get_audience_active()) {
                                    timer_state.borrow_mut().audience_join_available = false;
                                    set_join_visible(&timer_refs, &timer_state, false);
                                }
                            }
                        });
                    set_join_visible(&refs, &state, true);
                    crate::show_slide_window_from_menu(&refs, &state);
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
        let weak = Rc::downgrade(&ui);
        let window = presenter.as_weak();
        presenter.on_audience_open_url(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if *ui.stopping.borrow() {
                return;
            }
            let Some(SessionStatus::Running { url, .. }) = ui
                .session
                .borrow()
                .as_ref()
                .map(|session| session.snapshot().status)
            else {
                return;
            };
            let window = window.clone();
            // Browser launching may invoke OS IPC; keep it off the UI thread.
            std::thread::spawn(move || {
                if webbrowser::open(&url).is_err() {
                    let _ = window.upgrade_in_event_loop(|window| {
                        window.set_audience_status(
                            "Cannot open your browser. Copy the join URL instead.".into(),
                        );
                    });
                }
            });
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

    fn overlay_visible(&self) -> bool {
        self.windows.slide.upgrade().is_some_and(|slide| {
            slide.window().is_visible()
                && !slide.get_black_screen_active()
                && !slide.get_audience_guide_visible()
        })
    }
    fn clear_overlay(&self) {
        self.overlay.borrow_mut().clear(Instant::now());
        self.overlay_model.set_vec(Vec::new());
        self.overlay_timer.stop();
    }
    fn update_overlay(&self) {
        if !self.overlay_visible() {
            self.clear_overlay();
            return;
        }
        let frames = self.overlay.borrow_mut().frames(Instant::now());
        if frames.is_empty() {
            self.overlay_timer.stop();
        }
        let rows = frames
            .into_iter()
            .map(|f| ReactionOverlayItem {
                kind: match f.kind {
                    ReactionKind::ThumbsUp => 0,
                    ReactionKind::Heart => 1,
                    ReactionKind::Applause => 2,
                    ReactionKind::Laugh => 3,
                    ReactionKind::Question => 4,
                },
                x: f.x,
                y: f.y,
                opacity: f.opacity,
            })
            .collect::<Vec<_>>();
        if rows.len() != self.overlay_model.row_count() {
            self.overlay_model.set_vec(rows);
        } else {
            for (index, row) in rows.into_iter().enumerate() {
                self.overlay_model.set_row_data(index, row);
            }
        }
    }
    fn start_overlay_animation(&self) {
        self.update_overlay();
        if self.overlay_model.row_count() == 0 || self.overlay_timer.running() {
            return;
        }
        let weak = self.weak_self.borrow().clone();
        self.overlay_timer
            .start(TimerMode::Repeated, Duration::from_millis(16), move || {
                if let Some(ui) = weak.upgrade() {
                    ui.update_overlay();
                }
            });
    }
    fn clear_reactions(&self, window: &PresenterWindow) {
        self.clear_overlay();
        self.recent.borrow_mut().clear();
        window.set_audience_recent_reactions("".into());
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
                window.set_audience_busy(false);
                let events = self
                    .session
                    .borrow()
                    .as_ref()
                    .map(LocalAudienceSession::drain_events)
                    .unwrap_or_default();
                if !events.is_empty() {
                    window.set_audience_reaction_count(
                        window
                            .get_audience_reaction_count()
                            .saturating_add(events.len() as i32),
                    );
                    let visible = self.overlay_visible();
                    if !visible {
                        self.clear_overlay();
                    }
                    let mut recent = self.recent.borrow_mut();
                    for accepted in events {
                        if visible {
                            self.overlay.borrow_mut().push(&accepted, Instant::now());
                        }
                        let AudienceEvent::Reaction { kind } = accepted.event;
                        recent.push_back(kind.emoji().to_owned());
                        if recent.len() > 36 {
                            recent.pop_front();
                        }
                    }
                    window.set_audience_recent_reactions(
                        recent.iter().cloned().collect::<Vec<_>>().join(" ").into(),
                    );
                }
                self.start_overlay_animation();
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
                self.clear_reactions(&window);
                self.session.borrow_mut().take();
                *self.stopping.borrow_mut() = false;
                self.timer.stop();
                self.last_url.borrow_mut().clear();
                window.set_audience_active(false);
                window.set_audience_busy(false);
                window.set_audience_status(message.into());
                window.set_audience_url("".into());
                window.set_audience_code("".into());
                window.set_audience_qr(Default::default());
                window.set_audience_count(0);
            }
        }
    }

    pub fn shutdown(&self) {
        self.clear_overlay();
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
        slide.invoke_audience_clear_overlay();
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
        slide.set_audience_status(presenter.get_audience_status());
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

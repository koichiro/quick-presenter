//! UI-thread adapter for the optional audience subsystem.
use crate::{
    app_state::AppState,
    audience::{self, SessionStatus},
    audience_comments::CommentEngine,
    audience_events::{AudienceEvent, ReactionKind},
    audience_overlay::OverlayEngine,
    audience_session::AudienceSession,
    window_controller::{AppWindowRefs, AppWindows},
    CommentOverlayItem, PresenterWindow, ReactionOverlayItem,
};
use slint::{
    ComponentHandle, Model, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode, VecModel, Weak,
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    net::Ipv4Addr,
    rc::Rc,
    time::{Duration, Instant},
};

const RELAY_RECOVERY_GUIDANCE: &str =
    "Close Settings, retry Audience Live on, then stop it before importing or resetting settings.";

fn ensure_relay_replaceable(
    relay: Option<&crate::audience_relay_http::ManagedRelay>,
) -> Result<(), &'static str> {
    if relay.is_some_and(|relay| relay.has_attempt()) {
        Err(RELAY_RECOVERY_GUIDANCE)
    } else {
        Ok(())
    }
}

fn join_url_for_copy(status: Option<SessionStatus>, stopping: bool) -> Option<String> {
    match (stopping, status) {
        (false, Some(SessionStatus::Running { url, .. })) if !url.is_empty() => Some(url),
        _ => None,
    }
}

// Manual choices last until the session changes, not merely until its next poll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AudiencePanelVisibility {
    active: bool,
    expanded: bool,
}

impl AudiencePanelVisibility {
    fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.expanded = active;
        }
    }

    fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }
}

pub struct AudienceUi {
    panel_visibility: Cell<AudiencePanelVisibility>,
    panel_geometry: RefCell<crate::audience_panel::AudiencePanelGeometry>,
    session: RefCell<Option<AudienceSession>>,
    clipboard: RefCell<crate::clipboard::TextClipboard>,
    timer: Timer,
    addresses: RefCell<Vec<Ipv4Addr>>,
    relay_config: RefCell<Option<crate::audience_relay_http::ManagedRelay>>,
    config_timer: Timer,
    config_path: RefCell<Option<std::path::PathBuf>>,
    pending_import: RefCell<Option<crate::app_config::RelayEndpoint>>,
    configuring: std::cell::Cell<bool>,
    config_error: RefCell<Option<&'static str>>,
    stopping: RefCell<bool>,
    address_index: RefCell<usize>,
    last_url: RefCell<String>,
    recent: RefCell<VecDeque<String>>,
    windows: AppWindowRefs,
    overlay: RefCell<OverlayEngine>,
    overlay_timer: Timer,
    overlay_model: Rc<VecModel<ReactionOverlayItem>>,
    comments: RefCell<CommentEngine>,
    comment_model: Rc<VecModel<CommentOverlayItem>>,
    weak_self: RefCell<std::rc::Weak<Self>>,
}

impl AudienceUi {
    pub fn install(
        windows: &AppWindows,
        state: Rc<RefCell<AppState>>,
        config_path: Option<std::path::PathBuf>,
    ) -> Rc<Self> {
        let ui = Self::install_with_address_source(
            windows,
            state,
            audience::local_addresses,
            OverlayEngine::default(),
            None,
        );
        *ui.config_path.borrow_mut() = config_path.clone();
        ui.configuring.set(true);
        windows.presenter.set_audience_can_import(false);
        windows.presenter.set_audience_can_start(false);
        windows
            .presenter
            .set_audience_status("Loading Audience configuration…".into());
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("audience-config".into())
            .spawn(move || {
                let _ = sender.send(crate::app_config::load(config_path));
            });
        if spawned.is_err() {
            ui.configuring.set(false);
            windows.presenter.set_audience_can_import(true);
            *ui.config_error.borrow_mut() = Some("Cannot load Audience configuration.");
            windows
                .presenter
                .set_audience_status("Cannot load Audience configuration.".into());
            return ui;
        }
        let weak = Rc::downgrade(&ui);
        let window = windows.presenter.as_weak();
        ui.config_timer
            .start(TimerMode::Repeated, Duration::from_millis(20), move || {
                let Some(ui) = weak.upgrade() else {
                    return;
                };
                let result = match receiver.try_recv() {
                    Ok(result) => result,
                    Err(std::sync::mpsc::TryRecvError::Empty) => return,
                    Err(_) => Err("Cannot load Audience configuration."),
                };
                ui.config_timer.stop();
                ui.configuring.set(false);
                if let Some(window) = window.upgrade() { window.set_audience_can_import(true); }
                match result {
                    Ok(endpoint) => {
                        *ui.relay_config.borrow_mut() =
                            endpoint.map(crate::audience_relay_http::ManagedRelay::new);
                        if let Some(window) = window.upgrade() {
                            window.set_audience_status("".into());
                            window.set_audience_address(ui.address_label().into());
                            window.set_audience_can_start(ui.can_start());
                        }
                    }
                    Err(error) => {
                        *ui.config_error.borrow_mut() = Some(error);
                        if let Some(window) = window.upgrade() {
                            window.set_audience_can_start(false);
                            window.set_audience_status(error.into());
                            window.set_audience_alert_title("Audience Live configuration needs attention".into());
                            window.set_audience_alert_guidance("Open Settings to import a valid configuration, or check the file and its access permissions. PDF playback is still available.".into());
                            ui.reveal_alert(&window);
                        }
                    }
                }
            });
        ui
    }

    pub(crate) fn install_for_smoke(
        windows: &AppWindows,
        state: Rc<RefCell<AppState>>,
    ) -> Rc<Self> {
        Self::install_with_address_source(
            windows,
            state,
            || Ok(vec![Ipv4Addr::LOCALHOST]),
            OverlayEngine::for_gui_smoke(),
            None,
        )
    }

    fn install_with_address_source(
        windows: &AppWindows,
        state: Rc<RefCell<AppState>>,
        addresses: fn() -> std::io::Result<Vec<Ipv4Addr>>,
        overlay: OverlayEngine,
        relay_config: Option<crate::audience_relay_http::ManagedRelay>,
    ) -> Rc<Self> {
        let presenter = &windows.presenter;
        let ui = Rc::new(Self {
            panel_visibility: Cell::new(AudiencePanelVisibility::default()),
            panel_geometry: RefCell::new(crate::audience_panel::AudiencePanelGeometry::default()),
            session: RefCell::new(None),
            clipboard: RefCell::new(crate::clipboard::TextClipboard::default()),
            timer: Timer::default(),
            addresses: RefCell::new(Vec::new()),
            relay_config: RefCell::new(relay_config),
            config_timer: Timer::default(),
            config_path: RefCell::new(None),
            pending_import: RefCell::new(None),
            configuring: std::cell::Cell::new(false),
            config_error: RefCell::new(None),
            stopping: RefCell::new(false),
            address_index: RefCell::new(0),
            last_url: RefCell::new(String::new()),
            recent: RefCell::new(VecDeque::new()),
            windows: windows.refs(),
            overlay: RefCell::new(overlay),
            overlay_timer: Timer::default(),
            overlay_model: Rc::new(VecModel::default()),
            comments: RefCell::new(CommentEngine::default()),
            comment_model: Rc::new(VecModel::default()),
            weak_self: RefCell::new(std::rc::Weak::new()),
        });
        *ui.weak_self.borrow_mut() = Rc::downgrade(&ui);
        windows
            .slide
            .set_audience_overlay(ui.overlay_model.clone().into());
        windows
            .slide
            .set_audience_comments(ui.comment_model.clone().into());
        let weak = Rc::downgrade(&ui);
        presenter.on_audience_toggle_panel(move || {
            if let Some(ui) = weak.upgrade() {
                let mut visibility = ui.panel_visibility.get();
                visibility.toggle();
                ui.panel_visibility.set(visibility);
                if let Some(window) = ui.windows.presenter.upgrade() {
                    ui.panel_geometry
                        .borrow_mut()
                        .set_expanded(&window, visibility.expanded);
                }
            }
        });
        let weak = Rc::downgrade(&ui);
        presenter.on_audience_toggle_comments(move || {
            if let Some(ui) = weak.upgrade() {
                if let Some(session) = ui.session.borrow().as_ref() {
                    let paused = !ui.comments.borrow().paused();
                    let generation = session.advance_comment_generation(paused);
                    ui.comments.borrow_mut().set_paused(paused, generation);
                    ui.comment_model.set_vec(Vec::new());
                    if let Some(window) = ui.windows.presenter.upgrade() {
                        window.set_audience_comments_paused(paused);
                    }
                }
            }
        });
        let weak = Rc::downgrade(&ui);
        windows.slide.on_audience_clear_overlay(move || {
            if let Some(ui) = weak.upgrade() {
                ui.clear_overlay();
            }
        });
        let weak = Rc::downgrade(&ui);
        presenter.on_audience_import_configuration(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if !ui
                .windows
                .presenter
                .upgrade()
                .is_some_and(|w| w.get_settings_visible())
                || ui.session.borrow().is_some()
                || ui.configuring.get()
            {
                return;
            }
            if let Err(message) = ensure_relay_replaceable(ui.relay_config.borrow().as_ref()) {
                if let Some(window) = ui.windows.presenter.upgrade() {
                    window.set_audience_status(message.into());
                }
                return;
            }
            let Some(path) = rfd::FileDialog::new()
                .add_filter("TOML configuration", &["toml"])
                .pick_file()
            else {
                return;
            };
            if let Some(window) = ui.windows.presenter.upgrade() {
                match crate::app_config::preview_import(&path) {
                    Ok(endpoint) => {
                        window.set_audience_import_preview(endpoint.origin.as_str().into());
                        window.set_audience_can_start(false);
                        *ui.pending_import.borrow_mut() = Some(endpoint);
                    }
                    Err(message) => window.set_audience_status(message.into()),
                }
            }
        });
        let weak = Rc::downgrade(&ui);
        presenter.on_audience_confirm_import(move |apply| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let endpoint = ui.pending_import.borrow_mut().take();
            let Some(window) = ui.windows.presenter.upgrade() else {
                return;
            };
            window.set_audience_import_preview("".into());
            if let Some(endpoint) = endpoint.filter(|_| apply) {
                if let Err(message) = ensure_relay_replaceable(ui.relay_config.borrow().as_ref()) {
                    window.set_audience_status(message.into());
                    window.set_audience_can_start(ui.can_start());
                    return;
                }
                let result = crate::app_config::import_destination(ui.config_path.borrow().clone())
                    .and_then(|destination| {
                        crate::app_config::save_import(&destination, &endpoint)
                    });
                match result {
                    Ok(()) => {
                        *ui.relay_config.borrow_mut() =
                            Some(crate::audience_relay_http::ManagedRelay::new(endpoint));
                        *ui.config_error.borrow_mut() = None;
                        window.set_audience_address(ui.address_label().into());
                        window.set_audience_status(
                            "Configuration imported. Close Settings and turn Audience Live on when ready.".into(),
                        );
                        window.set_audience_alert_title("".into());
                        window.set_audience_alert_guidance("".into());
                    }
                    Err(message) => window.set_audience_status(message.into()),
                }
            }
            window.set_audience_can_start(ui.can_start());
        });
        let weak = Rc::downgrade(&ui);
        presenter.on_audience_reset_configuration(move || {
            let Some(ui) = weak.upgrade() else { return; };
            let Some(window) = ui.windows.presenter.upgrade() else { return; };
            if !window.get_settings_visible()
                || ui.session.borrow().is_some()
                || ui.configuring.get()
                || ui.pending_import.borrow().is_some()
            {
                return;
            }
            if let Err(message) = ensure_relay_replaceable(ui.relay_config.borrow().as_ref()) {
                window.set_audience_status(message.into());
                return;
            }
            let result = crate::app_config::import_destination(ui.config_path.borrow().clone())
                .and_then(|path| crate::app_config::reset_relay(&path));
            match result {
                Ok(()) => {
                    *ui.relay_config.borrow_mut() = None;
                    *ui.config_error.borrow_mut() = None;
                    window.set_audience_alert_title("".into());
                    window.set_audience_alert_guidance("".into());
                    window.invoke_audience_refresh_addresses();
                    window.set_audience_address(ui.address_label().into());
                    window.set_audience_can_start(ui.can_start());
                    window.set_audience_status(
                        "Relay settings reset. Close Settings and turn Audience Live on to use the local server.".into(),
                    );
                }
                Err(message) => window.set_audience_status(message.into()),
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
                        window.set_audience_can_start(ui.can_start());
                    }
                }
                Err(error) => {
                    if let Some(window) = window.upgrade() {
                        window.set_audience_status(
                            format!("Cannot discover LAN addresses: {error}").into(),
                        );
                        window.set_audience_can_start(ui.can_start());
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
            if *ui.stopping.borrow()
                || ui.configuring.get()
                || ui.pending_import.borrow().is_some()
                || ui.config_error.borrow().is_some()
            {
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
            let address = ui
                .addresses
                .borrow()
                .get(*ui.address_index.borrow())
                .copied();
            let relay = ui.relay_config.borrow().clone();
            match AudienceSession::start(address, relay) {
                Ok(session) => {
                    if let Some(window) = window.upgrade() {
                        ui.clear_reactions(&window);
                        window.set_audience_reaction_count(0);
                    }
                    *ui.stopping.borrow_mut() = false;
                    *ui.session.borrow_mut() = Some(session);
                    state.borrow_mut().audience_join_available = true;
                    if let Some(window) = window.upgrade() {
                        ui.set_active(&window, true);
                        window.set_audience_busy(true);
                        window.set_audience_status("Starting…".into());
                        window.set_audience_alert_title("".into());
                        window.set_audience_alert_guidance("".into());
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
                        let alert = crate::audience_alert::for_failure(&error.to_string());
                        window.set_audience_status(alert.title.into());
                        window.set_audience_alert_title(alert.title.into());
                        window.set_audience_alert_guidance(alert.guidance.into());
                        ui.reveal_alert(&window);
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
                if let Err(error) = crate::browser::open_join_url(&url) {
                    tracing::warn!(error = %error, "Join URL browser activation failed");
                    let _ = window.upgrade_in_event_loop(|window| {
                        window.set_status_text(
                            "Cannot open your browser. Copy the join URL instead.".into(),
                        );
                        window.set_audience_status(
                            "Cannot open your browser. Copy the join URL instead.".into(),
                        );
                    });
                }
            });
        });
        let weak = Rc::downgrade(&ui);
        let window = presenter.as_weak();
        presenter.on_audience_copy_url(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let status = ui
                .session
                .borrow()
                .as_ref()
                .map(|session| session.snapshot().status);
            let Some(url) = join_url_for_copy(status, *ui.stopping.borrow()) else {
                return;
            };
            let message = match ui.clipboard.borrow_mut().copy(&url) {
                Ok(()) => "Join URL copied.",
                Err(error) => {
                    tracing::warn!(%error, "Join URL clipboard copy failed");
                    "Cannot copy the join URL. Try again."
                }
            };
            if let Some(window) = window.upgrade() {
                window.set_status_text(message.into());
            }
        });
        presenter.invoke_audience_refresh_addresses();
        ui
    }

    fn set_active(&self, window: &PresenterWindow, active: bool) {
        let mut visibility = self.panel_visibility.get();
        visibility.set_active(active);
        self.panel_visibility.set(visibility);
        window.set_audience_active(active);
        self.panel_geometry
            .borrow_mut()
            .set_expanded(window, visibility.expanded);
    }

    fn reveal_alert(&self, window: &PresenterWindow) {
        let mut visibility = self.panel_visibility.get();
        visibility.expanded = true;
        self.panel_visibility.set(visibility);
        self.panel_geometry.borrow_mut().set_expanded(window, true);
    }

    fn can_start(&self) -> bool {
        !self.configuring.get()
            && self.config_error.borrow().is_none()
            && (self.relay_config.borrow().is_some() || !self.addresses.borrow().is_empty())
    }

    fn address_label(&self) -> String {
        if self.relay_config.borrow().is_some() {
            return "Relay".into();
        }
        self.addresses
            .borrow()
            .get(*self.address_index.borrow())
            .map(ToString::to_string)
            .unwrap_or_else(|| "No IPv4 LAN address. Connect to Wi-Fi or Ethernet.".into())
    }

    fn overlay_visible(&self) -> bool {
        self.windows
            .slide
            .upgrade()
            .is_some_and(|slide| slide.window().is_visible() && !slide.get_black_screen_active())
    }
    fn clear_overlay(&self) {
        self.overlay.borrow_mut().clear(Instant::now());
        self.overlay_model.set_vec(Vec::new());
        if !self.comments.borrow().has_work() {
            self.overlay_timer.stop();
        }
    }
    fn update_overlay(&self) {
        if !self.overlay_visible() {
            self.overlay.borrow_mut().clear(Instant::now());
            self.overlay_model.set_vec(Vec::new());
        }
        let now = Instant::now();
        let comment_frames = self.comments.borrow_mut().frames(now);
        self.comment_model.set_vec(
            comment_frames
                .into_iter()
                .map(|f| CommentOverlayItem {
                    text: f.text.into(),
                    x: f.x,
                    y: f.y,
                    width: f.width,
                })
                .collect::<Vec<_>>(),
        );
        let frames = self.overlay.borrow_mut().frames(Instant::now());
        if frames.is_empty() && !self.comments.borrow().has_work() {
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
        if (self.overlay_model.row_count() == 0 && !self.comments.borrow().has_work())
            || self.overlay_timer.running()
        {
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
        self.comments.borrow_mut().reset();
        self.comment_model.set_vec(Vec::new());
        window.set_audience_comments_paused(false);
        window.set_audience_comment_history("".into());
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
            .map(AudienceSession::snapshot);
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
                    .map(AudienceSession::drain_events)
                    .unwrap_or_default();
                if !events.is_empty() {
                    let visible = self.overlay_visible();
                    if !visible {
                        self.clear_overlay();
                    }
                    let mut recent = self.recent.borrow_mut();
                    for accepted in events {
                        match &accepted.event {
                            AudienceEvent::Reaction { kind } => {
                                window.set_audience_reaction_count(
                                    window.get_audience_reaction_count().saturating_add(1),
                                );
                                if visible {
                                    self.overlay.borrow_mut().push(&accepted, Instant::now());
                                }
                                recent.push_back(kind.emoji().to_owned());
                                if recent.len() > 36 {
                                    recent.pop_front();
                                }
                            }
                            AudienceEvent::Comment { body, handle } => {
                                let text = crate::audience_events::comment_display(body, handle);
                                let width = if let Some(slide) = self.windows.slide.upgrade() {
                                    slide.set_comment_measure_text(text.into());
                                    slide.get_comment_measure_width()
                                } else {
                                    f32::NAN
                                };
                                self.comments.borrow_mut().receive(
                                    &accepted,
                                    width,
                                    Instant::now(),
                                );
                            }
                        }
                    }
                    window.set_audience_comment_history(
                        self.comments
                            .borrow()
                            .history()
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("\n")
                            .into(),
                    );
                    window.set_audience_recent_reactions(
                        recent.iter().cloned().collect::<Vec<_>>().join(" ").into(),
                    );
                }
                self.start_overlay_animation();
                window.set_audience_code(code.clone().into());
                window.set_audience_status(
                    format!(
                        "Connected: {} · Session {code}{}",
                        snapshot.connections,
                        if self
                            .session
                            .borrow()
                            .as_ref()
                            .is_some_and(AudienceSession::comment_log_failed)
                        {
                            " · Comment log unavailable"
                        } else {
                            ""
                        }
                    )
                    .into(),
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
                    .is_some_and(AudienceSession::is_finished)
                {
                    return;
                }
                let failed = matches!(&snapshot.status, SessionStatus::Failed(_));
                let message: String = match snapshot.status {
                    SessionStatus::Failed(error) => {
                        let alert = crate::audience_alert::for_failure(&error);
                        window.set_audience_alert_title(alert.title.into());
                        window.set_audience_alert_guidance(alert.guidance.into());
                        alert.title.into()
                    }
                    _ => "Stopped".into(),
                };
                self.clear_reactions(&window);
                self.session.borrow_mut().take();
                *self.stopping.borrow_mut() = false;
                self.timer.stop();
                self.last_url.borrow_mut().clear();
                self.set_active(&window, false);
                if failed {
                    self.reveal_alert(&window);
                }
                window.set_audience_busy(false);
                window.set_audience_recovery_pending(
                    ensure_relay_replaceable(self.relay_config.borrow().as_ref()).is_err(),
                );
                window.set_audience_status(message.into());
                window.set_audience_url("".into());
                window.set_audience_code("".into());
                window.set_audience_qr(Default::default());
                window.set_audience_count(0);
            }
        }
    }

    pub fn shutdown(&self) {
        self.clipboard.borrow_mut().release();
        self.clear_overlay();
        self.comments.borrow_mut().reset();
        self.comment_model.set_vec(Vec::new());
        self.overlay_timer.stop();
        self.timer.stop();
        self.config_timer.stop();
        if let Some(mut session) = self.session.borrow_mut().take() {
            session.shutdown();
        }
        // A failed start may retain an ambiguous creation identity for manual retry.
        // Application exit also attempts bounded cleanup of that identity.
        if let Some(settings) = self.relay_config.borrow().clone() {
            if settings.has_attempt() {
                if let Ok(mut session) =
                    crate::audience_relay::RemoteAudienceSession::start_managed(settings)
                {
                    session.shutdown();
                }
            }
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

#[cfg(test)]
mod configuration_change_tests {
    use super::*;
    use crate::{app_config::RelayEndpoint, audience_relay_http::ManagedRelay};

    #[test]
    fn pending_creation_blocks_configuration_changes_without_losing_retry_identity() {
        let relay = ManagedRelay::new(RelayEndpoint {
            origin: url::Url::parse("https://relay.example/").unwrap(),
            create_token: "A".repeat(43),
        });
        assert!(ensure_relay_replaceable(None).is_ok());
        assert!(ensure_relay_replaceable(Some(&relay)).is_ok());
        let attempt = relay.attempt().unwrap();
        // A failed creation can retain only its request identity, without room metadata.
        assert!(attempt.room.is_none());
        let shared = relay.clone();
        for _ in 0..3 {
            assert_eq!(
                ensure_relay_replaceable(Some(&relay)),
                Err(RELAY_RECOVERY_GUIDANCE)
            );
            assert!(shared.has_attempt());
            assert_eq!(shared.attempt().unwrap().key, attempt.key);
        }
        // The session lifecycle, not a settings action, releases the retained identity.
        shared.clear();
        assert!(ensure_relay_replaceable(Some(&relay)).is_ok());
    }
}

#[cfg(test)]
mod panel_visibility_tests {
    use super::AudiencePanelVisibility;

    #[test]
    fn inactive_starts_collapsed_and_supports_manual_open_and_close() {
        let mut panel = AudiencePanelVisibility::default();
        assert!(!panel.expanded);
        panel.toggle();
        assert!(panel.expanded);
        panel.set_active(false);
        assert!(panel.expanded);
        panel.toggle();
        assert!(!panel.expanded);
        assert!(!panel.active);
    }

    #[test]
    fn session_transitions_override_manual_choice_but_polls_do_not() {
        let mut panel = AudiencePanelVisibility::default();
        panel.set_active(true);
        assert!(panel.expanded);
        panel.toggle();
        assert!(!panel.expanded);
        panel.set_active(true);
        assert!(!panel.expanded);
        assert!(panel.active);
        panel.toggle();
        panel.set_active(false);
        assert!(!panel.expanded);
        panel.toggle();
        assert!(panel.expanded);
        panel.set_active(true);
        panel.set_active(false);
        assert!(!panel.expanded);
    }
}

#[cfg(test)]
mod join_url_copy_tests {
    use super::{join_url_for_copy, SessionStatus};

    #[test]
    fn copy_preserves_the_full_join_url_including_its_fragment() {
        let url = "https://relay.example/join/session?lang=en#k=secret-test-token";
        let running = SessionStatus::Running {
            url: url.into(),
            code: "session".into(),
        };
        assert_eq!(
            join_url_for_copy(Some(running.clone()), false).as_deref(),
            Some(url)
        );
        assert_eq!(join_url_for_copy(Some(running), true), None);
    }

    #[test]
    fn copy_is_unavailable_without_a_current_running_url() {
        for status in [
            None,
            Some(SessionStatus::Starting),
            Some(SessionStatus::Stopped),
            Some(SessionStatus::Failed("offline".into())),
            Some(SessionStatus::Running {
                url: String::new(),
                code: "session".into(),
            }),
        ] {
            assert_eq!(join_url_for_copy(status, false), None);
        }
    }
}

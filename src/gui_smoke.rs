use std::{
    cell::{Cell, RefCell},
    fs,
    path::Path,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use slint::{ComponentHandle, Model};

use crate::{
    app_state::AppState,
    cli::GuiSmokeOptions,
    input::PresentationCommand,
    render_controller::CURRENT_RENDER_WIDTH,
    rendering::{RenderPurpose, RenderRequest},
    window_controller::{set_slide_fullscreen, AppWindows},
};

const ASYNC_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const ASYNC_OPEN_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub fn run(options: GuiSmokeOptions) -> Result<()> {
    let mut report = GuiSmokeReport::new(options.pdf_path.display().to_string());
    let result = run_checks(&options, &mut report);

    if let Err(err) = result {
        report.fail("run completed", err.to_string());
    }

    let output = report.render();
    if let Some(path) = options.report_path.as_ref() {
        write_report(path, &output)?;
    } else {
        print!("{output}");
    }

    if report.failed_count() > 0 {
        bail!(
            "GUI smoke failed: {} passed, {} failed",
            report.passed_count(),
            report.failed_count()
        );
    }

    println!(
        "GUI smoke succeeded: {} passed, {} failed",
        report.passed_count(),
        report.failed_count()
    );
    Ok(())
}

fn run_checks(options: &GuiSmokeOptions, report: &mut GuiSmokeReport) -> Result<()> {
    let windows = AppWindows::new().context("failed to create Slint windows")?;
    crate::configure_linux_desktop_identity()?;
    crate::configure_shortcut_modifiers(&windows);
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState::default()));

    crate::wire_callbacks(&windows, windows.refs(), state.clone());
    crate::apply_app_metadata(&windows.presenter);
    let _audience_ui = crate::audience_ui::AudienceUi::install_for_smoke(&windows, state.clone());

    windows.apply_initial_positions();
    windows
        .slide
        .show()
        .context("failed to show slide window")?;
    let window_refs = windows.refs();
    crate::apply_macos_slide_window_chrome(&window_refs);
    crate::sync_slide_chrome(&window_refs);
    windows
        .presenter
        .show()
        .context("failed to show presenter window")?;
    crate::set_application_icon();

    report.pass("created presenter and slide windows");
    report.check(
        "presenter window handle is alive",
        window_refs.presenter.upgrade().is_some(),
        "presenter window weak handle upgraded",
        "presenter window weak handle could not be upgraded",
    );
    report.check(
        "slide window handle is alive",
        window_refs.slide.upgrade().is_some(),
        "slide window weak handle upgraded",
        "slide window weak handle could not be upgraded",
    );

    check_notes_font_size(&windows, &state, report, "before opening a PDF")?;
    check_audience_join_screen(&windows, &state, report)?;

    crate::begin_open_pdf(&window_refs, &state, options.pdf_path.clone());
    report.check(
        "slide progress indicator is hidden while opening without a deck",
        !windows.presenter.get_has_slide_progress()
            && windows.presenter.get_slide_progress_value() == 0.0,
        "presenter progress indicator is empty",
        "presenter progress indicator was unexpectedly visible",
    );
    wait_for_async_open(&window_refs, &state)
        .context("failed to open and render PDF through async render scheduler")?;

    report.pass("opened and rendered PDF through async render scheduler");
    report_state(
        report,
        &state,
        "opened document has presentation state",
        |state| state.presentation.snapshot().is_some(),
    );
    report_state(
        report,
        &state,
        "opened document has at least two pages",
        |state| {
            state
                .presentation
                .snapshot()
                .is_some_and(|snapshot| snapshot.total_pages >= 2)
        },
    );
    report_state(report, &state, "current slide render is cached", |state| {
        state.presentation.snapshot().is_some_and(|snapshot| {
            state
                .render_cache
                .peek(RenderRequest {
                    page_index: snapshot.current_index,
                    width: CURRENT_RENDER_WIDTH,
                    purpose: RenderPurpose::CurrentSlide,
                })
                .is_some()
        })
    });
    report_state(
        report,
        &state,
        "next preview availability is known",
        |state| {
            state
                .presentation
                .snapshot()
                .is_some_and(|snapshot| snapshot.total_pages == 1 || snapshot.next_index.is_some())
        },
    );
    report_state(report, &state, "speaker notes were checked", |state| {
        state.status_text == "Ready" || state.status_text.contains("notes")
    });
    check_notes_font_size(&windows, &state, report, "with a PDF open")?;
    check_audience_join_screen(&windows, &state, report)?;
    report_presenter_progress(
        report,
        &windows,
        &state,
        "slide progress indicator matches the first page",
    );

    run_command(
        &window_refs,
        &state,
        report,
        "next page command advances",
        PresentationCommand::NextPage,
        |state| current_page_index(state) == Some(1),
    );
    report_presenter_progress(
        report,
        &windows,
        &state,
        "slide progress indicator updates after next page",
    );
    run_command(
        &window_refs,
        &state,
        report,
        "previous page command returns to first page",
        PresentationCommand::PreviousPage,
        |state| current_page_index(state) == Some(0),
    );
    run_command(
        &window_refs,
        &state,
        report,
        "last page command jumps to final page",
        PresentationCommand::LastPage,
        |state| {
            state
                .presentation
                .snapshot()
                .is_some_and(|snapshot| snapshot.current_index + 1 == snapshot.total_pages)
        },
    );
    report_presenter_progress(
        report,
        &windows,
        &state,
        "slide progress indicator reaches the final page",
    );
    run_command(
        &window_refs,
        &state,
        report,
        "first page command returns to first page",
        PresentationCommand::FirstPage,
        |state| current_page_index(state) == Some(0),
    );

    crate::handle_presentation_command(
        &window_refs,
        &state,
        PresentationCommand::ToggleBlackScreen,
    );
    report_state(
        report,
        &state,
        "black screen command enables blanking",
        |state| state.black_screen.is_active(),
    );
    crate::handle_presentation_command(&window_refs, &state, PresentationCommand::NextPage);
    report_state(
        report,
        &state,
        "navigation continues while black screen is active",
        |state| state.black_screen.is_active() && current_page_index(state) == Some(1),
    );
    crate::handle_presentation_command(
        &window_refs,
        &state,
        PresentationCommand::ToggleBlackScreen,
    );
    report_state(
        report,
        &state,
        "black screen command restores slide output",
        |state| !state.black_screen.is_active() && current_page_index(state) == Some(1),
    );

    {
        let fullscreen = state.borrow_mut().fullscreen.toggle_slide_fullscreen();
        set_slide_fullscreen(&window_refs, fullscreen);
    }
    report_state(report, &state, "fullscreen state toggles on", |state| {
        state.fullscreen.is_slide_fullscreen()
    });
    crate::handle_presentation_command(
        &window_refs,
        &state,
        PresentationCommand::ExitSlideFullscreen,
    );
    report_state(
        report,
        &state,
        "fullscreen exit command toggles off",
        |state| !state.fullscreen.is_slide_fullscreen(),
    );

    report_state(
        report,
        &state,
        "presenter and slide state remain synchronized",
        |state| {
            state.presentation.snapshot().is_some()
                && !state.black_screen.is_active()
                && !state.fullscreen.is_slide_fullscreen()
        },
    );

    #[cfg(unix)]
    {
        crate::control_smoke::run(&windows, state.clone(), options.pdf_path.clone())?;
        report.pass("local control operates through the GUI and isolated renderer");
    }
    check_hidden_slide_recovery(&windows, &state, report)?;

    Ok(())
}

fn check_hidden_slide_recovery(
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    report: &mut GuiSmokeReport,
) -> Result<()> {
    let refs = windows.refs();
    let frames = Rc::new(Cell::new(0_u32));
    let rendered_frames = frames.clone();
    windows
        .slide
        .window()
        .set_rendering_notifier(move |phase, _| {
            if matches!(phase, slint::RenderingState::AfterRendering) {
                rendered_frames.set(rendered_frames.get() + 1);
            }
        })
        .context("failed to monitor slide rendering for hidden-window recovery")?;
    settle_notes_layout()?;

    for fullscreen in [false, true] {
        set_slide_fullscreen(&refs, fullscreen);
        settle_notes_layout()?;
        for (close_request, raise, color) in [
            (true, false, [210, 30, 90, 255]),
            (true, true, [20, 160, 200, 255]),
            (false, false, [90, 180, 30, 255]),
            (false, true, [160, 40, 200, 255]),
        ] {
            if close_request {
                windows
                    .slide
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::CloseRequested);
            } else {
                crate::hide_slide_window_from_menu(&refs, state);
            }
            report.check(
                format!(
                    "slide is hidden through {}",
                    if close_request {
                        "close request"
                    } else {
                        "Hide Slide"
                    }
                ),
                !windows.slide.window().is_visible() && !state.borrow().window_menu.slide_visible(),
                "Slint lifecycle is hidden",
                "native hide left the Slint lifecycle visible",
            );
            // Different solid images make stale backing content unambiguous and
            // avoid relying on the appearance of the caller's PDF fixture.
            let buffer =
                slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&color, 1, 1);
            windows
                .slide
                .set_page_image(slint::Image::from_rgba8(buffer));
            settle_notes_layout()?;
            let previous_frames = frames.get();
            if raise {
                crate::bring_slide_window_to_front(&refs, state);
            } else {
                crate::show_slide_window_from_menu(&refs, state);
            }
            settle_notes_layout()?;
            let action = format!(
                "{} ({})",
                if raise {
                    "Bring Slide to Front"
                } else {
                    "Show Slide"
                },
                if fullscreen { "fullscreen" } else { "windowed" }
            );
            report.check(
                format!("{action} restores the slide rendering lifecycle"),
                windows.slide.window().is_visible() && frames.get() > previous_frames,
                "Slint is visible and rendered a new frame",
                "Slint is hidden or no new frame was rendered",
            );
            let snapshot = windows
                .slide
                .window()
                .take_snapshot()
                .context("failed to capture the restored slide")?;
            let center = &snapshot.as_bytes()[((snapshot.height() / 2 * snapshot.width()
                + snapshot.width() / 2)
                * 4) as usize..];
            report.check(
                format!("{action} renders the image changed while hidden"),
                center[..3]
                    .iter()
                    .zip(color[..3].iter())
                    .all(|(actual, expected)| actual.abs_diff(*expected) <= 2),
                "restored slide has the replacement image pixels",
                format!(
                    "unexpected center pixel: {:?}, expected {:?}",
                    &center[..4],
                    color
                ),
            );
        }
    }
    set_slide_fullscreen(&refs, false);
    if let Some(snapshot) = state.borrow().presentation.snapshot() {
        crate::apply_snapshot_to_windows(&refs, &state.borrow(), &snapshot);
    }
    settle_notes_layout()?;
    Ok(())
}

fn run_command(
    windows: &crate::window_controller::AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    report: &mut GuiSmokeReport,
    name: &'static str,
    command: PresentationCommand,
    verify: impl FnOnce(&AppState) -> bool,
) {
    crate::handle_presentation_command(windows, state, command);
    report_state(report, state, name, verify);
}

fn report_state(
    report: &mut GuiSmokeReport,
    state: &Rc<RefCell<AppState>>,
    name: &'static str,
    verify: impl FnOnce(&AppState) -> bool,
) {
    let state = state.borrow();
    report.check(name, verify(&state), "state matched", "state did not match");
}

fn check_audience_join_screen(
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    report: &mut GuiSmokeReport,
) -> Result<()> {
    let before = state.borrow().presentation.snapshot();
    let generation = state.borrow().render_generation;
    let now = Instant::now();
    let elapsed = state.borrow().timer.elapsed_at(now);
    let presenter = &windows.presenter;
    report.check(
        "Inactive Audience Live offers the ON action",
        presenter.get_audience_toggle_label() == "Audience Live ON 👍️",
        "start action displayed",
        "incorrect start label",
    );
    presenter.invoke_audience_toggle_session();
    let deadline = Instant::now() + Duration::from_secs(5);
    while presenter.get_audience_url().is_empty() && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter)?;
    }
    let url = presenter.get_audience_url();
    report.check(
        "Audience Live ON starts a local session with a QR code",
        presenter.get_audience_active()
            && url.starts_with("http://127.0.0.1:")
            && presenter.get_audience_qr().size().width > 0,
        "real loopback session became ready",
        "session or QR not ready",
    );
    report.check(
        "Audience Live ON shows page 0 on the slide window",
        windows.slide.get_audience_guide_visible()
            && windows.slide.get_audience_url() == url
            && windows.slide.get_audience_code() == presenter.get_audience_code()
            && windows.slide.get_audience_qr().size().width > 0
            && windows.slide.get_audience_overlay().row_count() == 0,
        "join metadata reached the audience window",
        "join screen or metadata missing",
    );
    report.check(
        "page 0 preserves PDF, render generation, and timer",
        state.borrow().presentation.snapshot() == before
            && state.borrow().render_generation == generation
            && state.borrow().timer.elapsed_at(now) == elapsed,
        "presentation state unchanged",
        "join screen changed the presentation",
    );
    submit_audience_smoke_reaction(&url)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while windows.slide.get_audience_overlay().row_count() == 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter)?;
    }
    let snapshot = windows.slide.window().take_snapshot()?;
    let colored = snapshot
        .as_bytes()
        .chunks_exact(4)
        .filter(|p| p[0] > 220 && p[1] > 160 && p[1] < 230 && p[2] < 120)
        .count();
    report.check(
        "Page 0 renders incoming reactions above the join information",
        windows.slide.get_audience_guide_visible()
            && windows.slide.get_audience_overlay().row_count() > 0
            && colored > 5
            && windows.slide.get_audience_url() == url
            && state.borrow().render_generation == generation,
        "reaction pixels visible on the join screen without PDF rendering",
        "page 0 reaction missing or presentation changed",
    );
    crate::handle_presentation_command(
        &windows.refs(),
        state,
        PresentationCommand::SetBlackScreen(true),
    );
    settle_presenter_notes_layout(presenter)?;
    let blank = windows.slide.window().take_snapshot()?;
    let center = ((blank.height() / 2 * blank.width() + blank.width() / 2) * 4) as usize;
    report.check(
        "blackout renders black over the join QR code",
        blank.as_bytes()[center..center + 3] == [0, 0, 0],
        "center pixel is black",
        "QR code remained visible during blackout",
    );
    report.check(
        "blackout hides the page 0 join screen",
        windows.slide.get_black_screen_active(),
        "blackout covers join information",
        "join information bypassed blackout",
    );
    crate::handle_presentation_command(
        &windows.refs(),
        state,
        PresentationCommand::SetBlackScreen(false),
    );
    crate::handle_presentation_command(&windows.refs(), state, PresentationCommand::NextPage);
    report.check(
        "next from page 0 returns to the current PDF without skipping",
        !windows.slide.get_audience_guide_visible()
            && state.borrow().presentation.snapshot() == before,
        "join screen dismissed without PDF navigation",
        "PDF page skipped or join screen remained",
    );
    if before.is_some() {
        crate::handle_presentation_command(&windows.refs(), state, PresentationCommand::FirstPage);
        let first = state.borrow().presentation.snapshot();
        let generation = state.borrow().render_generation;
        crate::handle_presentation_command(
            &windows.refs(),
            state,
            PresentationCommand::PreviousPage,
        );
        report.check(
            "Previous from PDF page 1 returns to page 0 while Audience Live is ON",
            windows.slide.get_audience_guide_visible()
                && state.borrow().presentation.snapshot() == first
                && state.borrow().render_generation == generation,
            "join screen restored without rendering or PDF navigation",
            "page 0 missing or PDF changed",
        );
        crate::handle_presentation_command(
            &windows.refs(),
            state,
            PresentationCommand::PreviousPage,
        );
        report.check(
            "Previous at page 0 remains on page 0",
            windows.slide.get_audience_guide_visible(),
            "lower boundary preserved",
            "join screen dismissed at lower boundary",
        );
        crate::handle_presentation_command(&windows.refs(), state, PresentationCommand::NextPage);
        report.check(
            "Next from restored page 0 returns to PDF page 1",
            !windows.slide.get_audience_guide_visible()
                && state.borrow().presentation.snapshot() == first,
            "first PDF page restored",
            "PDF skipped or guide remained",
        );
        if let Some(snapshot) = &before {
            crate::handle_presentation_command(
                &windows.refs(),
                state,
                PresentationCommand::JumpToPage(snapshot.current_index),
            );
        }
    }
    submit_audience_smoke_reaction(&url)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while presenter.get_audience_reaction_count() < 2 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter)?;
    }
    report.check(
        "WebSocket reaction reaches Presenter View",
        presenter.get_audience_reaction_count() == 2
            && presenter.get_audience_recent_reactions().contains("👏"),
        "authenticated reaction displayed",
        "reaction did not reach the presenter",
    );
    check_audience_overlay(windows, state, &url, report)?;
    report.check(
        "PDF navigation keeps Audience Live running",
        !state.borrow().audience_join_visible
            && !windows.slide.get_audience_guide_visible()
            && presenter.get_audience_active()
            && presenter.get_audience_toggle_label() == "Audience Live OFF 👍️"
            && presenter.get_audience_url_text() == presenter.get_audience_url(),
        "both windows returned to PDF",
        "join screen remained active",
    );
    submit_audience_smoke_reaction(&url)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while windows.slide.get_audience_overlay().row_count() == 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter)?;
    }
    report.check(
        "Reaction is visible before session stop",
        windows.slide.get_audience_overlay().row_count() > 0,
        "overlay active",
        "overlay missing before stop",
    );
    presenter.invoke_audience_toggle_session();
    report.check(
        "Stop immediately clears live reaction overlays",
        windows.slide.get_audience_overlay().row_count() == 0,
        "overlay cleared",
        "overlay survived stop",
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while presenter.get_audience_active() && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter)?;
    }
    report.check(
        "Stop clears audience QR, URLs, and join screen",
        !presenter.get_audience_active()
            && presenter.get_audience_url().is_empty()
            && windows.slide.get_audience_url().is_empty()
            && windows.slide.get_audience_qr().size().width == 0
            && !windows.slide.get_audience_guide_visible()
            && !state.borrow().audience_join_available
            && presenter.get_audience_recent_reactions().is_empty(),
        "stale join credentials cleared",
        "stale join information remained",
    );
    presenter.invoke_audience_toggle_session();
    let deadline = Instant::now() + Duration::from_secs(5);
    while presenter.get_audience_url().is_empty() && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter)?;
    }
    let new_url = presenter.get_audience_url();
    submit_audience_smoke_reaction(&new_url)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while presenter.get_audience_reaction_count() == 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter)?;
    }
    report.check(
        "Audience Live can restart with a new URL and automatically receives reactions",
        new_url != url
            && presenter.get_audience_reaction_count() == 1
            && !presenter.get_audience_recent_reactions().is_empty(),
        "new session received a reaction",
        "restart reused credentials or failed to receive",
    );
    presenter.invoke_audience_toggle_session();
    let deadline = Instant::now() + Duration::from_secs(5);
    while presenter.get_audience_active() && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter)?;
    }
    check_audience_panel_layout(windows, report)?;
    Ok(())
}

fn check_audience_panel_layout(windows: &AppWindows, report: &mut GuiSmokeReport) -> Result<()> {
    let presenter = &windows.presenter;
    let size = presenter.window().size();
    for (width, height) in [(800.0, 800.0), (1020.0, 960.0), (1200.0, 1240.0)] {
        presenter
            .window()
            .dispatch_event(slint::platform::WindowEvent::Resized {
                size: slint::LogicalSize::new(width, height),
            });
        settle_presenter_notes_layout(presenter)?;
        report.check(
            format!("Audience panel leaves thumbnails unobstructed ({width}x{height})"),
            presenter.get_audience_panel_right() + 8.0 <= presenter.get_thumbnails_left()
                && presenter.get_thumbnails_bottom() > presenter.get_notes_area_bottom()
                && (presenter.get_thumbnails_bottom() - presenter.get_audience_url_bottom()).abs()
                    <= 1.0,
            "thumbnail column stays beside the footer",
            "audience footer overlaps the thumbnail column",
        );
        report.check(
            format!(
                "Audience toggle follows notes and reactions have more space ({width}x{height})"
            ),
            presenter.get_audience_toggle_y() >= presenter.get_notes_area_bottom()
                && presenter.get_audience_panel_top()
                    >= presenter.get_audience_toggle_bottom() + 4.0
                && presenter.get_audience_reactions_width()
                    > presenter.get_audience_controls_width()
                && presenter.get_audience_url_top() >= presenter.get_audience_panel_bottom() + 4.0
                && presenter.get_audience_address_controls_inline(),
            "toggle and expanded feed fit",
            "toggle placement or feed width failed",
        );
    }
    presenter.window().set_size(size);
    presenter
        .window()
        .dispatch_event(slint::platform::WindowEvent::Resized {
            size: size.to_logical(presenter.window().scale_factor()),
        });
    settle_presenter_notes_layout(presenter)?;
    Ok(())
}

fn check_audience_overlay(
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    url: &str,
    report: &mut GuiSmokeReport,
) -> Result<()> {
    let slide = &windows.slide;
    let presenter = &windows.presenter;
    let generation = state.borrow().render_generation;
    let first = slide.get_audience_overlay().row_data(0);
    let snapshot = slide.window().take_snapshot()?;
    let colored = snapshot
        .as_bytes()
        .chunks_exact(4)
        .filter(|p| p[0] > 220 && p[1] > 160 && p[1] < 230 && p[2] < 120)
        .count();
    report.check(
        "Reaction renders an independent SVG overlay",
        first.is_some() && colored > 5,
        "overlay model and colored pixels present",
        "reaction overlay was not rendered",
    );
    let size = slide.window().size();
    slide
        .window()
        .set_size(slint::LogicalSize::new(800.0, 600.0));
    settle_presenter_notes_layout(presenter)?;
    let moving = slide.get_audience_overlay().row_data(0);
    report.check(
        "Overlay animates within resized bounds and preserves the presentation session",
        first.zip(moving).is_some_and(|(a, b)| {
            b.y < a.y && (0.0..=1.0).contains(&b.x) && (0.0..=1.0).contains(&b.y)
        }) && state.borrow().render_generation == generation,
        "motion stays normalized and render generation unchanged",
        "motion or rendering boundary failed",
    );
    slide.window().set_size(size);
    crate::handle_presentation_command(
        &windows.refs(),
        state,
        PresentationCommand::SetBlackScreen(true),
    );
    report.check(
        "Blackout immediately discards reaction overlays",
        slide.get_audience_overlay().row_count() == 0,
        "model cleared",
        "overlay remained queued",
    );
    submit_audience_smoke_reaction(url)?;
    settle_presenter_notes_layout(presenter)?;
    let blank = slide.window().take_snapshot()?;
    report.check(
        "Blackout remains black while audience reactions arrive",
        blank
            .as_bytes()
            .chunks_exact(4)
            .all(|p| p[0..3] == [0, 0, 0]),
        "all pixels black",
        "reaction bypassed blackout",
    );
    crate::handle_presentation_command(
        &windows.refs(),
        state,
        PresentationCommand::SetBlackScreen(false),
    );
    settle_presenter_notes_layout(presenter)?;
    report.check(
        "Restoring blackout never replays suppressed reactions",
        slide.get_audience_overlay().row_count() == 0,
        "no stale reactions",
        "old reaction replayed",
    );
    crate::window_controller::hide_slide_window(&windows.refs());
    submit_audience_smoke_reaction(url)?;
    settle_presenter_notes_layout(presenter)?;
    crate::window_controller::show_slide_window(&windows.refs());
    settle_presenter_notes_layout(presenter)?;
    report.check(
        "Showing the slide never replays hidden-window reactions",
        slide.get_audience_overlay().row_count() == 0,
        "hidden reactions discarded",
        "hidden reaction replayed",
    );
    submit_audience_smoke_reaction(url)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while slide.get_audience_overlay().row_count() == 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter)?;
    }
    report.check(
        "New reactions resume after showing the slide",
        slide.get_audience_overlay().row_count() > 0,
        "new input displayed",
        "new input missing",
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while slide.get_audience_overlay().row_count() > 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter)?;
    }
    report.check(
        "Reaction overlay expires without further inputs",
        slide.get_audience_overlay().row_count() == 0,
        "animation expired",
        "animation retained an old reaction",
    );
    Ok(())
}

// A minimal loopback client keeps GUI smoke independent of a new runtime dependency.
fn submit_audience_smoke_reaction(url: &str) -> Result<()> {
    use std::io::{Read, Write};
    let (url, token) = url.split_once("#k=").context("missing audience token")?;
    let authority = url
        .strip_prefix("http://")
        .context("invalid join URL")?
        .split('/')
        .next()
        .unwrap();
    let mut socket = std::net::TcpStream::connect(authority)?;
    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
    socket.set_write_timeout(Some(Duration::from_secs(2)))?;
    write!(socket, "GET /ws HTTP/1.1\r\nHost: {authority}\r\nOrigin: http://{authority}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n")?;
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") && headers.len() < 4096 {
        let mut byte = [0];
        socket.read_exact(&mut byte)?;
        headers.push(byte[0]);
    }
    anyhow::ensure!(
        headers.starts_with(b"HTTP/1.1 101"),
        "WebSocket upgrade failed"
    );
    fn send(socket: &mut std::net::TcpStream, value: serde_json::Value) -> Result<()> {
        let payload = value.to_string().into_bytes();
        anyhow::ensure!(payload.len() < 126, "smoke message too large");
        let mask = [1u8, 2, 3, 4];
        socket.write_all(&[0x81, 0x80 | payload.len() as u8])?;
        socket.write_all(&mask)?;
        socket.write_all(
            &payload
                .iter()
                .enumerate()
                .map(|(n, b)| b ^ mask[n % 4])
                .collect::<Vec<_>>(),
        )?;
        Ok(())
    }
    fn read(socket: &mut std::net::TcpStream) -> Result<serde_json::Value> {
        let mut header = [0; 2];
        socket.read_exact(&mut header)?;
        anyhow::ensure!(
            header[0] == 0x81 && header[1] < 126,
            "unexpected smoke server frame"
        );
        let mut payload = vec![0; header[1] as usize];
        socket.read_exact(&mut payload)?;
        Ok(serde_json::from_slice(&payload)?)
    }
    send(&mut socket, serde_json::json!({"v":1,"token":token}))?;
    anyhow::ensure!(
        read(&mut socket)?["type"] == "welcome",
        "audience authentication failed"
    );
    send(
        &mut socket,
        serde_json::json!({"v":1,"type":"reaction","request_id":"smoke","kind":"applause"}),
    )?;
    anyhow::ensure!(
        read(&mut socket)?["status"] == "accepted",
        "smoke reaction rejected"
    );
    Ok(())
}

fn settle_notes_layout() -> Result<()> {
    slint::Timer::single_shot(Duration::from_millis(50), || {
        slint::quit_event_loop().expect("GUI smoke event loop should accept quit");
    });
    slint::run_event_loop_until_quit().context("failed to process notes layout changes")
}

fn settle_presenter_notes_layout(presenter: &crate::PresenterWindow) -> Result<()> {
    // Paragraph models are materialized by Slint's input/rendering passes, not
    // by elapsed time alone. PointerExited runs the normal input pass without
    // clicking controls or changing focus, even when native redraw is deferred.
    presenter
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerExited);
    settle_notes_layout()
}

fn check_notes_font_size(
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    report: &mut GuiSmokeReport,
    context: &str,
) -> Result<()> {
    let presenter = &windows.presenter;
    let original_size = presenter.window().size();
    let original_aspect = presenter.get_current_page_aspect_ratio();
    let original_has_notes = presenter.get_has_notes();
    let original_text = presenter.get_notes_text();
    let original_menu = presenter.get_use_native_menu_bar();
    let before = state.borrow().presentation.snapshot();
    let generation_before = state.borrow().render_generation;
    let now = Instant::now();
    let elapsed_before = state.borrow().timer.elapsed_at(now);
    presenter
        .window()
        .dispatch_event(slint::platform::WindowEvent::Resized {
            size: slint::LogicalSize::new(800.0, 560.0 + presenter.get_audience_panel_height()),
        });

    presenter.set_has_notes(false);
    presenter.set_notes_text("".into());
    settle_presenter_notes_layout(presenter)?;
    report.check(
        format!("no-notes placeholder uses 12px ({context})"),
        presenter.get_notes_font_size() == 12.0,
        "fixed placeholder size",
        "unexpected placeholder size",
    );

    presenter.set_has_notes(true);
    presenter.set_notes_text("Opening remarks. 日本語のノート。".into());
    settle_presenter_notes_layout(presenter)?;
    report.check(
        format!("short notes automatically use 24px ({context})"),
        presenter.get_notes_font_size() == 24.0
            && presenter.get_notes_text_y().abs() <= 1.0
            && presenter.get_notes_content_height() <= presenter.get_notes_visible_height(),
        "short notes fit at the maximum size and start at the top of the viewport",
        format!(
            "size={}, text_y={}, content={}, viewport={}",
            presenter.get_notes_font_size(),
            presenter.get_notes_text_y(),
            presenter.get_notes_content_height(),
            presenter.get_notes_visible_height()
        ),
    );

    presenter.set_notes_text(
        "Long speaker notes with English and 日本語.\n"
            .repeat(150)
            .into(),
    );
    settle_presenter_notes_layout(presenter)?;
    report.check(
        format!("long notes stay readable and scrollable at 12px ({context})"),
        presenter.get_notes_font_size() == 12.0
            && presenter.get_notes_content_height() > presenter.get_notes_visible_height(),
        "overflow remains scrollable at the minimum size",
        "overflow shrank below the minimum or did not remain scrollable",
    );

    #[cfg(target_os = "linux")]
    {
        let typography = presenter.global::<crate::NotesTypography>();
        let compact_height = presenter.get_notes_content_height();
        let compact_measurement = presenter.get_notes_measured_heights().row_data(0).unwrap();
        typography.set_line_height_factor(1.25);
        settle_presenter_notes_layout(presenter)?;
        let expanded_height = presenter.get_notes_content_height();
        let expanded_measurement = presenter.get_notes_measured_heights().row_data(0).unwrap();
        typography.set_line_height_factor(1.0);
        settle_presenter_notes_layout(presenter)?;
        report.check(
            format!("Linux natural line spacing matches visible notes and sizing probes ({context})"),
            expanded_height > compact_height * 1.2
                && (compact_height - compact_measurement - crate::notes::NOTES_BOTTOM_PADDING).abs() <= 1.0
                && (expanded_height - expanded_measurement - crate::notes::NOTES_BOTTOM_PADDING).abs() <= 1.0
                && (presenter.get_notes_content_height() - compact_height).abs() <= 1.0,
            format!("12px mixed-language notes: natural={compact_height}px, 1.25x={expanded_height}px"),
            format!("visible/probe mismatch: natural={compact_height}/{compact_measurement}, 1.25x={expanded_height}/{expanded_measurement}"),
        );

        presenter.set_notes_text("English line\n日本語の行".into());
        settle_presenter_notes_layout(presenter)?;
        let lines = presenter.get_notes_measured_heights().row_data(0).unwrap();
        presenter.set_notes_text("English line\n\n日本語の行".into());
        settle_presenter_notes_layout(presenter)?;
        let one_blank = presenter.get_notes_measured_heights().row_data(0).unwrap();
        presenter.set_notes_text("English line\n\n\n日本語の行".into());
        settle_presenter_notes_layout(presenter)?;
        let two_blanks = presenter.get_notes_measured_heights().row_data(0).unwrap();
        report.check(
            format!("Linux blank note lines retain compact paragraph gaps ({context})"),
            (one_blank - lines - 6.0).abs() <= 1.0 && (two_blanks - one_blank - 6.0).abs() <= 1.0,
            format!(
                "12px probes: lines={lines}px, one blank={one_blank}px, two blanks={two_blanks}px"
            ),
            format!("unexpected blank-line spacing: {lines}, {one_blank}, {two_blanks}"),
        );
        presenter.set_notes_text("English paragraph\n日本語の段落\n\n".repeat(100).into());
        settle_presenter_notes_layout(presenter)?;
        let measurement = presenter.get_notes_measured_heights().row_data(0).unwrap();
        report.check(
            format!("Linux paragraph notes remain scrollable with matching probes ({context})"),
            presenter.get_notes_font_size() == 12.0
                && (presenter.get_notes_content_height()
                    - measurement
                    - crate::notes::NOTES_BOTTOM_PADDING)
                    .abs()
                    <= 1.0
                && presenter.get_notes_content_height() > presenter.get_notes_visible_height(),
            "paragraph gaps are included in visible content and automatic sizing",
            "paragraph overflow or measurement mismatch",
        );
    }

    presenter.set_notes_scroll_y(
        presenter.get_notes_visible_height() - presenter.get_notes_content_height(),
    );
    presenter.set_notes_text("日本語のノートを確認します。\n".repeat(8).into());
    settle_presenter_notes_layout(presenter)?;
    let small_size = presenter.get_notes_font_size();
    let heights: Vec<f32> = presenter.get_notes_measured_heights().iter().collect();
    let selected = (small_size as usize).saturating_sub(12);
    report.check(
        format!("shrinking content clamps the previous end-of-note scroll offset ({context})"),
        presenter.get_notes_scroll_y() == 0.0,
        "the fitting note returned to the top without an empty viewport",
        format!("scroll offset={}", presenter.get_notes_scroll_y()),
    );
    report.check(
        format!("wrapped notes use the largest fitting measured size ({context})"),
        small_size > 12.0
            && small_size < 24.0
            && heights
                .get(selected)
                .is_some_and(|height| *height + 10.0 <= presenter.get_notes_visible_height())
            && heights
                .iter()
                .skip(selected + 1)
                .all(|height| *height + 10.0 > presenter.get_notes_visible_height())
            && presenter.get_notes_content_height() <= presenter.get_notes_visible_height(),
        "an intermediate size fits and every larger candidate overflows",
        format!(
            "size={small_size}, measurements={heights:?}, viewport={}",
            presenter.get_notes_visible_height()
        ),
    );
    // Native configure events can constrain this request. Check the settled
    // viewport rather than assuming that a larger root window enlarges notes.
    presenter
        .window()
        .dispatch_event(slint::platform::WindowEvent::Resized {
            size: slint::LogicalSize::new(1200.0, 1000.0 + presenter.get_audience_panel_height()),
        });
    settle_presenter_notes_layout(presenter)?;
    let resized_size = presenter.get_notes_font_size();
    let resized_height = presenter.get_notes_visible_height();
    let resized_measurements: Vec<f32> = presenter.get_notes_measured_heights().iter().collect();
    let resized_selected = (resized_size as usize).saturating_sub(12);
    report.check(
        format!("resize events select the largest size fitting the settled notes viewport ({context})"),
        (12.0..=24.0).contains(&resized_size)
            && resized_size.fract() == 0.0
            && resized_measurements.len() == 13
            && resized_measurements.iter().all(|height| height.is_finite() && *height > 0.0)
            && resized_measurements.get(resized_selected).is_some_and(|height| {
                *height + crate::notes::NOTES_BOTTOM_PADDING <= resized_height
            })
            && resized_measurements.iter().skip(resized_selected + 1).all(|height| {
                *height + crate::notes::NOTES_BOTTOM_PADDING > resized_height
            })
            && presenter.get_notes_content_height() <= resized_height,
        format!("font={resized_size}px fits the settled {resized_height}px viewport"),
        format!(
            "before={small_size}, after={resized_size}, viewport={resized_height}, measurements={resized_measurements:?}, window={:?}",
            presenter.window().size()
        ),
    );

    // Change only the preview's aspect ratio to release notes space without
    // requesting an oversized native window. Preserve strict growth coverage.
    presenter.set_current_page_aspect_ratio(0.6);
    settle_presenter_notes_layout(presenter)?;
    let constrained_height = presenter.get_notes_visible_height();
    let constrained_size = presenter.get_notes_font_size();
    let controlled_window = presenter.window().size();
    presenter.set_current_page_aspect_ratio(4.0);
    settle_presenter_notes_layout(presenter)?;
    report.check(
        format!("increasing available notes space automatically enlarges text ({context})"),
        presenter.window().size() == controlled_window
            && presenter.get_notes_visible_height() > constrained_height
            && presenter.get_notes_font_size() > constrained_size
            && presenter.get_notes_font_size() == 24.0
            && presenter.get_notes_content_height() <= presenter.get_notes_visible_height(),
        format!("viewport {constrained_height} -> {}px, font {constrained_size} -> 24px", presenter.get_notes_visible_height()),
        format!("viewport {constrained_height} -> {}px, font {constrained_size} -> {}px, window {controlled_window:?} -> {:?}", presenter.get_notes_visible_height(), presenter.get_notes_font_size(), presenter.window().size()),
    );
    presenter
        .window()
        .dispatch_event(slint::platform::WindowEvent::Resized {
            size: slint::LogicalSize::new(800.0, 560.0 + presenter.get_audience_panel_height()),
        });
    presenter.set_current_page_aspect_ratio(0.6);
    presenter.set_use_native_menu_bar(false);
    settle_presenter_notes_layout(presenter)?;
    report.check(
        format!("portrait slides and inline menus retain bounded notes sizing ({context})"),
        (12.0..=24.0).contains(&presenter.get_notes_font_size())
            && presenter.get_notes_visible_height() >= 180.0,
        "minimum window retained a usable notes viewport",
        "portrait or inline menu layout exceeded the typography bounds",
    );

    presenter.set_has_notes(original_has_notes);
    presenter.set_notes_text(original_text);
    presenter.set_current_page_aspect_ratio(original_aspect);
    presenter.set_use_native_menu_bar(original_menu);
    presenter
        .window()
        .dispatch_event(slint::platform::WindowEvent::Resized {
            size: original_size.to_logical(presenter.window().scale_factor()),
        });
    settle_presenter_notes_layout(presenter)?;
    report.check(
        format!("automatic sizing preserves presentation state ({context})"),
        state.borrow().presentation.snapshot() == before
            && state.borrow().render_generation == generation_before
            && state.borrow().timer.elapsed_at(now) == elapsed_before,
        "page, render generation, and timer stayed unchanged",
        "automatic sizing unexpectedly changed presentation state",
    );
    Ok(())
}

fn report_presenter_progress(
    report: &mut GuiSmokeReport,
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    name: &'static str,
) {
    let expected_progress = state
        .borrow()
        .presentation
        .snapshot()
        .map(|snapshot| snapshot.progress_fraction());
    let actual_progress = windows.presenter.get_slide_progress_value();
    let progress_matches = expected_progress.is_some_and(|expected_progress| {
        (actual_progress - expected_progress).abs() < f32::EPSILON
    });

    report.check(
        name,
        windows.presenter.get_has_slide_progress() && progress_matches,
        "presenter progress indicator matched page state",
        format!(
            "presenter progress indicator mismatch: expected {:?}, got {}",
            expected_progress, actual_progress
        ),
    );
}

fn current_page_index(state: &AppState) -> Option<u32> {
    state
        .presentation
        .snapshot()
        .map(|snapshot| snapshot.current_index)
}

fn wait_for_async_open(
    windows: &crate::window_controller::AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
) -> Result<()> {
    let started = Instant::now();
    let mut speaker_notes_loaded = false;
    while started.elapsed() < ASYNC_OPEN_TIMEOUT {
        let drain = crate::drain_render_events(windows, state);
        speaker_notes_loaded |= drain.speaker_notes_loaded;
        if drain.open_failed {
            bail!("async render scheduler failed to open PDF");
        }
        if drain.page_failed {
            bail!("async render scheduler failed to render a page");
        }
        if drain.worker_failed {
            bail!("async render worker failed during GUI smoke");
        }
        if async_open_is_ready(&state.borrow(), speaker_notes_loaded) {
            return Ok(());
        }
        thread::sleep(ASYNC_OPEN_POLL_INTERVAL);
    }

    bail!("timed out waiting for async render scheduler to open, render, and load speaker notes")
}

fn async_open_is_ready(state: &AppState, speaker_notes_loaded: bool) -> bool {
    let Some(snapshot) = state.presentation.snapshot() else {
        return false;
    };

    let current_slide_cached = state
        .render_cache
        .peek(RenderRequest {
            page_index: snapshot.current_index,
            width: CURRENT_RENDER_WIDTH,
            purpose: RenderPurpose::CurrentSlide,
        })
        .is_some();
    let next_preview_known = snapshot.total_pages == 1 || snapshot.next_index.is_some();

    current_slide_cached && next_preview_known && speaker_notes_loaded
}

fn write_report(path: &Path, output: &str) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create report directory: {}", parent.display()))?;
    }
    fs::write(path, output)
        .with_context(|| format!("failed to write GUI smoke report: {}", path.display()))
}

struct GuiSmokeReport {
    pdf_path: String,
    checks: Vec<GuiSmokeCheck>,
}

impl GuiSmokeReport {
    fn new(pdf_path: String) -> Self {
        Self {
            pdf_path,
            checks: Vec::new(),
        }
    }

    fn pass(&mut self, name: impl Into<String>) {
        self.checks.push(GuiSmokeCheck {
            name: name.into(),
            passed: true,
            detail: "ok".to_owned(),
        });
    }

    fn fail(&mut self, name: impl Into<String>, detail: impl Into<String>) {
        self.checks.push(GuiSmokeCheck {
            name: name.into(),
            passed: false,
            detail: detail.into(),
        });
    }

    fn check(
        &mut self,
        name: impl Into<String>,
        passed: bool,
        pass_detail: impl Into<String>,
        fail_detail: impl Into<String>,
    ) {
        self.checks.push(GuiSmokeCheck {
            name: name.into(),
            passed,
            detail: if passed {
                pass_detail.into()
            } else {
                fail_detail.into()
            },
        });
    }

    fn passed_count(&self) -> usize {
        self.checks.iter().filter(|check| check.passed).count()
    }

    fn failed_count(&self) -> usize {
        self.checks.iter().filter(|check| !check.passed).count()
    }

    fn render(&self) -> String {
        let mut output = String::new();
        output.push_str("Quick Presenter GUI Smoke Report\n");
        output.push_str(&format!("PDF: {}\n", self.pdf_path));
        output.push_str(&format!(
            "Result: {} passed, {} failed\n\n",
            self.passed_count(),
            self.failed_count()
        ));

        for check in &self.checks {
            let status = if check.passed { "PASS" } else { "FAIL" };
            output.push_str(&format!("- {status}: {} ({})\n", check.name, check.detail));
        }

        output
    }
}

struct GuiSmokeCheck {
    name: String,
    passed: bool,
    detail: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{presentation::PresentationState, rendering::RenderedPage};
    use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

    fn cached_page() -> RenderedPage {
        let pixels = SharedPixelBuffer::<Rgba8Pixel>::new(1, 1);
        RenderedPage {
            image: Image::from_rgba8(pixels),
            aspect_ratio: 1.0,
            estimated_bytes: 4,
        }
    }

    #[test]
    fn report_counts_passed_and_failed_checks() {
        let mut report = GuiSmokeReport::new("deck.pdf".to_owned());
        report.pass("created windows");
        report.fail("opened PDF", "missing file");

        assert_eq!(report.passed_count(), 1);
        assert_eq!(report.failed_count(), 1);

        let rendered = report.render();
        assert!(rendered.contains("Quick Presenter GUI Smoke Report"));
        assert!(rendered.contains("PASS: created windows"));
        assert!(rendered.contains("FAIL: opened PDF"));
    }

    #[test]
    fn async_open_ready_requires_presentation_current_render_and_notes_status() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 2),
            status_text: "Ready".to_owned(),
            ..AppState::default()
        };

        assert!(!async_open_is_ready(&state, false));

        state.render_cache.insert(
            RenderRequest {
                page_index: 0,
                width: CURRENT_RENDER_WIDTH,
                purpose: RenderPurpose::CurrentSlide,
            },
            cached_page(),
        );

        assert!(!async_open_is_ready(&state, false));
        assert!(async_open_is_ready(&state, true));
    }

    #[test]
    fn async_open_ready_accepts_speaker_notes_warning_status() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 1),
            status_text: "Could not read speaker notes".to_owned(),
            ..AppState::default()
        };

        state.render_cache.insert(
            RenderRequest {
                page_index: 0,
                width: CURRENT_RENDER_WIDTH,
                purpose: RenderPurpose::CurrentSlide,
            },
            cached_page(),
        );

        assert!(async_open_is_ready(&state, true));
    }
}

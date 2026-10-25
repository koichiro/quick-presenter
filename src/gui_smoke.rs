#[cfg(target_os = "macos")]
mod macos_fullscreen;

use std::{
    cell::{Cell, RefCell},
    fs,
    path::Path,
    rc::Rc,
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
    window_controller::{set_audience_fullscreen, AppWindows},
};

const ASYNC_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const ASYNC_OPEN_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub fn run(options: GuiSmokeOptions) -> Result<()> {
    let windows = AppWindows::new().context("failed to create Slint windows")?;
    let completed = Rc::new(RefCell::new(None));
    let outcome = completed.clone();
    slint::spawn_local(async move {
        let mut report = GuiSmokeReport::new(options.pdf_path.display().to_string());
        if let Err(err) = run_checks(windows, &options, &mut report).await {
            report.fail("run completed", format!("{err:#}"));
        }
        *outcome.borrow_mut() = Some((options, report));
        slint::quit_event_loop().expect("GUI smoke loop should accept quit");
    })?;
    // Exit only after every check finishes. On macOS an exit closes native
    // windows and clears their delegates, even if Slint retains its adapters.
    slint::run_event_loop_until_quit().context("failed to run GUI smoke")?;
    let (options, report) = completed
        .borrow_mut()
        .take()
        .context("GUI smoke interrupted")?;

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

async fn run_checks(
    windows: AppWindows,
    options: &GuiSmokeOptions,
    report: &mut GuiSmokeReport,
) -> Result<()> {
    crate::configure_linux_desktop_identity()?;
    crate::configure_shortcut_modifiers(&windows);
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState::default()));

    crate::wire_callbacks(&windows, windows.refs(), state.clone());
    crate::apply_app_metadata(&windows.presenter);
    let _audience_ui = crate::audience_ui::AudienceUi::install_for_smoke(&windows, state.clone());

    windows.apply_initial_positions();
    windows
        .audience
        .show()
        .context("failed to show audience window")?;
    #[cfg(target_os = "macos")]
    let transitions = macos_fullscreen::Transitions::new(&windows.audience)?;
    #[cfg(target_os = "macos")]
    {
        use slint::winit_030::WinitWindowAccessor;
        check_macos_fullscreen_reversal(&windows, report, &transitions).await?;
        settle_notes_layout().await?;
        report.check(
            "macOS native audience titlebar is transparent and hidden from creation",
            macos_audience_chrome_is_correct(&windows.audience),
            "AppKit full-size content, transparent titlebar, and hidden title are set",
            "initial native titlebar attributes are incomplete",
        );
        report.check(
            "macOS audience content includes the titlebar from creation",
            windows
                .audience
                .window()
                .with_winit_window(|native| native.inner_size() == native.outer_size())
                == Some(true),
            "native audience viewport covers the entire window before chrome synchronization",
            "native audience viewport excludes the titlebar",
        );
    }
    let window_refs = windows.refs();
    crate::apply_macos_audience_window_chrome(&window_refs);
    crate::sync_audience_chrome(&window_refs);
    windows
        .presenter
        .show()
        .context("failed to show presenter window")?;
    crate::set_application_icon();

    report.pass("created presenter and audience windows");
    report.check(
        "presenter window handle is alive",
        window_refs.presenter.upgrade().is_some(),
        "presenter window weak handle upgraded",
        "presenter window weak handle could not be upgraded",
    );
    report.check(
        "audience window handle is alive",
        window_refs.audience.upgrade().is_some(),
        "audience window weak handle upgraded",
        "audience window weak handle could not be upgraded",
    );

    check_notes_font_size(&windows, &state, report, "before opening a PDF").await?;
    check_audience_join_screen(&windows, &state, report).await?;

    crate::begin_open_pdf(&window_refs, &state, options.pdf_path.clone());
    report.check(
        "slide progress indicator is hidden while opening without a deck",
        !windows.presenter.get_has_slide_progress()
            && windows.presenter.get_slide_progress_value() == 0.0,
        "presenter progress indicator is empty",
        "presenter progress indicator was unexpectedly visible",
    );
    wait_for_async_open(&window_refs, &state)
        .await
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
    check_notes_font_size(&windows, &state, report, "with a PDF open").await?;
    check_audience_join_screen(&windows, &state, report).await?;
    check_settings_dialog(&windows, &state, report).await?;
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
        "black screen command restores audience output",
        |state| !state.black_screen.is_active() && current_page_index(state) == Some(1),
    );

    {
        let fullscreen = state.borrow_mut().fullscreen.toggle_audience_fullscreen();
        set_audience_fullscreen(&window_refs, fullscreen);
    }
    report_state(report, &state, "fullscreen state toggles on", |state| {
        state.fullscreen.is_audience_fullscreen()
    });
    crate::handle_presentation_command(
        &window_refs,
        &state,
        PresentationCommand::ExitAudienceFullscreen,
    );
    report_state(
        report,
        &state,
        "fullscreen exit command toggles off",
        |state| !state.fullscreen.is_audience_fullscreen(),
    );

    report_state(
        report,
        &state,
        "presenter and audience state remain synchronized",
        |state| {
            state.presentation.snapshot().is_some()
                && !state.black_screen.is_active()
                && !state.fullscreen.is_audience_fullscreen()
        },
    );

    #[cfg(all(unix, not(all(target_os = "macos", feature = "macos-store-control"))))]
    {
        crate::control_smoke::run(&windows, state.clone(), options.pdf_path.clone()).await?;
        report.pass("local control operates through the GUI and isolated renderer");
    }
    check_hidden_audience_recovery(
        &windows,
        &state,
        report,
        #[cfg(target_os = "macos")]
        &transitions,
    )
    .await?;
    #[cfg(target_os = "macos")]
    {
        check_macos_audience_edges(&windows, report, &transitions).await?;
        check_macos_preserved_pdf_open(&windows, &state, options, report).await?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
async fn check_macos_audience_edges(
    windows: &AppWindows,
    report: &mut GuiSmokeReport,
    transitions: &macos_fullscreen::Transitions,
) -> Result<()> {
    use slint::winit_030::WinitWindowAccessor;
    let audience = &windows.audience;
    let original_image = audience.get_page_image();
    let original_aspect = audience.get_page_aspect_ratio();
    let original_size = audience
        .window()
        .size()
        .to_logical(audience.window().scale_factor());
    // Narrow edge bands disappear if the old 28-point offset crops the slide.
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(160, 90);
    for (row, pixels) in buffer
        .make_mut_bytes()
        .chunks_exact_mut(160 * 4)
        .enumerate()
    {
        let color = if row < 3 {
            [220, 30, 40, 255]
        } else if row >= 87 {
            [20, 210, 60, 255]
        } else {
            [30, 80, 220, 255]
        };
        for (column, pixel) in pixels.chunks_exact_mut(4).enumerate() {
            let color = if column < 3 {
                [240, 180, 20, 255]
            } else if column >= 157 {
                [180, 20, 210, 255]
            } else {
                color
            };
            pixel.copy_from_slice(&color);
        }
    }
    audience.set_page_image(slint::Image::from_rgba8(buffer));
    audience.set_page_aspect_ratio(16.0 / 9.0);
    let refs = windows.refs();
    let display_size = audience
        .window()
        .with_winit_window(|native| {
            native
                .current_monitor()
                .or_else(|| native.primary_monitor())
                .map(|monitor| monitor.size().to_logical::<f64>(monitor.scale_factor()))
        })
        .flatten()
        .context("native edge checks require an available display")?;
    for (name, width, height) in [
        ("windowed", 960.0, 540.0),
        ("resized", 1280.0, 720.0),
        ("restored tall window", 1704.0, 1063.0),
        ("after fullscreen", 960.0, 540.0),
    ] {
        let scale = (display_size.width * 0.8 / width)
            .min(display_size.height * 0.8 / height)
            .min(1.0);
        let width = width * scale;
        let height = height * scale;
        if name == "after fullscreen" {
            settle_audience_fullscreen(&refs, true, transitions).await?;
            settle_audience_fullscreen(&refs, false, transitions).await?;
        }
        let action_refs = refs.clone();
        run_macos_window_action(move || {
            crate::window_controller::set_audience_logical_size(&action_refs, [width, height]);
            if name == "restored tall window" {
                let preserve = action_refs.placement.preserve_size.replace(true);
                crate::fit_audience_window_to_aspect_ratio(&action_refs, 16.0 / 9.0);
                action_refs.placement.preserve_size.set(preserve);
            }
            crate::sync_audience_chrome(&action_refs);
        })
        .await?;
        check_macos_native_audience_edges(audience, report, name)?;
    }

    let unrelated_tick = Rc::new(Cell::new(false));
    let ticked = unrelated_tick.clone();
    let unrelated_timer = slint::Timer::default();
    unrelated_timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(20),
        move || ticked.set(true),
    );
    let action_count = Rc::new(Cell::new(0));
    let counted = action_count.clone();
    let started = Instant::now();
    run_macos_window_action(move || counted.set(counted.get() + 1)).await?;
    unrelated_timer.stop();
    report.check(
        "macOS window actions wait for their own condition across unrelated timers",
        unrelated_tick.get()
            && action_count.get() == 1
            && started.elapsed() >= Duration::from_secs(1),
        "action ran once and its own settling condition completed",
        "action was skipped, repeated, or ended before settling",
    );

    let timed_out = run_macos_window_action_until(|| {}, || false, Duration::from_millis(30))
        .await
        .is_err();
    let next_action_ran = Rc::new(Cell::new(false));
    let ran = next_action_ran.clone();
    run_macos_window_action_until(move || ran.set(true), || true, Duration::from_secs(5)).await?;
    report.check(
        "macOS window action timeout does not leak into the next check",
        timed_out && next_action_ran.get(),
        "deadline failed explicitly and the next action completed",
        "timeout did not fail or a later action was interrupted",
    );

    // Exercise the actual startup planner and its asynchronous verification,
    // rather than directly invoking the PDF fitting helper.
    let saved = audience
        .window()
        .with_winit_window(|native| {
            let monitor = native
                .primary_monitor()
                .or_else(|| native.available_monitors().next())?;
            let scale = monitor.scale_factor();
            let origin = monitor.position().to_logical::<f64>(scale);
            let extent = monitor.size().to_logical::<f64>(scale);
            Some(crate::window_placement::PlacementRecord {
                platform: "macos".into(),
                source_display: crate::window_placement::DisplayRect {
                    origin: [origin.x.round() as i32, origin.y.round() as i32],
                    extent: [extent.width.round() as u32, extent.height.round() as u32],
                    scale,
                },
                relative_center: [0.5, 0.5],
                logical_inner_size: [960.0, 540.0],
            })
        })
        .flatten()
        .context("macOS regression requires an available display")?;
    for (name, width, height, aspect) in [
        ("startup tall widescreen", 1704.0, 1063.0, 16.0 / 9.0),
        ("startup short widescreen", 1500.0, 700.0, 16.0 / 9.0),
        ("startup standard", 1000.0, 700.0, 4.0 / 3.0),
    ] {
        // Keep fixtures inside the native display on smaller CI desktops, while
        // preserving the mismatched saved/page ratios that caused the regression.
        let scale = (f64::from(saved.source_display.extent[0]) * 0.9 / width)
            .min(f64::from(saved.source_display.extent[1]) * 0.9 / height)
            .min(1.0);
        let width = width * scale;
        let height = height * scale;
        // Distort the image to the test page ratio; edge colors remain intact.
        audience.set_page_aspect_ratio(aspect);
        let mut record = saved.clone();
        record.logical_inner_size = [width, height];
        refs.placement.initialize(Some(record));
        let action_refs = refs.clone();
        let verification_refs = refs.clone();
        run_macos_window_action_until(
            move || crate::window_controller::start_audience_placement(action_refs),
            move || verification_refs.placement.restore_verified.get(),
            Duration::from_secs(5),
        )
        .await?;
        report.check(
            format!("macOS {name} restoration completes"),
            refs.placement.restore_verified.get(),
            "startup restore settled through the production verification path",
            "startup restore did not verify the saved geometry",
        );
        let logical = audience
            .window()
            .size()
            .to_logical(audience.window().scale_factor());
        report.check(
            format!("macOS {name} preserves scale within saved bounds"),
            logical.width <= width as f32 + 2.0
                && logical.height <= height as f32 + 2.0
                && ((logical.width - width as f32).abs() < 2.0
                    || (logical.height - height as f32).abs() < 2.0),
            "one saved dimension is retained and the other fits the page",
            format!("unexpected restored size {logical:?} for {width}x{height}"),
        );
        check_macos_native_audience_edges(audience, report, name)?;
    }
    refs.placement.cancel();
    refs.placement.enabled.set(false);
    audience.set_page_image(original_image);
    audience.set_page_aspect_ratio(original_aspect);
    run_macos_window_action(move || {
        crate::window_controller::set_audience_logical_size(
            &refs,
            [
                f64::from(original_size.width),
                f64::from(original_size.height),
            ],
        );
    })
    .await?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_native_fullscreen(audience: &crate::AudienceWindow) -> Option<bool> {
    use objc2_app_kit::{NSView, NSWindowStyleMask};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = audience.window().window_handle();
    let handle = handle.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // The live Slint adapter owns the view, and GUI smoke runs on the main thread.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let window = view.window()?;
    window.delegate()?;
    Some(window.styleMask().contains(NSWindowStyleMask::FullScreen))
}

#[cfg(target_os = "macos")]
async fn check_macos_fullscreen_reversal(
    windows: &AppWindows,
    report: &mut GuiSmokeReport,
    transitions: &macos_fullscreen::Transitions,
) -> Result<()> {
    use slint::winit_030::WinitWindowAccessor;
    let refs = windows.refs();
    for (name, sequence, expected) in [
        ("reversal baseline", &[false][..], false),
        ("rapid entry cancellation", &[true, false][..], false),
        ("settled entry", &[true][..], true),
        ("rapid exit cancellation", &[false, true][..], true),
        ("reversal cleanup", &[false][..], false),
    ] {
        let before = transitions.counts();
        let started = Instant::now();
        set_audience_fullscreen(&refs, sequence[0]);
        if sequence.len() == 2 {
            let will = if sequence[0] { 0 } else { 2 };
            while transitions.counts()[will] == before[will] {
                if started.elapsed() >= Duration::from_secs(5) {
                    bail!("macOS {name}: initial transition never started");
                }
                delay(Duration::from_millis(10)).await;
            }
            anyhow::ensure!(
                transitions.counts()[will + 1] == before[will + 1],
                "macOS {name}: initial transition finished before reversal"
            );
            set_audience_fullscreen(&refs, sequence[1]);
        }
        loop {
            let actual = (
                macos_native_fullscreen(&windows.audience),
                windows.audience.window().is_fullscreen(),
                windows
                    .audience
                    .window()
                    .with_winit_window(|native| native.fullscreen().is_some()),
            );
            let counts = transitions.counts();
            let notifications_match = if sequence.len() == 2 {
                // Both actual native transitions must finish; a dropped first
                // request cannot pass just because the final state is unchanged.
                counts
                    .iter()
                    .zip(before)
                    .all(|(count, previous)| *count == previous + 1)
            } else if name == "settled entry" {
                counts[0] == before[0] + 1 && counts[1] == before[1] + 1
            } else if name == "reversal cleanup" {
                counts[2] == before[2] + 1 && counts[3] == before[3] + 1
            } else {
                counts == before
            };
            if started.elapsed() >= Duration::from_secs(1)
                && notifications_match
                && actual == (Some(expected), expected, Some(expected))
            {
                report.pass(format!(
                    "macOS {name} matches AppKit fullscreen state and notifications"
                ));
                break;
            }
            if started.elapsed() >= Duration::from_secs(5) {
                bail!("macOS {name} did not settle: state={actual:?}, notifications={before:?} -> {counts:?}");
            }
            delay(Duration::from_millis(10)).await;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_audience_chrome_is_correct(audience: &crate::AudienceWindow) -> bool {
    use objc2_app_kit::{NSView, NSWindowStyleMask, NSWindowTitleVisibility};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = audience.window().window_handle();
    let Ok(handle) = handle.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return false;
    };
    // GUI smoke runs on the main thread; the Slint adapter owns this live view.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    view.window().is_some_and(|window| {
        window.delegate().is_some()
            && window
                .styleMask()
                .contains(NSWindowStyleMask::FullSizeContentView)
            && window.titlebarAppearsTransparent()
            && window.titleVisibility() == NSWindowTitleVisibility::Hidden
    })
}

#[cfg(target_os = "macos")]
fn check_macos_native_audience_edges(
    audience: &crate::AudienceWindow,
    report: &mut GuiSmokeReport,
    name: &str,
) -> Result<()> {
    use slint::winit_030::WinitWindowAccessor;
    report.check(
        format!("macOS {name} native titlebar remains transparent"),
        macos_audience_chrome_is_correct(audience),
        "native titlebar allows the slide behind window buttons",
        "native titlebar attributes were lost",
    );
    let snapshot = audience.window().take_snapshot()?;
    let native_size = audience.window().with_winit_window(|native| {
        (
            native.inner_size(),
            native.outer_size(),
            native.fullscreen(),
        )
    });
    report.check(
        format!("macOS {name} renderer covers the native window including the titlebar"),
        audience.window().with_winit_window(|native| {
            let size = native.outer_size();
            size.width == snapshot.width() && size.height == snapshot.height()
        }) == Some(true),
        "renderer and complete native window dimensions match",
        format!(
            "renderer {}x{}, Slint {:?}, native {native_size:?}",
            snapshot.width(),
            snapshot.height(),
            audience.window().size()
        ),
    );
    anyhow::ensure!(
        snapshot.width() >= 8 && snapshot.height() >= 8,
        "audience snapshot is too small"
    );
    for fraction in 1..=3 {
        for (edge, x, y, color) in [
            ("top", snapshot.width() * fraction / 4, 2, [220_u8, 30, 40]),
            (
                "bottom",
                snapshot.width() * fraction / 4,
                snapshot.height() - 3,
                [20, 210, 60],
            ),
            ("left", 2, snapshot.height() * fraction / 4, [240, 180, 20]),
            (
                "right",
                snapshot.width() - 3,
                snapshot.height() * fraction / 4,
                [180, 20, 210],
            ),
        ] {
            let index = ((y * snapshot.width() + x) * 4) as usize;
            let pixel = &snapshot.as_bytes()[index..index + 3];
            report.check(
                format!("macOS {name} audience {edge} edge at {fraction}/4 fills the window without cropping"),
                pixel.iter().zip(color).all(|(actual, expected)| actual.abs_diff(expected) <= 2),
                "the original slide edge reaches the window edge",
                format!("pixel at ({x}, {y}) is {pixel:?}, expected {color:?}"),
            );
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn check_macos_preserved_pdf_open(
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    options: &GuiSmokeOptions,
    report: &mut GuiSmokeReport,
) -> Result<()> {
    let refs = windows.refs();
    refs.placement.preserve_size.set(true);
    let action_refs = refs.clone();
    run_macos_window_action(move || {
        crate::window_controller::set_audience_logical_size(&action_refs, [1704.0, 1063.0]);
    })
    .await?;
    crate::begin_open_pdf(&refs, state, options.pdf_path.clone());
    wait_for_async_open(&refs, state).await?;
    run_macos_window_action(|| {}).await?;
    let size = windows
        .audience
        .window()
        .size()
        .to_logical(windows.audience.window().scale_factor());
    let aspect = windows.audience.get_page_aspect_ratio();
    report.check(
        "macOS opening a PDF refits a preserved window through the async open path",
        (size.width - size.height * aspect).abs() < 2.0,
        "preserved window dimensions match the rendered PDF aspect ratio",
        format!("preserved window {size:?} does not match PDF aspect {aspect}"),
    );
    refs.placement.preserve_size.set(false);
    Ok(())
}

#[cfg(target_os = "macos")]
async fn run_macos_window_action(action: impl FnOnce() + 'static) -> Result<()> {
    let applied_at = Rc::new(Cell::new(None::<Instant>));
    let observed = applied_at.clone();
    run_macos_window_action_until(
        move || {
            action();
            applied_at.set(Some(Instant::now()));
        },
        move || {
            observed
                .get()
                .is_some_and(|started| started.elapsed() >= Duration::from_secs(1))
        },
        Duration::from_secs(5),
    )
    .await
}

#[cfg(target_os = "macos")]
async fn run_macos_window_action_until(
    action: impl FnOnce() + 'static,
    settled: impl Fn() -> bool + 'static,
    timeout: Duration,
) -> Result<()> {
    // Yield before the action so native callbacks run with Winit's handler active.
    delay(Duration::ZERO).await;
    action();
    let started = Instant::now();
    loop {
        if settled() {
            return Ok(());
        }
        if started.elapsed() >= timeout {
            bail!("macOS window action did not settle before its deadline");
        }
        delay(Duration::from_millis(10)).await;
    }
}

async fn settle_audience_fullscreen(
    refs: &crate::window_controller::AppWindowRefs,
    fullscreen: bool,
    #[cfg(target_os = "macos")] transitions: &macos_fullscreen::Transitions,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use slint::winit_030::WinitWindowAccessor;
        let started = Instant::now();
        // The suite-long observer also sees requests made by earlier IPC checks.
        // Finish those before deciding whether this request is a native no-op.
        loop {
            let counts = transitions.counts();
            if counts[0] == counts[1] && counts[2] == counts[3] {
                break;
            }
            if started.elapsed() >= Duration::from_secs(5) {
                bail!("prior fullscreen transition did not complete: {counts:?}");
            }
            delay(Duration::from_millis(10)).await;
        }
        let audience = refs
            .audience
            .upgrade()
            .context("hidden-window fixture lost its audience")?;
        let initial =
            macos_native_fullscreen(&audience).context("native audience delegate is missing")?;
        let before = transitions.counts();
        let did = if fullscreen { 1 } else { 3 };
        set_audience_fullscreen(refs, fullscreen);
        loop {
            let counts = transitions.counts();
            let completed = counts[0] == counts[1]
                && counts[2] == counts[3]
                && (initial == fullscreen || counts[did] > before[did]);
            let actual = (
                macos_native_fullscreen(&audience),
                audience.window().is_fullscreen(),
                audience
                    .window()
                    .with_winit_window(|native| native.fullscreen().is_some()),
            );
            if completed && actual == (Some(fullscreen), fullscreen, Some(fullscreen)) {
                return Ok(());
            }
            if started.elapsed() >= Duration::from_secs(5) {
                bail!("hidden-window fullscreen={fullscreen} did not complete: state={actual:?}, notifications={before:?} -> {counts:?}");
            }
            delay(Duration::from_millis(10)).await;
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        set_audience_fullscreen(refs, fullscreen);
        settle_notes_layout().await
    }
}

async fn check_hidden_audience_recovery(
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    report: &mut GuiSmokeReport,
    #[cfg(target_os = "macos")] transitions: &macos_fullscreen::Transitions,
) -> Result<()> {
    let refs = windows.refs();
    let frames = Rc::new(Cell::new(0_u32));
    let rendered_frames = frames.clone();
    windows
        .audience
        .window()
        .set_rendering_notifier(move |phase, _| {
            if matches!(phase, slint::RenderingState::AfterRendering) {
                rendered_frames.set(rendered_frames.get() + 1);
            }
        })
        .context("failed to monitor audience rendering for hidden-window recovery")?;
    settle_notes_layout().await?;

    for fullscreen in [false, true] {
        settle_audience_fullscreen(
            &refs,
            fullscreen,
            #[cfg(target_os = "macos")]
            transitions,
        )
        .await?;
        for (close_request, raise, color) in [
            (true, false, [210, 30, 90, 255]),
            (true, true, [20, 160, 200, 255]),
            (false, false, [90, 180, 30, 255]),
            (false, true, [160, 40, 200, 255]),
        ] {
            if close_request {
                windows
                    .audience
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::CloseRequested);
            } else {
                crate::hide_audience_window_from_menu(&refs, state);
            }
            report.check(
                format!(
                    "audience is hidden through {}",
                    if close_request {
                        "close request"
                    } else {
                        "Hide Audience"
                    }
                ),
                !windows.audience.window().is_visible()
                    && !state.borrow().window_menu.audience_visible(),
                "Slint lifecycle is hidden",
                "native hide left the Slint lifecycle visible",
            );
            // Different solid images make stale backing content unambiguous and
            // avoid relying on the appearance of the caller's PDF fixture.
            let buffer =
                slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&color, 1, 1);
            windows
                .audience
                .set_page_image(slint::Image::from_rgba8(buffer));
            settle_notes_layout().await?;
            let previous_frames = frames.get();
            if raise {
                crate::bring_audience_window_to_front(&refs, state);
            } else {
                crate::show_audience_window_from_menu(&refs, state);
            }
            settle_notes_layout().await?;
            let action = format!(
                "{} ({})",
                if raise {
                    "Bring Audience to Front"
                } else {
                    "Show Audience"
                },
                if fullscreen { "fullscreen" } else { "windowed" }
            );
            report.check(
                format!("{action} restores the audience rendering lifecycle"),
                windows.audience.window().is_visible() && frames.get() > previous_frames,
                "Slint is visible and rendered a new frame",
                "Slint is hidden or no new frame was rendered",
            );
            let snapshot = windows
                .audience
                .window()
                .take_snapshot()
                .context("failed to capture the restored audience")?;
            let center = &snapshot.as_bytes()[((snapshot.height() / 2 * snapshot.width()
                + snapshot.width() / 2)
                * 4) as usize..];
            report.check(
                format!("{action} renders the image changed while hidden"),
                center[..3]
                    .iter()
                    .zip(color[..3].iter())
                    .all(|(actual, expected)| actual.abs_diff(*expected) <= 2),
                "restored audience has the replacement image pixels",
                format!(
                    "unexpected center pixel: {:?}, expected {:?}",
                    &center[..4],
                    color
                ),
            );
        }
    }
    settle_audience_fullscreen(
        &refs,
        false,
        #[cfg(target_os = "macos")]
        transitions,
    )
    .await?;
    if let Some(snapshot) = state.borrow().presentation.snapshot() {
        crate::apply_snapshot_to_windows(&refs, &state.borrow(), &snapshot);
    }
    settle_notes_layout().await?;
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

async fn check_audience_join_screen(
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
        presenter.get_audience_toggle_label() == "Audience Live ON  👍️"
            && !presenter.get_audience_panel_expanded(),
        "start action displayed",
        "incorrect start label",
    );
    presenter.invoke_audience_toggle_panel();
    settle_presenter_notes_layout(presenter).await?;
    report.check(
        "Inactive Audience Live history can be manually opened",
        presenter.get_audience_panel_expanded() && !presenter.get_audience_active(),
        "history opened without starting a session",
        "disclosure changed session state or did not open history",
    );
    presenter.invoke_audience_toggle_panel();
    presenter.invoke_audience_toggle_session();
    let deadline = Instant::now() + Duration::from_secs(5);
    while presenter.get_audience_url().is_empty() && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter).await?;
    }
    let url = presenter.get_audience_url();
    report.check(
        "Audience Live ON starts a local session with a QR code",
        presenter.get_audience_active()
            && presenter.get_audience_panel_expanded()
            && url.starts_with("http://127.0.0.1:")
            && presenter.get_audience_qr().size().width > 0,
        "real loopback session became ready",
        "session or QR not ready",
    );
    presenter.invoke_audience_toggle_panel();
    settle_presenter_notes_layout(presenter).await?;
    report.check(
        "Active Audience Live history can collapse without stopping reception",
        !presenter.get_audience_panel_expanded()
            && presenter.get_audience_active()
            && presenter.get_audience_url() == url,
        "session and join URL remained active",
        "collapsing history changed the session",
    );
    presenter.invoke_audience_toggle_panel();
    report.check(
        "Audience Live ON shows page 0 on the audience window",
        windows.audience.get_audience_guide_visible()
            && windows.audience.get_audience_url() == url
            && windows.audience.get_audience_code() == presenter.get_audience_code()
            && windows.audience.get_audience_qr().size().width > 0
            && windows.audience.get_audience_overlay().row_count() == 0,
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
    while windows.audience.get_audience_overlay().row_count() == 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter).await?;
    }
    let snapshot = windows.audience.window().take_snapshot()?;
    let colored = snapshot
        .as_bytes()
        .chunks_exact(4)
        .filter(|p| p[0] > 220 && p[1] > 160 && p[1] < 230 && p[2] < 120)
        .count();
    report.check(
        "Page 0 renders incoming reactions above the join information",
        windows.audience.get_audience_guide_visible()
            && windows.audience.get_audience_overlay().row_count() > 0
            && colored > 5
            && windows.audience.get_audience_url() == url
            && state.borrow().render_generation == generation,
        "reaction pixels visible on the join screen without PDF rendering",
        "page 0 reaction missing or presentation changed",
    );
    crate::handle_presentation_command(
        &windows.refs(),
        state,
        PresentationCommand::SetBlackScreen(true),
    );
    settle_presenter_notes_layout(presenter).await?;
    let blank = windows.audience.window().take_snapshot()?;
    let center = ((blank.height() / 2 * blank.width() + blank.width() / 2) * 4) as usize;
    report.check(
        "blackout renders black over the join QR code",
        blank.as_bytes()[center..center + 3] == [0, 0, 0],
        "center pixel is black",
        "QR code remained visible during blackout",
    );
    report.check(
        "blackout hides the page 0 join screen",
        windows.audience.get_black_screen_active(),
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
        !windows.audience.get_audience_guide_visible()
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
            windows.audience.get_audience_guide_visible()
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
            windows.audience.get_audience_guide_visible(),
            "lower boundary preserved",
            "join screen dismissed at lower boundary",
        );
        crate::handle_presentation_command(&windows.refs(), state, PresentationCommand::NextPage);
        report.check(
            "Next from restored page 0 returns to PDF page 1",
            !windows.audience.get_audience_guide_visible()
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
        settle_presenter_notes_layout(presenter).await?;
    }
    report.check(
        "WebSocket reaction reaches Presenter View",
        presenter.get_audience_reaction_count() == 2
            && presenter.get_audience_recent_reactions().contains("👏"),
        "authenticated reaction displayed",
        "reaction did not reach the presenter",
    );
    check_audience_overlay(windows, state, &url, report).await?;
    check_live_comments(windows, state, &url, report).await?;
    report.check(
        "PDF navigation keeps Audience Live running",
        !state.borrow().audience_join_visible
            && !windows.audience.get_audience_guide_visible()
            && presenter.get_audience_active()
            && presenter.get_audience_toggle_label() == "Audience Live OFF 👍️"
            && presenter.get_audience_url_text() == presenter.get_audience_url(),
        "both windows returned to PDF",
        "join screen remained active",
    );
    submit_audience_smoke_reaction(&url)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while windows.audience.get_audience_overlay().row_count() == 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter).await?;
    }
    report.check(
        "Reaction is visible before session stop",
        windows.audience.get_audience_overlay().row_count() > 0,
        "overlay active",
        "overlay missing before stop",
    );
    presenter.invoke_audience_toggle_session();
    report.check(
        "Stop immediately clears live reaction overlays",
        windows.audience.get_audience_overlay().row_count() == 0,
        "overlay cleared",
        "overlay survived stop",
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while presenter.get_audience_active() && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter).await?;
    }
    report.check(
        "Stop clears audience QR, URLs, and join screen",
        !presenter.get_audience_active()
            && !presenter.get_audience_panel_expanded()
            && presenter.get_audience_url().is_empty()
            && windows.audience.get_audience_url().is_empty()
            && windows.audience.get_audience_qr().size().width == 0
            && !windows.audience.get_audience_guide_visible()
            && !state.borrow().audience_join_available
            && presenter.get_audience_recent_reactions().is_empty(),
        "stale join credentials cleared",
        "stale join information remained",
    );
    presenter.invoke_audience_toggle_session();
    let deadline = Instant::now() + Duration::from_secs(5);
    while presenter.get_audience_url().is_empty() && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter).await?;
    }
    let new_url = presenter.get_audience_url();
    submit_audience_smoke_reaction(&new_url)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while presenter.get_audience_reaction_count() == 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter).await?;
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
        settle_presenter_notes_layout(presenter).await?;
    }
    check_audience_panel_layout(windows, report).await?;
    Ok(())
}

async fn check_audience_panel_layout(
    windows: &AppWindows,
    report: &mut GuiSmokeReport,
) -> Result<()> {
    let presenter = &windows.presenter;
    let was_expanded = presenter.get_audience_panel_expanded();
    if !was_expanded {
        presenter.invoke_audience_toggle_panel();
        settle_presenter_notes_layout(presenter).await?;
    }
    let size = presenter.window().size();
    for (width, height) in [
        (800.0, 842.0),
        (1024.0, 1002.0),
        (1280.0, 900.0),
        (1920.0, 1080.0),
        (2560.0, 1440.0),
        (1200.0, 1240.0),
    ] {
        presenter
            .window()
            .dispatch_event(slint::platform::WindowEvent::Resized {
                size: slint::LogicalSize::new(width, height),
            });
        settle_presenter_notes_layout(presenter).await?;
        let current_width = presenter.get_presenter_current_column_width();
        let next_width = presenter.get_presenter_next_column_width();
        let thumbnail_width = presenter.get_presenter_thumbnail_width();
        report.check(
            format!("Presenter panes stay balanced and notes remain usable ({width}x{height})"),
            current_width >= next_width
                && next_width >= 260.0
                && (128.0..=192.0).contains(&thumbnail_width)
                && (presenter.get_notes_layout_outer_width() - current_width).abs() <= 1.0
                && presenter.get_notes_visible_height() >= 180.0
                && presenter.get_presenter_next_preview_height() > 0.0
                && presenter.get_presenter_next_preview_height()
                    <= next_width / presenter.get_next_page_aspect_ratio() + 1.0
                && (next_width <= 260.0 || (current_width - next_width * 2.0).abs() <= 1.0),
            format!("current={current_width}, next={next_width}, thumbnails={thumbnail_width}"),
            format!(
                "unbalanced panes or unusable notes: {:?}",
                NotesResizeSnapshot::capture(presenter)
            ),
        );
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
                && (presenter.get_audience_reactions_width()
                    - presenter.get_audience_panel_width())
                .abs()
                    <= 1.0
                && presenter.get_audience_toolbar_ordered()
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
    settle_presenter_notes_layout(presenter).await?;
    if !was_expanded {
        presenter.invoke_audience_toggle_panel();
        settle_presenter_notes_layout(presenter).await?;
    }
    Ok(())
}

async fn check_audience_overlay(
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    url: &str,
    report: &mut GuiSmokeReport,
) -> Result<()> {
    let audience = &windows.audience;
    let presenter = &windows.presenter;
    let generation = state.borrow().render_generation;
    let first = audience.get_audience_overlay().row_data(0);
    let observed_at = Instant::now();
    let snapshot = audience.window().take_snapshot()?;
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
    let size = audience.window().size();
    audience
        .window()
        .set_size(slint::LogicalSize::new(800.0, 600.0));
    settle_presenter_notes_layout(presenter).await?;
    let moving = audience.get_audience_overlay().row_data(0);
    report.check(
        "Overlay animates within resized bounds and preserves the presentation session",
        first.as_ref().zip(moving.as_ref()).is_some_and(|(a, b)| {
            b.y < a.y && (0.0..=1.0).contains(&b.x) && (0.0..=1.0).contains(&b.y)
        }) && state.borrow().render_generation == generation,
        "motion stays normalized and render generation unchanged",
        format!("first={first:?}, after={moving:?}, elapsed={:?}, generation={generation}->{}, window={:?}", observed_at.elapsed(), state.borrow().render_generation, audience.window().size()),
    );
    audience.window().set_size(size);
    crate::handle_presentation_command(
        &windows.refs(),
        state,
        PresentationCommand::SetBlackScreen(true),
    );
    report.check(
        "Blackout immediately discards reaction overlays",
        audience.get_audience_overlay().row_count() == 0,
        "model cleared",
        "overlay remained queued",
    );
    submit_audience_smoke_reaction(url)?;
    settle_presenter_notes_layout(presenter).await?;
    let blank = audience.window().take_snapshot()?;
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
    settle_presenter_notes_layout(presenter).await?;
    report.check(
        "Restoring blackout never replays suppressed reactions",
        audience.get_audience_overlay().row_count() == 0,
        "no stale reactions",
        "old reaction replayed",
    );
    crate::window_controller::hide_audience_window(&windows.refs());
    submit_audience_smoke_reaction(url)?;
    settle_presenter_notes_layout(presenter).await?;
    crate::window_controller::show_audience_window(&windows.refs());
    settle_presenter_notes_layout(presenter).await?;
    report.check(
        "Showing the audience never replays hidden-window reactions",
        audience.get_audience_overlay().row_count() == 0,
        "hidden reactions discarded",
        "hidden reaction replayed",
    );
    submit_audience_smoke_reaction(url)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while audience.get_audience_overlay().row_count() == 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter).await?;
    }
    report.check(
        "New reactions resume after showing the audience",
        audience.get_audience_overlay().row_count() > 0,
        "new input displayed",
        "new input missing",
    );
    let deadline =
        Instant::now() + crate::audience_overlay::GUI_SMOKE_LIFETIME + Duration::from_secs(2);
    while audience.get_audience_overlay().row_count() > 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter).await?;
    }
    report.check(
        "Reaction overlay expires without further inputs",
        audience.get_audience_overlay().row_count() == 0,
        "animation expired",
        "animation retained an old reaction",
    );
    Ok(())
}

async fn check_live_comments(
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    url: &str,
    report: &mut GuiSmokeReport,
) -> Result<()> {
    let presenter = &windows.presenter;
    let audience = &windows.audience;
    let submit = |body: &str| {
        submit_audience_smoke_event(
            url,
            serde_json::json!({"protocol":"qp.audience","version":"1","type":"comment","request_id":"c","body":body,"handle":"Anonymous"}),
        )
    };
    submit("Hello 世界")?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while audience.get_audience_comments().row_count() == 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter).await?;
    }
    let first = audience.get_audience_comments().row_data(0);
    report.check(
        "Live comments measure the actual font and enter the logical canvas",
        first.as_ref().is_some_and(|f| {
            f.width > 100.0 && f.x <= 1280.0 && f.y == 48.0 && f.text == "Hello 世界"
        }) && presenter
            .get_audience_comment_history()
            .contains("Hello 世界"),
        "measured comment and independent history present",
        format!("comment={first:?}"),
    );
    let original_size = audience.window().size();
    audience
        .window()
        .set_size(slint::LogicalSize::new(800.0, 600.0));
    crate::audience_ui::set_join_visible(&windows.refs(), state, true);
    settle_presenter_notes_layout(presenter).await?;
    let next = audience.get_audience_comments().row_data(0);
    report.check(
        "Resize and page 0 preserve comment lane, width, and elapsed motion",
        first
            .as_ref()
            .zip(next.as_ref())
            .is_some_and(|(a, b)| b.x < a.x && a.y == b.y && a.width == b.width),
        "logical comment state preserved",
        format!("before={first:?}, after={next:?}"),
    );
    crate::audience_ui::set_join_visible(&windows.refs(), state, false);
    audience.window().set_size(original_size);
    presenter.invoke_audience_toggle_comments();
    report.check(
        "Pause immediately clears comments",
        presenter.get_audience_comments_paused()
            && audience.get_audience_comments().row_count() == 0,
        "comments cleared",
        "comments survived pause",
    );
    submit("Paused comment")?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while !presenter
        .get_audience_comment_history()
        .contains("Paused comment")
        && Instant::now() < deadline
    {
        settle_presenter_notes_layout(presenter).await?;
    }
    report.check(
        "Paused comments reach history without overlay",
        presenter
            .get_audience_comment_history()
            .contains("Paused comment")
            && audience.get_audience_comments().row_count() == 0,
        "history updated only",
        "pause lost history or displayed comment",
    );
    // Resume before the UI poll: this queued event must retain its old generation.
    submit("Old generation")?;
    presenter.invoke_audience_toggle_comments();
    settle_presenter_notes_layout(presenter).await?;
    settle_presenter_notes_layout(presenter).await?;
    report.check(
        "Resume rejects old-generation UI deliveries",
        !presenter.get_audience_comments_paused()
            && audience.get_audience_comments().row_count() == 0,
        "no replay",
        "old comment appeared after resume",
    );
    submit("After resume")?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while audience.get_audience_comments().row_count() == 0 && Instant::now() < deadline {
        settle_presenter_notes_layout(presenter).await?;
    }
    report.check(
        "New comments display after Resume",
        audience.get_audience_comments().row_count() == 1
            && audience
                .get_audience_comments()
                .row_data(0)
                .is_some_and(|f| f.text == "After resume"),
        "new comment displayed",
        "new comment missing or old comment replayed",
    );
    let history = presenter.get_audience_comment_history();
    presenter.set_audience_comment_history(
        (1..=20)
            .map(|n| format!("{n}: A long comment that wraps 日本語のコメント").repeat(4))
            .collect::<Vec<_>>()
            .join("\n")
            .into(),
    );
    settle_presenter_notes_layout(presenter).await?;
    report.check(
        "Twenty comment history entries remain scrollable below notes",
        presenter.get_audience_history_height() > presenter.get_audience_history_visible_height()
            && presenter.get_audience_history_visible_height() > 0.0,
        "bounded history overflows a usable scroll viewport",
        "history height or viewport missing",
    );
    presenter.set_audience_comment_history(history);
    presenter.invoke_audience_toggle_comments();
    presenter.invoke_audience_toggle_comments();
    Ok(())
}

// A minimal loopback client keeps GUI smoke independent of a new runtime dependency.
fn submit_audience_smoke_reaction(url: &str) -> Result<()> {
    submit_audience_smoke_event(
        url,
        serde_json::json!({"protocol":"qp.audience","version":"1","type":"reaction","request_id":"smoke","kind":"applause"}),
    )
}
fn submit_audience_smoke_event(url: &str, event: serde_json::Value) -> Result<()> {
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
    let code = url.rsplit('/').next().unwrap();
    write!(socket, "GET /ws/audience/{code} HTTP/1.1\r\nHost: {authority}\r\nOrigin: http://{authority}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n")?;
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
        anyhow::ensure!(payload.len() <= 4096, "smoke message too large");
        let mask = [1u8, 2, 3, 4];
        if payload.len() < 126 {
            socket.write_all(&[0x81, 0x80 | payload.len() as u8])?;
        } else {
            socket.write_all(&[0x81, 0x80 | 126])?;
            socket.write_all(&(payload.len() as u16).to_be_bytes())?;
        }
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
            header[0] == 0x81 && header[1] <= 126,
            "unexpected smoke server frame"
        );
        let length = if header[1] == 126 {
            let mut bytes = [0; 2];
            socket.read_exact(&mut bytes)?;
            u16::from_be_bytes(bytes) as usize
        } else {
            header[1] as usize
        };
        anyhow::ensure!(length <= 4096, "smoke server frame too large");
        let mut payload = vec![0; length];
        socket.read_exact(&mut payload)?;
        Ok(serde_json::from_slice(&payload)?)
    }
    send(
        &mut socket,
        serde_json::json!({"protocol":"qp.audience","version":"1","type":"hello","token":token}),
    )?;
    anyhow::ensure!(
        read(&mut socket)?["type"] == "welcome",
        "audience authentication failed"
    );
    send(&mut socket, event)?;
    anyhow::ensure!(
        read(&mut socket)?["status"] == "accepted",
        "smoke reaction rejected"
    );
    Ok(())
}

pub(crate) async fn delay(duration: Duration) {
    use std::task::Poll;
    let ready = Rc::new(Cell::new(false));
    let waker = Rc::new(RefCell::new(None::<std::task::Waker>));
    let completed = ready.clone();
    let wake = waker.clone();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::SingleShot, duration, move || {
        completed.set(true);
        if let Some(waker) = wake.borrow_mut().take() {
            waker.wake();
        }
    });
    std::future::poll_fn(|cx| {
        if ready.get() {
            Poll::Ready(())
        } else {
            *waker.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        }
    })
    .await;
}

async fn settle_notes_layout() -> Result<()> {
    delay(Duration::from_millis(50)).await;
    Ok(())
}

async fn settle_presenter_notes_layout(presenter: &crate::PresenterWindow) -> Result<()> {
    // Paragraph models are materialized by Slint's input/rendering passes, not
    // by elapsed time alone. PointerExited runs the normal input pass without
    // clicking controls or changing focus, even when native redraw is deferred.
    presenter
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerExited);
    settle_notes_layout().await
}

async fn check_settings_dialog(
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    report: &mut GuiSmokeReport,
) -> Result<()> {
    let presenter = &windows.presenter;
    let before = state.borrow().presentation.snapshot();
    presenter.invoke_open_settings();
    settle_notes_layout().await?;
    if let Some(path) = std::env::var_os("QP_GUI_SMOKE_SETTINGS_SNAPSHOT") {
        let snapshot = presenter.window().take_snapshot()?;
        let mut bytes =
            format!("P6\n{} {}\n255\n", snapshot.width(), snapshot.height()).into_bytes();
        for pixel in snapshot.as_bytes().chunks_exact(4) {
            bytes.extend_from_slice(&pixel[..3]);
        }
        fs::write(path, bytes).context("failed to save Settings smoke snapshot")?;
    }
    report.check(
        "Settings opens independently of Audience Live",
        presenter.get_settings_visible() && !presenter.get_audience_active(),
        "settings opened without starting an audience session",
        "settings did not open independently",
    );
    for text in [" ", "\n"] {
        presenter
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.into() });
        presenter
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: text.into() });
    }
    settle_notes_layout().await?;
    report.check(
        "Settings isolates presentation keyboard navigation",
        state.borrow().presentation.snapshot() == before,
        "Space/Return in Settings did not turn the page",
        "Settings forwarded a navigation key to the presentation",
    );
    presenter.set_audience_import_preview("https://relay.example/".into());
    presenter.invoke_close_settings();
    settle_notes_layout().await?;
    report.check(
        "Closing Settings clears pending import preview",
        !presenter.get_settings_visible() && presenter.get_audience_import_preview().is_empty(),
        "settings closed and preview was discarded",
        "settings or preview remained open",
    );
    Ok(())
}

#[derive(Debug, PartialEq)]
struct NotesResizeSnapshot {
    window_size: slint::PhysicalSize,
    has_notes: bool,
    font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
    content_height: f32,
    measured_heights: Vec<f32>,
    slide_width: f32,
    slide_height: f32,
    notes_outer_width: f32,
    menu_height: f32,
    main_y: f32,
}

impl NotesResizeSnapshot {
    fn capture(presenter: &crate::PresenterWindow) -> Self {
        Self {
            window_size: presenter.window().size(),
            has_notes: presenter.get_has_notes(),
            font_size: presenter.get_notes_font_size(),
            viewport_width: presenter.get_notes_visible_width(),
            viewport_height: presenter.get_notes_visible_height(),
            content_height: presenter.get_notes_content_height(),
            measured_heights: presenter.get_notes_measured_heights().iter().collect(),
            slide_width: presenter.get_notes_layout_slide_width(),
            slide_height: presenter.get_notes_layout_slide_height(),
            notes_outer_width: presenter.get_notes_layout_outer_width(),
            menu_height: presenter.get_notes_layout_menu_height(),
            main_y: presenter.get_notes_layout_main_y(),
        }
    }

    fn portrait_inline_layout_is_applied(&self) -> bool {
        // Read actual element bounds, not just the requested aspect/menu flags.
        // The 0.6 slide surface, 28px inline menu and displaced main area must
        // agree, and the notes viewport must contain that new slide width.
        // This is an applied-layout witness even if native size did not change;
        // neither an arbitrary quiet period nor a resize event proves it alone.
        self.slide_width > 0.0
            && self.slide_height > 0.0
            && (self.slide_width - self.slide_height * 0.6).abs() <= 1.0
            && (self.menu_height - 28.0).abs() <= 1.0
            && (self.main_y - self.menu_height - 54.0).abs() <= 1.0
            && self.notes_outer_width >= self.slide_width - 1.0
            && self.viewport_width <= self.notes_outer_width - 20.0 + 1.0
    }

    fn is_fitted(&self) -> bool {
        let expected = crate::notes::fit_notes_font_size(
            self.has_notes,
            self.viewport_width,
            self.viewport_height,
            &self.measured_heights,
        );
        if self.viewport_width <= 0.0
            || self.viewport_height <= 0.0
            || self.font_size != f32::from(expected)
        {
            return false;
        }
        if !self.has_notes {
            return true;
        }
        let index = usize::from(expected - crate::notes::MIN_NOTES_FONT_SIZE);
        self.measured_heights.get(index).is_some_and(|height| {
            let expected_content = self
                .viewport_height
                .max(*height + crate::notes::NOTES_BOTTOM_PADDING);
            (self.content_height - expected_content).abs() <= 1.0
        })
    }
}

async fn resize_presenter_notes(
    presenter: &crate::PresenterWindow,
    size: slint::LogicalSize,
) -> Result<()> {
    presenter.window().set_size(size);
    wait_for_presenter_notes_resize(presenter, Some(size)).await
}

async fn wait_for_presenter_notes_resize(
    presenter: &crate::PresenterWindow,
    requested: Option<slint::LogicalSize>,
) -> Result<()> {
    let started = Instant::now();
    let mut previous = None;
    let mut unchanged_since = started;
    let mut last_request = started;
    let mut retries = 0;
    settle_presenter_notes_layout(presenter).await?;
    loop {
        // Rendering callbacks can stop for occluded Wayland windows. Keep the
        // input/layout pass running while native configuration is in flight.
        presenter
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerExited);
        presenter.window().request_redraw();
        delay(Duration::from_millis(10)).await;
        let snapshot = NotesResizeSnapshot::capture(presenter);
        let now = Instant::now();
        if previous.as_ref() != Some(&snapshot) {
            unchanged_since = now;
        }
        let logical = snapshot
            .window_size
            .to_logical(presenter.window().scale_factor());
        let size_matches = requested.map_or_else(
            || snapshot.portrait_inline_layout_is_applied(),
            |size| {
                (logical.width - size.width).abs() <= 1.0
                    && (logical.height - size.height).abs() <= 1.0
            },
        );
        // A late Wayland configure can restore the previous native size after
        // set_size returned. Reissue only an unapplied geometry request; never
        // change notes properties or bypass the production fitting callback.
        if let Some(size) = requested {
            if !size_matches && now.duration_since(last_request) >= Duration::from_millis(100) {
                presenter.window().set_size(size);
                last_request = now;
                retries += 1;
                unchanged_since = now;
            }
        }
        if size_matches
            && snapshot.is_fitted()
            && now.duration_since(unchanged_since) >= Duration::from_millis(200)
        {
            if retries > 0 {
                eprintln!(
                    "GUI smoke: notes resize settled after {retries} geometry retries: requested={requested:?}, actual={logical:?}"
                );
            }
            return Ok(());
        }
        previous = Some(snapshot);
        if started.elapsed() >= Duration::from_secs(5) {
            bail!(
                "presenter notes did not settle after resize to {requested:?} ({retries} geometry retries): layout={:?}",
                NotesResizeSnapshot::capture(presenter)
            );
        }
    }
}

async fn check_notes_font_size(
    windows: &AppWindows,
    state: &Rc<RefCell<AppState>>,
    report: &mut GuiSmokeReport,
    context: &str,
) -> Result<()> {
    let presenter = &windows.presenter;
    let original_panel_expanded = presenter.get_audience_panel_expanded();
    // Keep typography fixtures within small CI displays. Expanding the footer
    // raises even the minimum fixture above AppKit's available work area.
    // Disclosure and expanded history are exercised separately below.
    if original_panel_expanded {
        presenter.invoke_audience_toggle_panel();
        settle_presenter_notes_layout(presenter).await?;
    }
    let original_size = presenter.window().size();
    let original_aspect = presenter.get_current_page_aspect_ratio();
    let original_has_notes = presenter.get_has_notes();
    let original_text = presenter.get_notes_text();
    let original_menu = presenter.get_use_native_menu_bar();
    let before = state.borrow().presentation.snapshot();
    let generation_before = state.borrow().render_generation;
    let now = Instant::now();
    let elapsed_before = state.borrow().timer.elapsed_at(now);
    resize_presenter_notes(
        presenter,
        slint::LogicalSize::new(800.0, 560.0 + presenter.get_audience_panel_height()),
    )
    .await?;

    presenter.set_has_notes(false);
    presenter.set_notes_text("".into());
    settle_presenter_notes_layout(presenter).await?;
    report.check(
        format!("no-notes placeholder uses 12px ({context})"),
        presenter.get_notes_font_size() == 12.0,
        "fixed placeholder size",
        "unexpected placeholder size",
    );

    // These fixtures isolate vertical font fitting even on small CI displays.
    presenter.set_has_notes(true);
    presenter.set_notes_text("Hi. 日本語。".into());
    settle_presenter_notes_layout(presenter).await?;
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
    settle_presenter_notes_layout(presenter).await?;
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
        settle_presenter_notes_layout(presenter).await?;
        let expanded_height = presenter.get_notes_content_height();
        let expanded_measurement = presenter.get_notes_measured_heights().row_data(0).unwrap();
        typography.set_line_height_factor(1.0);
        settle_presenter_notes_layout(presenter).await?;
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
        settle_presenter_notes_layout(presenter).await?;
        let lines = presenter.get_notes_measured_heights().row_data(0).unwrap();
        presenter.set_notes_text("English line\n\n日本語の行".into());
        settle_presenter_notes_layout(presenter).await?;
        let one_blank = presenter.get_notes_measured_heights().row_data(0).unwrap();
        presenter.set_notes_text("English line\n\n\n日本語の行".into());
        settle_presenter_notes_layout(presenter).await?;
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
        settle_presenter_notes_layout(presenter).await?;
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
    presenter.set_notes_text("語\n".repeat(8).into());
    settle_presenter_notes_layout(presenter).await?;
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
        format!("multi-line notes use the largest fitting measured size ({context})"),
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
    // Keep the width fixed so the slide preview does not consume the extra
    // vertical room. The collapsed footer makes this a 608 -> 640px resize,
    // which fits the 656px client work area on macOS CI.
    resize_presenter_notes(
        presenter,
        slint::LogicalSize::new(800.0, 592.0 + presenter.get_audience_panel_height()),
    )
    .await?;
    settle_presenter_notes_layout(presenter).await?;
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
    settle_presenter_notes_layout(presenter).await?;
    let constrained_height = presenter.get_notes_visible_height();
    let constrained_size = presenter.get_notes_font_size();
    let controlled_window = presenter.window().size();
    presenter.set_current_page_aspect_ratio(4.0);
    settle_presenter_notes_layout(presenter).await?;
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
    resize_presenter_notes(
        presenter,
        slint::LogicalSize::new(800.0, 560.0 + presenter.get_audience_panel_height()),
    )
    .await?;
    presenter.set_current_page_aspect_ratio(0.6);
    presenter.set_use_native_menu_bar(false);
    // The new layout can raise the native minimum size (notably with Windows
    // inline menus). Verify fitting and the usable viewport at the OS-selected
    // size; the Audience Live footer can also constrain the native geometry.
    presenter.window().set_size(slint::LogicalSize::new(
        800.0,
        560.0 + presenter.get_audience_panel_height(),
    ));
    wait_for_presenter_notes_resize(presenter, None).await?;
    report.check(
        format!("portrait slides and inline menus retain bounded notes sizing ({context})"),
        (12.0..=24.0).contains(&presenter.get_notes_font_size())
            && presenter.get_notes_visible_height() >= 180.0,
        format!(
            "native size={:?}, viewport={}x{}, font={}",
            presenter.window().size(),
            presenter.get_notes_visible_width(),
            presenter.get_notes_visible_height(),
            presenter.get_notes_font_size()
        ),
        format!(
            "portrait or inline menu layout exceeded the typography bounds: {:?}",
            NotesResizeSnapshot::capture(presenter)
        ),
    );

    presenter.set_has_notes(original_has_notes);
    presenter.set_notes_text(original_text);
    presenter.set_current_page_aspect_ratio(original_aspect);
    presenter.set_use_native_menu_bar(original_menu);
    // The initial size can be a pre-configure hint that the window manager
    // constrains, rather than an exact size the restored window must attain.
    presenter.window().set_size(original_size);
    settle_presenter_notes_layout(presenter).await?;
    report.check(
        format!("automatic sizing preserves presentation state ({context})"),
        state.borrow().presentation.snapshot() == before
            && state.borrow().render_generation == generation_before
            && state.borrow().timer.elapsed_at(now) == elapsed_before,
        "page, render generation, and timer stayed unchanged",
        "automatic sizing unexpectedly changed presentation state",
    );
    if original_panel_expanded {
        presenter.invoke_audience_toggle_panel();
        settle_presenter_notes_layout(presenter).await?;
    }
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

async fn wait_for_async_open(
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
        delay(ASYNC_OPEN_POLL_INTERVAL).await;
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

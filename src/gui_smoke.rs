use std::{
    cell::RefCell,
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
    crate::configure_shortcut_modifiers(&windows);
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState::default()));

    crate::wire_callbacks(&windows, windows.refs(), state.clone());
    crate::apply_app_metadata(&windows.presenter);

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

fn settle_notes_layout() -> Result<()> {
    slint::Timer::single_shot(Duration::from_millis(50), || {
        slint::quit_event_loop().expect("GUI smoke event loop should accept quit");
    });
    slint::run_event_loop_until_quit().context("failed to process notes layout changes")
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
            size: slint::LogicalSize::new(800.0, 560.0),
        });

    presenter.set_has_notes(false);
    presenter.set_notes_text("".into());
    settle_notes_layout()?;
    report.check(
        format!("no-notes placeholder uses 12px ({context})"),
        presenter.get_notes_font_size() == 12.0,
        "fixed placeholder size",
        "unexpected placeholder size",
    );

    presenter.set_has_notes(true);
    presenter.set_notes_text("Opening remarks. 日本語のノート。".into());
    settle_notes_layout()?;
    report.check(
        format!("short notes automatically use 24px ({context})"),
        presenter.get_notes_font_size() == 24.0
            && presenter.get_notes_content_height() <= presenter.get_notes_visible_height(),
        "short notes fit at the maximum size",
        format!(
            "size={}, content={}, viewport={}",
            presenter.get_notes_font_size(),
            presenter.get_notes_content_height(),
            presenter.get_notes_visible_height()
        ),
    );

    presenter.set_notes_text(
        "Long speaker notes with English and 日本語.\n"
            .repeat(150)
            .into(),
    );
    settle_notes_layout()?;
    report.check(
        format!("long notes stay readable and scrollable at 12px ({context})"),
        presenter.get_notes_font_size() == 12.0
            && presenter.get_notes_content_height() > presenter.get_notes_visible_height(),
        "overflow remains scrollable at the minimum size",
        "overflow shrank below the minimum or did not remain scrollable",
    );

    presenter.set_notes_scroll_y(
        presenter.get_notes_visible_height() - presenter.get_notes_content_height(),
    );
    presenter.set_notes_text("One line of notes.\n".repeat(10).into());
    settle_notes_layout()?;
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
    // Exercise layout resize events independently of WindowServer size restrictions.
    presenter
        .window()
        .dispatch_event(slint::platform::WindowEvent::Resized {
            size: slint::LogicalSize::new(1200.0, 1000.0),
        });
    settle_notes_layout()?;
    report.check(
        format!("logical resize events automatically enlarge notes ({context})"),
        presenter.get_notes_font_size() > small_size,
        "larger viewport enlarged the text",
        format!(
            "before={small_size}, after={}, viewport={}, window={:?}",
            presenter.get_notes_font_size(),
            presenter.get_notes_visible_height(),
            presenter.window().size()
        ),
    );
    presenter
        .window()
        .dispatch_event(slint::platform::WindowEvent::Resized {
            size: slint::LogicalSize::new(800.0, 560.0),
        });
    presenter.set_current_page_aspect_ratio(0.6);
    presenter.set_use_native_menu_bar(false);
    settle_notes_layout()?;
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
    settle_notes_layout()?;
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

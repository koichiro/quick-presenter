use std::{cell::RefCell, fs, path::Path, rc::Rc};

use anyhow::{bail, Context, Result};
use slint::ComponentHandle;

use crate::{
    app_state::AppState,
    cli::GuiSmokeOptions,
    input::PresentationCommand,
    render_controller::CURRENT_RENDER_WIDTH,
    rendering::{RenderPurpose, RenderRequest},
    window_controller::{set_slide_fullscreen, AppWindows},
};

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

    crate::open_and_render(&window_refs, &state, options.pdf_path.clone())
        .context("failed to open and render PDF in Slint windows")?;

    report.pass("opened and rendered PDF in Slint windows");
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

    run_command(
        &window_refs,
        &state,
        report,
        "next page command advances",
        PresentationCommand::NextPage,
        |state| current_page_index(state) == Some(1),
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

fn current_page_index(state: &AppState) -> Option<u32> {
    state
        .presentation
        .snapshot()
        .map(|snapshot| snapshot.current_index)
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
}

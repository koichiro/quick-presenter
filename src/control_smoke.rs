//! Exercise IPC against real GUI-owned state without keyboard or mouse automation.
use crate::{app_state::AppState, control_app::ControlRuntime, window_controller::AppWindows};
use anyhow::{Context, Result};
use quick_presenter::control::{client, protocol::*};
use std::{
    cell::RefCell,
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub fn run(windows: &AppWindows, state: Rc<RefCell<AppState>>, pdf: PathBuf) -> Result<()> {
    use slint::ComponentHandle;
    let directory = PathBuf::from(format!("/tmp/qp-control-smoke-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&directory)?;
    let path = directory.join("control.sock");
    let runtime = ControlRuntime::install_at(windows.refs(), state.clone(), &path)?;
    let _render_timer = crate::start_render_event_updates(windows.refs(), state.clone());
    // A hidden presenter cannot receive keyboard navigation.
    windows.presenter.hide()?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let _ = sender.send(check_commands(&path, &pdf).map_err(|error| format!("{error:#}")));
    });
    let result = Rc::new(RefCell::new(None));
    let result_timer = result.clone();
    let deadline = Instant::now() + Duration::from_secs(20);
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(10),
        move || {
            if let Ok(outcome) = receiver.try_recv() {
                *result_timer.borrow_mut() = Some(outcome);
                let _ = slint::quit_event_loop();
            } else if Instant::now() >= deadline {
                *result_timer.borrow_mut() = Some(Err("Control GUI smoke timed out".into()));
                let _ = slint::quit_event_loop();
            }
        },
    );
    let loop_result = slint::run_event_loop_until_quit();
    timer.stop();
    drop(runtime);
    let _ = worker.join();
    windows.presenter.show()?;
    fs::remove_dir_all(directory)?;
    loop_result.context("control GUI event loop")?;
    result
        .borrow_mut()
        .take()
        .unwrap_or_else(|| Err("Control GUI smoke interrupted".into()))
        .map_err(anyhow::Error::msg)?;
    Ok(())
}
fn call(path: &Path, command: Command) -> Result<Reply> {
    let response = client::send_to(path, &Request::new(1, command))
        .map_err(|e| anyhow::anyhow!("{:?}: {}", e.code, e.message))?;
    match response.outcome {
        Outcome::Result(reply) => Ok(reply),
        Outcome::Error(error) => anyhow::bail!("{:?}: {}", error.code, error.message),
    }
}
fn expect_error(path: &Path, command: Command, code: ErrorCode) -> Result<()> {
    let response =
        client::send_to(path, &Request::new(1, command)).map_err(|e| anyhow::anyhow!(e.message))?;
    anyhow::ensure!(
        matches!(response.outcome, Outcome::Error(error) if error.code == code),
        "unexpected error category"
    );
    Ok(())
}
fn ready(path: &Path) -> Result<()> {
    for _ in 0..200 {
        if matches!(call(path, Command::Status(Empty {}))?, Reply::Status(state) if state.render_state == RenderState::Ready && state.notes_state == NotesState::Ready)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    anyhow::bail!("presentation content did not become ready")
}
fn check_commands(path: &Path, pdf: &Path) -> Result<()> {
    let file = if pdf.is_absolute() {
        pdf.to_owned()
    } else {
        std::env::current_dir()?.join(pdf)
    };
    let file = file.to_str().context("UTF-8 smoke PDF path")?.to_owned();
    call(path, Command::GoTo(PageParams { page: 1 }))?;
    ready(path)?;
    anyhow::ensure!(
        matches!(call(path, Command::Slide(ContentParams::default()))?, Reply::Slide { content, .. } if content.page == 1 && !content.text.is_empty()),
        "slide text missing"
    );
    let total = match call(path, Command::Context(ContentParams::default()))? {
        Reply::Context {
            presentation,
            current,
            next,
        } => {
            anyhow::ensure!(
                current.slide.page == 1 && next.as_ref().is_some_and(|s| s.slide.page == 2),
                "context page mismatch"
            );
            presentation.pages
        }
        _ => anyhow::bail!("missing context"),
    };
    anyhow::ensure!(
        matches!(call(path, Command::Slide(ContentParams { full: true }))?, Reply::Slide { content, .. } if content.page == 1 && !content.truncated),
        "full slide unavailable"
    );
    anyhow::ensure!(
        matches!(call(path, Command::Context(ContentParams { full: true }))?, Reply::Context { current, next: Some(next), .. } if !current.slide.truncated && !next.slide.truncated),
        "full context unavailable"
    );
    call(path, Command::GoTo(PageParams { page: total }))?;
    anyhow::ensure!(
        matches!(call(path, Command::Context(ContentParams::default()))?, Reply::Context { current, next: None, .. } if current.slide.page == total),
        "end-of-deck context mismatch"
    );
    call(path, Command::GoTo(PageParams { page: 1 }))?;
    anyhow::ensure!(
        matches!(
            call(path, Command::Notes(Empty {}))?,
            Reply::Notes { page: 1, .. }
        ),
        "notes page mismatch"
    );
    anyhow::ensure!(
        matches!(call(path, Command::Next(Empty {}))?, Reply::Mutation { state, .. } if state.page == Some(2)),
        "next failed"
    );
    anyhow::ensure!(
        matches!(call(path, Command::Previous(Empty {}))?, Reply::Mutation { state, .. } if state.page == Some(1)),
        "previous failed"
    );
    expect_error(
        path,
        Command::GoTo(PageParams { page: 0 }),
        ErrorCode::InvalidPage,
    )?;
    for (value, changed) in [(true, true), (true, false), (false, true), (false, false)] {
        anyhow::ensure!(
            matches!(call(path, Command::Blackout(BlackoutParams { value }))?, Reply::Mutation { changed: actual, state } if actual == changed && state.blackout == value),
            "blackout was not deterministic"
        );
    }
    expect_error(
        path,
        Command::Open(OpenParams {
            file: format!("{file}.missing"),
        }),
        ErrorCode::OpenFailed,
    )?;
    anyhow::ensure!(
        matches!(call(path, Command::Status(Empty {}))?, Reply::Status(state) if state.page == Some(1)),
        "failed open lost the active PDF"
    );
    call(path, Command::Close(Empty {}))?;
    anyhow::ensure!(
        matches!(
            call(path, Command::Close(Empty {}))?,
            Reply::Mutation { changed: false, .. }
        ),
        "close was not idempotent"
    );
    anyhow::ensure!(
        matches!(call(path, Command::Status(Empty {}))?, Reply::Status(state) if state.page.is_none() && state.document.is_none()),
        "close did not clear the PDF"
    );
    expect_error(path, Command::Notes(Empty {}), ErrorCode::NoPresentation)?;
    expect_error(
        path,
        Command::Slide(ContentParams::default()),
        ErrorCode::NoPresentation,
    )?;
    expect_error(
        path,
        Command::Context(ContentParams::default()),
        ErrorCode::NoPresentation,
    )?;
    // Worker shutdown is asynchronous. Retry only the explicitly rejected BUSY open.
    for _ in 0..100 {
        let response = client::send_to(
            path,
            &Request::new(1, Command::Open(OpenParams { file: file.clone() })),
        )
        .map_err(|e| anyhow::anyhow!(e.message))?;
        match response.outcome {
            Outcome::Result(_) => return ready(path),
            Outcome::Error(e) if e.code == ErrorCode::Busy => {
                thread::sleep(Duration::from_millis(25))
            }
            Outcome::Error(e) => anyhow::bail!("reopen failed: {:?}: {}", e.code, e.message),
        }
    }
    anyhow::bail!("renderer did not finish shutdown")
}

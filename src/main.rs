pub mod aspect;
pub mod clock;
pub mod errors;
pub mod fullscreen;
pub mod input;
pub mod notes;
pub mod pdf;
pub mod presentation;
pub mod timer;

use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::{bail, Result};
use aspect::fitted_logical_size_within;
use clock::current_clock_label;
use errors::{presenter_error_message, speaker_notes_warning, PresenterMessage};
use fullscreen::FullscreenState;
use input::{apply_presentation_command, PresentationCommand};
use notes::SpeakerNotes;
use pdf::PdfDocumentState;
use presentation::{PageSnapshot, PresentationState};
use slint::{ComponentHandle, LogicalPosition, LogicalSize, Timer, TimerMode, Weak};
use timer::{is_first_page_advance, PresentationTimer};
use tracing::{error, warn};
use tracing_subscriber::EnvFilter;

slint::include_modules!();

const CURRENT_RENDER_WIDTH: i32 = 1600;
const PREVIEW_RENDER_WIDTH: i32 = 600;
const PRESENTER_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(80.0, 80.0);
const SLIDE_WINDOW_POSITION: LogicalPosition = LogicalPosition::new(180.0, 140.0);
const SLIDE_WINDOW_MAX_WIDTH: f32 = 1024.0;
const SLIDE_WINDOW_MAX_HEIGHT: f32 = 720.0;
const PRESENTER_TIME_UPDATE_INTERVAL: Duration = Duration::from_millis(250);

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let windows = AppWindows::new()?;
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState::default()));

    wire_callbacks(&windows, windows.refs(), state.clone());
    let _presenter_time_timer = start_presenter_time_updates(windows.refs(), state.clone());

    windows.apply_initial_positions();
    windows.slide.show()?;
    windows.presenter.show()?;
    slint::run_event_loop()?;
    Ok(())
}

struct AppWindows {
    presenter: PresenterWindow,
    slide: SlideWindow,
}

impl AppWindows {
    fn new() -> Result<Self> {
        Ok(Self {
            presenter: PresenterWindow::new()?,
            slide: SlideWindow::new()?,
        })
    }

    fn refs(&self) -> AppWindowRefs {
        AppWindowRefs {
            presenter: self.presenter.as_weak(),
            slide: self.slide.as_weak(),
        }
    }

    fn apply_initial_positions(&self) {
        self.presenter
            .window()
            .set_position(PRESENTER_WINDOW_POSITION);
        self.slide.window().set_position(SLIDE_WINDOW_POSITION);
    }
}

#[derive(Clone)]
struct AppWindowRefs {
    presenter: Weak<PresenterWindow>,
    slide: Weak<SlideWindow>,
}

#[derive(Default)]
struct AppState {
    fullscreen: FullscreenState,
    pdf: Option<PdfDocumentState>,
    notes: SpeakerNotes,
    presentation: PresentationState,
    timer: PresentationTimer,
    status_text: String,
}

struct RenderedPages {
    current: slint::Image,
    current_aspect_ratio: f32,
    next: Option<slint::Image>,
    next_aspect_ratio: Option<f32>,
}

fn wire_callbacks(windows: &AppWindows, refs: AppWindowRefs, state: Rc<RefCell<AppState>>) {
    let app = &windows.presenter;

    let window_refs = refs.clone();
    let state_for_open = state.clone();
    app.on_open_pdf(move || {
        if let Some(path) = pick_pdf_file() {
            if let Err(err) = open_and_render(&window_refs, &state_for_open, path) {
                error!(error = ?err, "failed to open and render PDF");
                set_presenter_message(&window_refs.presenter, presenter_error_message(&err));
            }
        }
    });

    let window_refs = refs.clone();
    let state_for_previous = state.clone();
    app.on_previous_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_previous,
            PresentationCommand::PreviousPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_next = state.clone();
    app.on_next_page(move || {
        handle_presentation_command(&window_refs, &state_for_next, PresentationCommand::NextPage);
    });

    let window_refs = refs.clone();
    let state_for_keyboard_previous = state.clone();
    app.on_keyboard_previous_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_keyboard_previous,
            PresentationCommand::PreviousPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_keyboard_next = state.clone();
    app.on_keyboard_next_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_keyboard_next,
            PresentationCommand::NextPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_toggle = state.clone();
    app.on_toggle_slide_fullscreen(move || {
        let mut state = state_for_toggle.borrow_mut();
        let fullscreen = state.fullscreen.toggle_slide_fullscreen();
        set_slide_fullscreen(&window_refs, fullscreen);
    });

    let window_refs = refs.clone();
    let state_for_slide_previous = state.clone();
    windows.slide.on_keyboard_previous_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_slide_previous,
            PresentationCommand::PreviousPage,
        );
    });

    let window_refs = refs.clone();
    let state_for_slide_next = state.clone();
    windows.slide.on_keyboard_next_page(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_slide_next,
            PresentationCommand::NextPage,
        );
    });

    let window_refs = refs;
    let state_for_slide_exit = state;
    windows.slide.on_exit_fullscreen(move || {
        handle_presentation_command(
            &window_refs,
            &state_for_slide_exit,
            PresentationCommand::ExitSlideFullscreen,
        );
    });
}

fn handle_presentation_command(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    command: PresentationCommand,
) {
    if command == PresentationCommand::ExitSlideFullscreen {
        exit_slide_fullscreen(windows, state);
        return;
    }

    let mut state = state.borrow_mut();
    let before = state.presentation.snapshot();
    apply_presentation_command(&mut state.presentation, command);
    let after = state.presentation.snapshot();
    maybe_start_elapsed_timer(command, before.as_ref(), after.as_ref(), &mut state.timer);
    if let Some(snapshot) = after {
        if let Err(err) = render_into_windows(windows, &state, &snapshot) {
            error!(error = ?err, "failed to render presentation page");
            set_presenter_message(&windows.presenter, presenter_error_message(&err));
        }
    }
}

fn maybe_start_elapsed_timer(
    command: PresentationCommand,
    before: Option<&PageSnapshot>,
    after: Option<&PageSnapshot>,
    timer: &mut PresentationTimer,
) {
    if command == PresentationCommand::NextPage
        && !timer.is_running()
        && is_first_page_advance(
            before.map(|snapshot| snapshot.current_index),
            after.map(|snapshot| snapshot.current_index),
        )
    {
        timer.start(Instant::now());
    }
}

fn exit_slide_fullscreen(windows: &AppWindowRefs, state: &Rc<RefCell<AppState>>) {
    let mut state = state.borrow_mut();
    let fullscreen = state.fullscreen.exit_slide_fullscreen();
    set_slide_fullscreen(windows, fullscreen);
}

fn set_slide_fullscreen(windows: &AppWindowRefs, fullscreen: bool) {
    if let Some(slide) = windows.slide.upgrade() {
        slide.window().set_fullscreen(fullscreen);
    }

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_slide_fullscreen(fullscreen);
    }
}

fn pick_pdf_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_title("Open PDF")
        .pick_file()
}

fn open_and_render(
    windows: &AppWindowRefs,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
) -> Result<()> {
    let doc = PdfDocumentState::open(path)?;
    let (notes, status_text) = match doc.speaker_notes() {
        Ok(notes) => (notes, "Ready".to_owned()),
        Err(err) => {
            warn!(error = ?err, "speaker notes unavailable");
            (
                SpeakerNotes::empty(),
                speaker_notes_warning(&err).text().to_owned(),
            )
        }
    };
    let presentation = PresentationState::open_document(doc.title(), doc.page_count());
    let snapshot = presentation.snapshot();
    let initial_slide_aspect_ratio = snapshot
        .as_ref()
        .map(|snapshot| doc.page_aspect_ratio(snapshot.current_index))
        .transpose()?;

    {
        let mut state = state.borrow_mut();
        state.pdf = Some(doc);
        state.notes = notes;
        state.presentation = presentation;
        state.timer.reset();
        state.status_text = status_text;
    }

    if let Some(aspect_ratio) = initial_slide_aspect_ratio {
        fit_slide_window_to_aspect_ratio(windows, aspect_ratio);
    }

    if let Some(snapshot) = snapshot {
        render_into_windows(windows, &state.borrow(), &snapshot)?;
    }

    Ok(())
}

fn fit_slide_window_to_aspect_ratio(windows: &AppWindowRefs, aspect_ratio: f32) {
    if let Some(slide) = windows.slide.upgrade() {
        let size = fitted_logical_size_within(
            SLIDE_WINDOW_MAX_WIDTH,
            SLIDE_WINDOW_MAX_HEIGHT,
            aspect_ratio,
        );
        let width = size.width.round();
        let height = size.height.round();
        slide.set_slide_window_width(width);
        slide.set_slide_window_height(height);
        slide.window().set_size(LogicalSize::new(width, height));
    }
}

fn render_into_windows(
    windows: &AppWindowRefs,
    state: &AppState,
    snapshot: &PageSnapshot,
) -> Result<()> {
    let Some(doc) = state.pdf.as_ref() else {
        bail!("missing open PDF for current presentation");
    };
    let rendered = render_pages(doc, snapshot)?;

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_current_page_image(rendered.current.clone());
        presenter.set_current_page_aspect_ratio(rendered.current_aspect_ratio);
        presenter.set_has_next_page(rendered.next.is_some());
        if let Some(next) = rendered.next.as_ref() {
            presenter.set_next_page_image(next.clone());
        }
        if let Some(next_aspect_ratio) = rendered.next_aspect_ratio {
            presenter.set_next_page_aspect_ratio(next_aspect_ratio);
        }
        presenter.set_document_title(snapshot.title.clone().into());
        presenter.set_page_label(snapshot.page_label.clone().into());
        presenter.set_clock_time_label(current_clock_label().into());
        presenter.set_elapsed_time_label(state.timer.elapsed_label_at(Instant::now()).into());
        presenter.set_status_text(state.status_text.clone().into());

        let current_note = state.notes.note_for_page_index(snapshot.current_index);
        presenter.set_has_notes(current_note.is_some());
        presenter.set_notes_text(current_note.unwrap_or_default().into());
    }

    if let Some(slide) = windows.slide.upgrade() {
        slide.set_page_aspect_ratio(rendered.current_aspect_ratio);
        slide.set_page_image(rendered.current);
    }

    Ok(())
}

fn render_pages(doc: &PdfDocumentState, snapshot: &PageSnapshot) -> Result<RenderedPages> {
    Ok(RenderedPages {
        current: doc.render_page(snapshot.current_index, CURRENT_RENDER_WIDTH)?,
        current_aspect_ratio: doc.page_aspect_ratio(snapshot.current_index)?,
        next: snapshot
            .next_index
            .map(|index| doc.render_page(index, PREVIEW_RENDER_WIDTH))
            .transpose()?,
        next_aspect_ratio: snapshot
            .next_index
            .map(|index| doc.page_aspect_ratio(index))
            .transpose()?,
    })
}

fn set_presenter_message(weak: &Weak<PresenterWindow>, message: PresenterMessage) {
    if let Some(app) = weak.upgrade() {
        app.set_status_text(message.text().into());
    }
}

fn start_presenter_time_updates(windows: AppWindowRefs, state: Rc<RefCell<AppState>>) -> Timer {
    let timer = Timer::default();
    timer.start(
        TimerMode::Repeated,
        PRESENTER_TIME_UPDATE_INTERVAL,
        move || update_presenter_time_labels(&windows.presenter, &state.borrow().timer),
    );
    timer
}

fn update_presenter_time_labels(presenter: &Weak<PresenterWindow>, timer: &PresentationTimer) {
    if let Some(presenter) = presenter.upgrade() {
        presenter.set_clock_time_label(current_clock_label().into());
        presenter.set_elapsed_time_label(timer.elapsed_label_at(Instant::now()).into());
    }
}

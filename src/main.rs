pub mod notes;
pub mod pdf;
pub mod presentation;

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use anyhow::Result;
use notes::SpeakerNotes;
use pdf::PdfDocumentState;
use presentation::{PageSnapshot, PresentationState};
use slint::Weak;
use tracing_subscriber::EnvFilter;

slint::include_modules!();

const CURRENT_RENDER_WIDTH: i32 = 1600;
const PREVIEW_RENDER_WIDTH: i32 = 600;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let windows = AppWindows::new()?;
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState::default()));

    wire_callbacks(&windows.presenter, windows.refs(), state.clone());

    windows.slide.show()?;
    windows.presenter.run()?;
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
}

#[derive(Clone)]
struct AppWindowRefs {
    presenter: Weak<PresenterWindow>,
    slide: Weak<SlideWindow>,
}

#[derive(Default)]
struct AppState {
    pdf: Option<PdfDocumentState>,
    notes: SpeakerNotes,
    presentation: PresentationState,
    status_text: String,
}

struct RenderedPages {
    current: slint::Image,
    next: Option<slint::Image>,
}

fn wire_callbacks(app: &PresenterWindow, windows: AppWindowRefs, state: Rc<RefCell<AppState>>) {
    let window_refs = windows.clone();
    let state_for_open = state.clone();
    app.on_open_pdf(move || {
        if let Some(path) = pick_pdf_file() {
            if let Err(err) = open_and_render(&window_refs, &state_for_open, path) {
                set_status(&window_refs.presenter, format!("Error: {err:#}"));
            }
        }
    });

    let window_refs = windows.clone();
    let state_for_previous = state.clone();
    app.on_previous_page(move || {
        let mut state = state_for_previous.borrow_mut();
        state.presentation.previous_page();
        if let Some(snapshot) = state.presentation.snapshot() {
            if let Err(err) = render_into_windows(&window_refs, &state, &snapshot) {
                set_status(&window_refs.presenter, format!("Error: {err:#}"));
            }
        }
    });

    let window_refs = windows;
    app.on_next_page(move || {
        let mut state = state.borrow_mut();
        state.presentation.next_page();
        if let Some(snapshot) = state.presentation.snapshot() {
            if let Err(err) = render_into_windows(&window_refs, &state, &snapshot) {
                set_status(&window_refs.presenter, format!("Error: {err:#}"));
            }
        }
    });
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
        Err(err) => (
            SpeakerNotes::empty(),
            format!("Ready (speaker notes unavailable: {err:#})"),
        ),
    };
    let presentation = PresentationState::open_document(doc.title(), doc.page_count());
    let snapshot = presentation.snapshot();

    {
        let mut state = state.borrow_mut();
        state.pdf = Some(doc);
        state.notes = notes;
        state.presentation = presentation;
        state.status_text = status_text;
    }

    if let Some(snapshot) = snapshot {
        render_into_windows(windows, &state.borrow(), &snapshot)?;
    }

    Ok(())
}

fn render_into_windows(
    windows: &AppWindowRefs,
    state: &AppState,
    snapshot: &PageSnapshot,
) -> Result<()> {
    let doc = state
        .pdf
        .as_ref()
        .expect("presentation snapshot should have an open PDF");
    let rendered = render_pages(doc, snapshot)?;

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_current_page_image(rendered.current.clone());
        presenter.set_has_next_page(rendered.next.is_some());
        if let Some(next) = rendered.next.as_ref() {
            presenter.set_next_page_image(next.clone());
        }
        presenter.set_document_title(snapshot.title.clone().into());
        presenter.set_page_label(snapshot.page_label.clone().into());
        presenter.set_status_text(state.status_text.clone().into());

        let current_note = state.notes.note_for_page_index(snapshot.current_index);
        presenter.set_has_notes(current_note.is_some());
        presenter.set_notes_text(current_note.unwrap_or_default().into());
    }

    if let Some(slide) = windows.slide.upgrade() {
        slide.set_page_image(rendered.current);
    }

    Ok(())
}

fn render_pages(doc: &PdfDocumentState, snapshot: &PageSnapshot) -> Result<RenderedPages> {
    Ok(RenderedPages {
        current: doc.render_page(snapshot.current_index, CURRENT_RENDER_WIDTH)?,
        next: match snapshot.next_index {
            Some(index) => Some(doc.render_page(index, PREVIEW_RENDER_WIDTH)?),
            None => None,
        },
    })
}

fn set_status(weak: &Weak<PresenterWindow>, message: String) {
    if let Some(app) = weak.upgrade() {
        app.set_status_text(message.into());
    }
}

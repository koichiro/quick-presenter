mod pdf;
pub mod presentation;

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use anyhow::Result;
use pdf::PdfDocumentState;
use presentation::{PageSnapshot, PresentationState};
use slint::Weak;
use tracing_subscriber::EnvFilter;

slint::include_modules!();

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let app = PresenterWindow::new()?;
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState::default()));

    wire_callbacks(&app, state.clone());

    app.run()?;
    Ok(())
}

#[derive(Default)]
struct AppState {
    pdf: Option<PdfDocumentState>,
    presentation: PresentationState,
}

fn wire_callbacks(app: &PresenterWindow, state: Rc<RefCell<AppState>>) {
    let weak = app.as_weak();
    let state_for_open = state.clone();
    app.on_open_pdf(move || {
        if let Some(path) = pick_pdf_file() {
            if let Err(err) = open_and_render(&weak, &state_for_open, path) {
                set_status(&weak, format!("Error: {err:#}"));
            }
        }
    });

    let weak = app.as_weak();
    let state_for_previous = state.clone();
    app.on_previous_page(move || {
        let mut state = state_for_previous.borrow_mut();
        state.presentation.previous_page();
        if let Some(snapshot) = state.presentation.snapshot() {
            if let Err(err) = render_into_app(&weak, &state, &snapshot) {
                set_status(&weak, format!("Error: {err:#}"));
            }
        }
    });

    let weak = app.as_weak();
    app.on_next_page(move || {
        let mut state = state.borrow_mut();
        state.presentation.next_page();
        if let Some(snapshot) = state.presentation.snapshot() {
            if let Err(err) = render_into_app(&weak, &state, &snapshot) {
                set_status(&weak, format!("Error: {err:#}"));
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
    weak: &Weak<PresenterWindow>,
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
) -> Result<()> {
    let doc = PdfDocumentState::open(path)?;
    let presentation = PresentationState::open_document(doc.title(), doc.page_count());
    let snapshot = presentation.snapshot();

    {
        let mut state = state.borrow_mut();
        state.pdf = Some(doc);
        state.presentation = presentation;
    }

    if let Some(snapshot) = snapshot {
        render_into_app(weak, &state.borrow(), &snapshot)?;
    }

    Ok(())
}

fn render_into_app(
    weak: &Weak<PresenterWindow>,
    state: &AppState,
    snapshot: &PageSnapshot,
) -> Result<()> {
    let doc = state
        .pdf
        .as_ref()
        .expect("presentation snapshot should have an open PDF");
    let image = doc.render_page(snapshot.current_index, 1600)?;
    let app = weak.upgrade().expect("window should still be alive");
    app.set_current_page_image(image);
    app.set_document_title(snapshot.title.clone().into());
    app.set_page_label(snapshot.page_label.clone().into());
    app.set_status_text("Ready".into());
    Ok(())
}

fn set_status(weak: &Weak<PresenterWindow>, message: String) {
    if let Some(app) = weak.upgrade() {
        app.set_status_text(message.into());
    }
}

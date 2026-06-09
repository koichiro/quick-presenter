mod pdf;
pub mod presentation;

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use anyhow::Result;
use pdf::PdfDocumentState;
use slint::Weak;
use tracing_subscriber::EnvFilter;

slint::include_modules!();

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let app = AppWindow::new()?;
    let state: Rc<RefCell<Option<PdfDocumentState>>> = Rc::new(RefCell::new(None));

    wire_callbacks(&app, state.clone());

    app.run()?;
    Ok(())
}

fn wire_callbacks(app: &AppWindow, state: Rc<RefCell<Option<PdfDocumentState>>>) {
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
        if let Some(ref mut doc) = *state_for_previous.borrow_mut() {
            doc.previous_page();
            if let Err(err) = render_into_app(&weak, doc) {
                set_status(&weak, format!("Error: {err:#}"));
            }
        }
    });

    let weak = app.as_weak();
    app.on_next_page(move || {
        if let Some(ref mut doc) = *state.borrow_mut() {
            doc.next_page();
            if let Err(err) = render_into_app(&weak, doc) {
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
    weak: &Weak<AppWindow>,
    state: &Rc<RefCell<Option<PdfDocumentState>>>,
    path: PathBuf,
) -> Result<()> {
    let doc = PdfDocumentState::open(path)?;
    *state.borrow_mut() = Some(doc);
    if let Some(ref doc) = *state.borrow() {
        render_into_app(weak, doc)?;
    }
    Ok(())
}

fn render_into_app(weak: &Weak<AppWindow>, doc: &PdfDocumentState) -> Result<()> {
    let image = doc.render_current_page(1600)?;
    let app = weak.upgrade().expect("window should still be alive");
    app.set_page_image(image);
    app.set_document_title(doc.title().into());
    app.set_page_label(doc.page_label().into());
    app.set_status_text("Ready".into());
    Ok(())
}

fn set_status(weak: &Weak<AppWindow>, message: String) {
    if let Some(app) = weak.upgrade() {
        app.set_status_text(message.into());
    }
}

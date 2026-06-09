use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use pdfium_render::prelude::*;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

pub struct PdfDocumentState {
    _pdfium: Pdfium,
    document: PdfDocument<'static>,
    path: PathBuf,
    page_index: PdfPageIndex,
}

impl PdfDocumentState {
    pub fn open(path: PathBuf) -> Result<Self> {
        let pdfium = create_pdfium()?;
        let document = pdfium
            .load_pdf_from_file(&path, None)
            .with_context(|| format!("failed to open PDF: {}", path.display()))?;
        let document =
            unsafe { std::mem::transmute::<PdfDocument<'_>, PdfDocument<'static>>(document) };

        Ok(Self {
            _pdfium: pdfium,
            document,
            path,
            page_index: 0,
        })
    }

    pub fn previous_page(&mut self) {
        self.page_index = self.page_index.saturating_sub(1);
    }

    pub fn next_page(&mut self) {
        let page_count = self.page_count();
        if page_count > 0 && self.page_index < page_count - 1 {
            self.page_index += 1;
        }
    }

    pub fn render_current_page(&self, target_width: i32) -> Result<Image> {
        let page = self
            .document
            .pages()
            .get(self.page_index)
            .with_context(|| format!("failed to load page {}", self.page_index + 1))?;
        let bitmap = page
            .render_with_config(&PdfRenderConfig::new().set_target_width(target_width))
            .with_context(|| format!("failed to render page {}", self.page_index + 1))?;
        let image = bitmap.as_image()?;
        let rgba = image.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let buffer =
            SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba.as_raw(), width, height);

        Ok(Image::from_rgba8(buffer))
    }

    pub fn title(&self) -> String {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled PDF")
            .to_owned()
    }

    pub fn page_label(&self) -> String {
        format!("{} / {}", self.page_index + 1, self.page_count())
    }

    fn page_count(&self) -> PdfPageIndex {
        self.document.pages().len()
    }
}

fn create_pdfium() -> Result<Pdfium> {
    if let Ok(path) = std::env::var("PDFIUM_DYNAMIC_LIB_PATH") {
        return new_or_reuse(Pdfium::bind_to_library(path))
            .context("failed to bind PDFium from PDFIUM_DYNAMIC_LIB_PATH");
    }

    let local_library = Pdfium::pdfium_platform_library_name_at_path(Path::new("pdfium/lib"));
    if local_library.exists() {
        return new_or_reuse(Pdfium::bind_to_library(&local_library))
            .with_context(|| format!("failed to bind local PDFium: {}", local_library.display()));
    }

    new_or_reuse(Pdfium::bind_to_system_library()).context("failed to bind system PDFium")
}

fn new_or_reuse(bindings: Result<Box<dyn PdfiumLibraryBindings>, PdfiumError>) -> Result<Pdfium> {
    match bindings {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Ok(Pdfium::default()),
        Err(err) => Err(err.into()),
    }
}

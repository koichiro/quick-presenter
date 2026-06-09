use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use pdfium_render::prelude::*;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

use crate::aspect::sanitize_aspect_ratio;
use crate::notes::{is_pdf_speaker_note_annotation, SpeakerNotes};

pub struct PdfDocumentState {
    _pdfium: Pdfium,
    document: PdfDocument<'static>,
    path: PathBuf,
    page_count: u32,
}

impl PdfDocumentState {
    pub fn open(path: PathBuf) -> Result<Self> {
        let pdfium = create_pdfium()?;
        let document = pdfium
            .load_pdf_from_file(&path, None)
            .with_context(|| format!("failed to open PDF: {}", path.display()))?;
        let document =
            unsafe { std::mem::transmute::<PdfDocument<'_>, PdfDocument<'static>>(document) };

        let page_count = document.pages().len();

        Ok(Self {
            _pdfium: pdfium,
            document,
            path,
            page_count: page_count.max(0) as u32,
        })
    }

    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    pub fn render_page(&self, page_index: u32, target_width: i32) -> Result<Image> {
        let page_number = page_index + 1;
        let page = self
            .document
            .pages()
            .get(page_index as PdfPageIndex)
            .with_context(|| format!("failed to load page {page_number}"))?;
        let bitmap = page
            .render_with_config(&PdfRenderConfig::new().set_target_width(target_width))
            .with_context(|| format!("failed to render page {page_number}"))?;
        let image = bitmap.as_image()?;
        let rgba = image.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let buffer =
            SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba.as_raw(), width, height);

        Ok(Image::from_rgba8(buffer))
    }

    pub fn page_aspect_ratio(&self, page_index: u32) -> Result<f32> {
        let page_number = page_index + 1;
        let page = self
            .document
            .pages()
            .get(page_index as PdfPageIndex)
            .with_context(|| format!("failed to load page {page_number}"))?;
        let width = page.width().value;
        let height = page.height().value;

        Ok(sanitize_aspect_ratio(width / height))
    }

    pub fn title(&self) -> String {
        document_title(&self.path)
    }

    pub fn speaker_notes(&self) -> Result<SpeakerNotes> {
        let mut notes = Vec::new();

        for page_index in 0..self.page_count {
            let page = self
                .document
                .pages()
                .get(page_index as PdfPageIndex)
                .with_context(|| format!("failed to load page {}", page_index + 1))?;

            for annotation in page.annotations().iter() {
                if annotation.annotation_type() != PdfPageAnnotationType::Text {
                    continue;
                }

                if let Some(contents) = annotation.contents() {
                    if is_pdf_speaker_note_annotation(None, Some(&contents)) {
                        notes.push((page_index + 1, contents));
                    }
                }
            }
        }

        Ok(SpeakerNotes::from_page_notes(notes))
    }
}

fn document_title(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled PDF")
        .to_owned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{Mutex, OnceLock},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn document_title_uses_file_name() {
        let title = document_title(Path::new("/tmp/decks/product-demo.pdf"));

        assert_eq!(title, "product-demo.pdf");
    }

    #[test]
    fn document_title_falls_back_for_directory_path() {
        let title = document_title(Path::new("/"));

        assert_eq!(title, "Untitled PDF");
    }

    #[test]
    fn pdf_document_state_opens_and_renders_pdf_pages() {
        let _guard = pdfium_test_lock().lock().expect("PDFium test lock");

        if !local_pdfium_available() {
            return;
        }

        let path = write_test_pdf();
        let document = PdfDocumentState::open(path.clone()).expect("test PDF should open");

        assert_eq!(
            document.title(),
            path.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(document.page_count(), 2);

        let first_page = document
            .render_page(0, 200)
            .expect("first page should render");
        assert!(first_page.size().width > 0);
        assert!(first_page.size().height > 0);
        assert_eq!(document.page_aspect_ratio(0).unwrap(), 1.0);

        let second_page = document
            .render_page(1, 200)
            .expect("second page should render");
        assert!(second_page.size().width > 0);
        assert!(second_page.size().height > 0);

        fs::remove_file(path).expect("test PDF should be removable");
    }

    #[test]
    fn pdf_document_state_extracts_marp_speaker_notes() {
        let _guard = pdfium_test_lock().lock().expect("PDFium test lock");

        if !local_pdfium_available() {
            return;
        }

        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/marp-speaker-notes.pdf");
        let document = PdfDocumentState::open(path).expect("sample PDF should open");
        let notes = document
            .speaker_notes()
            .expect("sample PDF speaker notes should be readable");

        assert_eq!(notes.note_for_page_number(1), None);
        assert_eq!(notes.note_for_page_number(2), Some("Presenter note text"));
        assert_eq!(
            notes.note_for_page_number(3),
            Some("\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30ce}\u{30fc}\u{30c8}")
        );
    }

    fn pdfium_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn local_pdfium_available() -> bool {
        std::env::var("PDFIUM_DYNAMIC_LIB_PATH").is_ok()
            || Pdfium::pdfium_platform_library_name_at_path(Path::new("pdfium/lib")).exists()
    }

    fn write_test_pdf() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("quick-presenter-test-{id}.pdf"));

        fs::write(&path, minimal_pdf()).expect("test PDF should be writable");

        path
    }

    fn minimal_pdf() -> Vec<u8> {
        let mut pdf = Vec::from("%PDF-1.4\n".as_bytes());
        let objects = [
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_owned(),
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>\nendobj\n".to_owned(),
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R >>\nendobj\n".to_owned(),
            stream_object(4, "0.2 0.4 0.8 rg\n20 20 160 160 re f\n"),
            "5 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 6 0 R >>\nendobj\n".to_owned(),
            stream_object(6, "0.8 0.3 0.2 rg\n40 40 120 120 re f\n"),
        ];
        let mut offsets = Vec::with_capacity(objects.len() + 1);
        offsets.push(0);

        for object in objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object.as_bytes());
        }

        let xref_offset = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len()).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
                offsets.len()
            )
            .as_bytes(),
        );

        pdf
    }

    fn stream_object(id: usize, contents: &str) -> String {
        format!(
            "{id} 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
            contents.len(),
            contents
        )
    }
}

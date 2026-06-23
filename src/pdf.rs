use std::{
    marker::PhantomData,
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use tracing::debug;

use crate::aspect::sanitize_aspect_ratio;
use crate::notes::{is_pdf_speaker_note_annotation, SpeakerNotes};

const PDFIUM_DYNAMIC_LIB_PATH_ENV: &str = "PDFIUM_DYNAMIC_LIB_PATH";
const PDFIUM_OVERRIDE_GUARD_ENV: &str = "QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE";

/// Worker-local PDF document state.
///
/// In the production runtime, this type should stay on the render worker. The UI
/// thread receives document metadata, rendered pixels, and errors through
/// `RenderEvent` instead of accessing PDFium documents directly. The marker keeps
/// accidental cross-thread moves from compiling while still allowing same-thread
/// smoke and unit-test paths to exercise PDF loading directly.
pub struct PdfDocumentState {
    document: PdfDocument<'static>,
    path: PathBuf,
    page_count: u32,
    _worker_thread_only: PhantomData<Rc<()>>,
}

impl PdfDocumentState {
    pub fn open(path: PathBuf) -> Result<Self> {
        let pdfium = shared_pdfium()?;
        let document = pdfium
            .load_pdf_from_file(&path, None)
            .with_context(|| format!("failed to open PDF: {}", path.display()))?;

        let page_count = document.pages().len();

        Ok(Self {
            document,
            path,
            page_count: page_count.max(0) as u32,
            _worker_thread_only: PhantomData,
        })
    }

    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    pub fn render_page(&self, page_index: u32, target_width: i32) -> Result<Image> {
        Ok(Image::from_rgba8(
            self.render_page_pixels(page_index, target_width)?,
        ))
    }

    pub fn render_page_pixels(
        &self,
        page_index: u32,
        target_width: i32,
    ) -> Result<SharedPixelBuffer<Rgba8Pixel>> {
        let page_number = page_index + 1;
        let page = self
            .document
            .pages()
            .get(page_index as PdfPageIndex)
            .with_context(|| format!("failed to load page {page_number}"))?;
        let render_config = PdfRenderConfig::new()
            .set_target_width(target_width)
            .render_annotations(false);
        let bitmap = page
            .render_with_config(&render_config)
            .with_context(|| format!("failed to render page {page_number}"))?;
        let image = bitmap.as_image()?;
        let rgba = image.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let buffer =
            SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba.as_raw(), width, height);

        Ok(buffer)
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
        self.speaker_notes_cancellable(|| false)?
            .context("speaker notes extraction was cancelled")
    }

    pub fn speaker_notes_cancellable(
        &self,
        is_cancelled: impl Fn() -> bool,
    ) -> Result<Option<SpeakerNotes>> {
        let mut notes = Vec::new();

        for page_index in 0..self.page_count {
            let Some(mut page_notes) =
                self.speaker_notes_for_page_cancellable(page_index, &|| is_cancelled())?
            else {
                return Ok(None);
            };
            notes.append(&mut page_notes);
        }

        Ok(Some(SpeakerNotes::from_page_notes(notes)))
    }

    pub fn speaker_notes_for_page_cancellable(
        &self,
        page_index: u32,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Vec<(u32, String)>>> {
        if is_cancelled() {
            return Ok(None);
        }

        let page = self
            .document
            .pages()
            .get(page_index as PdfPageIndex)
            .with_context(|| format!("failed to load page {}", page_index + 1))?;

        let mut notes = Vec::new();
        for annotation in page.annotations().iter() {
            if is_cancelled() {
                return Ok(None);
            }

            if annotation.annotation_type() != PdfPageAnnotationType::Text {
                continue;
            }

            if let Some(contents) = annotation.contents() {
                if is_pdf_speaker_note_annotation(None, Some(&contents)) {
                    notes.push((page_index + 1, contents));
                }
            }
        }

        Ok(Some(notes))
    }
}

fn shared_pdfium() -> Result<&'static Pdfium> {
    static PDFIUM: std::sync::OnceLock<Pdfium> = std::sync::OnceLock::new();

    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }

    let pdfium = create_pdfium()?;
    let _ = PDFIUM.set(pdfium);

    Ok(PDFIUM
        .get()
        .expect("PDFium should be initialized after successful binding"))
}

fn document_title(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled PDF")
        .to_owned()
}

fn create_pdfium() -> Result<Pdfium> {
    let policy = default_pdfium_load_policy();

    if let Ok(path) = std::env::var(PDFIUM_DYNAMIC_LIB_PATH_ENV) {
        let guard = std::env::var(PDFIUM_OVERRIDE_GUARD_ENV).ok();
        if !pdfium_dynamic_override_allowed(policy, guard.as_deref()) {
            debug!(
                env = PDFIUM_DYNAMIC_LIB_PATH_ENV,
                guard = PDFIUM_OVERRIDE_GUARD_ENV,
                "PDFium dynamic override ignored without explicit packaged-build guard"
            );
        } else {
            return new_or_reuse(Pdfium::bind_to_library(&path))
                .context("failed to bind PDFium from PDFIUM_DYNAMIC_LIB_PATH");
        }
    }

    if let Some(pdfium) = bind_first_existing_pdfium_candidate(bundled_pdfium_library_candidates(
        policy,
        std::env::current_exe().ok().as_deref(),
        std::env::current_dir().ok().as_deref(),
    ))? {
        return Ok(pdfium);
    }

    if policy == PdfiumLoadPolicy::Development {
        return new_or_reuse(Pdfium::bind_to_system_library())
            .context("failed to bind system PDFium");
    }

    bail!("failed to bind bundled PDFium: no packaged PDFium library found")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PdfiumLoadPolicy {
    Packaged,
    Development,
}

fn default_pdfium_load_policy() -> PdfiumLoadPolicy {
    if cfg!(debug_assertions) {
        PdfiumLoadPolicy::Development
    } else {
        PdfiumLoadPolicy::Packaged
    }
}

fn pdfium_dynamic_override_allowed(policy: PdfiumLoadPolicy, guard: Option<&str>) -> bool {
    policy == PdfiumLoadPolicy::Development || env_flag_enabled(guard)
}

fn env_flag_enabled(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("1" | "true" | "yes")
    )
}

fn bundled_pdfium_library_candidates(
    policy: PdfiumLoadPolicy,
    current_exe: Option<&Path>,
    current_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let mut candidates = packaged_pdfium_library_candidates(current_exe);

    if policy == PdfiumLoadPolicy::Development {
        if let Some(current_dir) = current_dir {
            push_pdfium_layout_candidates(&mut candidates, &current_dir.join("pdfium"));
        }
    }

    dedupe_paths(candidates)
}

fn packaged_pdfium_library_candidates(current_exe: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(exe_dir) = current_exe.and_then(Path::parent) {
        if let Some(parent_dir) = exe_dir.parent() {
            if exe_dir.file_name().and_then(|name| name.to_str()) == Some("MacOS") {
                push_pdfium_layout_candidates(
                    &mut candidates,
                    &parent_dir.join("Resources/pdfium"),
                );
                push_pdfium_layout_candidates(
                    &mut candidates,
                    &parent_dir.join("Frameworks/pdfium"),
                );
            }
        }

        push_pdfium_layout_candidates(&mut candidates, &exe_dir.join("pdfium"));

        if let Some(parent_dir) = exe_dir.parent() {
            push_pdfium_layout_candidates(&mut candidates, &parent_dir.join("pdfium"));
        }
    }

    candidates
}

fn bind_first_existing_pdfium_candidate(candidates: Vec<PathBuf>) -> Result<Option<Pdfium>> {
    for candidate in candidates {
        if !candidate.exists() {
            debug!(
                path = %candidate.display(),
                "PDFium bundled candidate does not exist"
            );
            continue;
        }

        return new_or_reuse(Pdfium::bind_to_library(&candidate))
            .with_context(|| format!("failed to bind bundled PDFium: {}", candidate.display()))
            .map(Some);
    }

    Ok(None)
}

fn push_pdfium_layout_candidates(candidates: &mut Vec<PathBuf>, pdfium_dir: &Path) {
    for library_dir in [
        pdfium_dir.join("lib"),
        pdfium_dir.join("bin"),
        pdfium_dir.to_path_buf(),
    ] {
        candidates.push(Pdfium::pdfium_platform_library_name_at_path(&library_dir));
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped = Vec::new();

    for path in paths {
        if !deduped.contains(&path) {
            deduped.push(path);
        }
    }

    deduped
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
    fn bundled_pdfium_candidates_include_raw_artifact_layout() {
        let exe = Path::new("/tmp/quick-presenter/quick-presenter");
        let candidates =
            bundled_pdfium_library_candidates(PdfiumLoadPolicy::Packaged, Some(exe), None);

        assert!(candidates.contains(&platform_library_at("/tmp/quick-presenter/pdfium/lib")));
        assert!(candidates.contains(&platform_library_at("/tmp/quick-presenter/pdfium/bin")));
        assert!(candidates.contains(&platform_library_at("/tmp/quick-presenter/pdfium")));
    }

    #[test]
    fn bundled_pdfium_candidates_include_macos_app_layout() {
        let exe = Path::new("/Applications/Quick Presenter.app/Contents/MacOS/quick-presenter");
        let candidates =
            bundled_pdfium_library_candidates(PdfiumLoadPolicy::Packaged, Some(exe), None);

        assert!(candidates.contains(&platform_library_at(
            "/Applications/Quick Presenter.app/Contents/Resources/pdfium/lib"
        )));
        assert!(candidates.contains(&platform_library_at(
            "/Applications/Quick Presenter.app/Contents/Frameworks/pdfium/lib"
        )));
        assert!(candidates.contains(&platform_library_at(
            "/Applications/Quick Presenter.app/Contents/MacOS/pdfium/lib"
        )));
    }

    #[test]
    fn bundled_pdfium_candidates_include_development_layout() {
        let candidates = bundled_pdfium_library_candidates(
            PdfiumLoadPolicy::Development,
            None,
            Some(Path::new("/work/quick-presenter")),
        );

        assert!(candidates.contains(&platform_library_at("/work/quick-presenter/pdfium/lib")));
    }

    #[test]
    fn bundled_pdfium_candidates_exclude_development_layout_for_packaged_policy() {
        let candidates = bundled_pdfium_library_candidates(
            PdfiumLoadPolicy::Packaged,
            None,
            Some(Path::new("/work/quick-presenter")),
        );

        assert!(!candidates.contains(&platform_library_at("/work/quick-presenter/pdfium/lib")));
    }

    #[test]
    fn bundled_pdfium_candidates_are_deduplicated() {
        let exe = Path::new("/work/quick-presenter/quick-presenter");
        let cwd = Path::new("/work/quick-presenter");
        let candidates =
            bundled_pdfium_library_candidates(PdfiumLoadPolicy::Development, Some(exe), Some(cwd));

        let unique_count = candidates
            .iter()
            .filter(|candidate| {
                **candidate == platform_library_at("/work/quick-presenter/pdfium/lib")
            })
            .count();

        assert_eq!(unique_count, 1);
    }

    #[test]
    fn dynamic_override_is_allowed_for_development_policy() {
        assert!(pdfium_dynamic_override_allowed(
            PdfiumLoadPolicy::Development,
            None
        ));
    }

    #[test]
    fn dynamic_override_requires_guard_for_packaged_policy() {
        assert!(!pdfium_dynamic_override_allowed(
            PdfiumLoadPolicy::Packaged,
            None
        ));
        assert!(!pdfium_dynamic_override_allowed(
            PdfiumLoadPolicy::Packaged,
            Some("0")
        ));
        assert!(pdfium_dynamic_override_allowed(
            PdfiumLoadPolicy::Packaged,
            Some("1")
        ));
        assert!(pdfium_dynamic_override_allowed(
            PdfiumLoadPolicy::Packaged,
            Some("true")
        ));
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

    #[test]
    fn pdf_document_state_can_cancel_speaker_notes_extraction() {
        let _guard = pdfium_test_lock().lock().expect("PDFium test lock");

        if !local_pdfium_available() {
            return;
        }

        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/marp-speaker-notes.pdf");
        let document = PdfDocumentState::open(path).expect("sample PDF should open");
        let notes = document
            .speaker_notes_cancellable(|| true)
            .expect("cancelled speaker notes extraction should not fail");

        assert_eq!(notes, None);
    }

    #[test]
    fn pdf_document_state_extracts_readme_sample_speaker_notes() {
        let _guard = pdfium_test_lock().lock().expect("PDFium test lock");

        if !local_pdfium_available() {
            return;
        }

        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/samples/quick-presenter-demo.pdf");
        let document = PdfDocumentState::open(path).expect("README sample PDF should open");
        let notes = document
            .speaker_notes()
            .expect("README sample PDF speaker notes should be readable");

        assert_eq!(
            notes.note_for_page_number(1),
            Some(
                "Open with the product promise: Quick Presenter does one thing well by playing prepared PDF slide decks with presenter-focused controls."
            )
        );
        let english_note = notes
            .note_for_page_number(4)
            .expect("English sample flow slide should have speaker notes");
        assert!(english_note.contains("English counterpart"));
        let japanese_note = notes
            .note_for_page_number(6)
            .expect("Japanese sample slide should have speaker notes");
        assert!(japanese_note.contains("README"));
        assert!(japanese_note.contains("UI"));
    }

    #[test]
    fn pdf_document_state_extracts_long_speaker_notes_fixture() {
        let _guard = pdfium_test_lock().lock().expect("PDFium test lock");

        if !local_pdfium_available() {
            return;
        }

        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/long-speaker-notes.pdf");
        let document = PdfDocumentState::open(path).expect("long-note fixture PDF should open");
        let notes = document
            .speaker_notes()
            .expect("long-note fixture speaker notes should be readable");
        let note = notes
            .note_for_page_number(1)
            .expect("first page should have long speaker notes");

        assert!(note.contains("deliberately long speaker note"));
        assert!(note.contains("End of the long speaker note."));
    }

    fn pdfium_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn local_pdfium_available() -> bool {
        std::env::var("PDFIUM_DYNAMIC_LIB_PATH").is_ok()
            || Pdfium::pdfium_platform_library_name_at_path(Path::new("pdfium/lib")).exists()
    }

    fn platform_library_at(path: impl AsRef<Path>) -> PathBuf {
        Pdfium::pdfium_platform_library_name_at_path(path.as_ref())
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

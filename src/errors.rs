use std::path::Path;

use anyhow::Error;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MessageSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PresenterMessage {
    text: String,
    severity: MessageSeverity,
}

impl PresenterMessage {
    pub fn new(text: impl Into<String>, severity: MessageSeverity) -> Self {
        Self {
            text: text.into(),
            severity,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn severity(&self) -> MessageSeverity {
        self.severity
    }
}

pub fn presenter_error_message(
    error: &Error,
    diagnostic_log_path: Option<&Path>,
) -> PresenterMessage {
    let chain = error_chain_text(error);

    if contains_any(
        &chain,
        &[
            "failed to bind PDFium",
            "failed to bind bundled PDFium",
            "failed to bind local PDFium",
            "failed to bind system PDFium",
            "PDFIUM_DYNAMIC_LIB_PATH",
        ],
    ) {
        return PresenterMessage::new(
            "PDF engine unavailable. Reinstall Quick Presenter or use the documented PDFium override.",
            MessageSeverity::Error,
        );
    }

    if chain.contains("failed to open PDF") {
        return PresenterMessage::new(
            "Could not open PDF. Choose another file.",
            MessageSeverity::Error,
        );
    }

    if contains_any(&chain, &["failed to load page", "failed to render page"]) {
        return PresenterMessage::new(
            "Could not render this page. Try another PDF or page.",
            MessageSeverity::Error,
        );
    }

    unexpected_error_message(diagnostic_log_path)
}

pub fn speaker_notes_warning(_error: &Error) -> PresenterMessage {
    PresenterMessage::new(
        "Ready. Speaker notes unavailable.",
        MessageSeverity::Warning,
    )
}

pub fn render_worker_failed_message(diagnostic_log_path: Option<&Path>) -> PresenterMessage {
    let text = diagnostic_log_path
        .map(|path| {
            format!(
                "Rendering stopped. Open the PDF again. Diagnostic log: {}",
                path.display()
            )
        })
        .unwrap_or_else(|| "Rendering stopped. Open the PDF again.".to_owned());

    PresenterMessage::new(text, MessageSeverity::Error)
}

fn error_chain_text(error: &Error) -> String {
    error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn unexpected_error_message(diagnostic_log_path: Option<&Path>) -> PresenterMessage {
    let text = diagnostic_log_path
        .map(|path| format!("Unexpected error. Diagnostic log: {}", path.display()))
        .unwrap_or_else(|| "Unexpected error. Diagnostic log unavailable.".to_owned());

    PresenterMessage::new(text, MessageSeverity::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn pdfium_binding_errors_are_actionable() {
        let error = anyhow!("library not found").context("failed to bind system PDFium");

        let message = presenter_error_message(&error, Some(Path::new("/tmp/quick-presenter.log")));

        assert_eq!(
            message.text(),
            "PDF engine unavailable. Reinstall Quick Presenter or use the documented PDFium override."
        );
        assert_eq!(message.severity(), MessageSeverity::Error);
    }

    #[test]
    fn pdf_open_errors_are_short() {
        let error = anyhow!("invalid file").context("failed to open PDF: /tmp/not-a-pdf.pdf");

        let message = presenter_error_message(&error, Some(Path::new("/tmp/quick-presenter.log")));

        assert_eq!(message.text(), "Could not open PDF. Choose another file.");
    }

    #[test]
    fn page_load_errors_are_render_errors() {
        let error = anyhow!("page index out of bounds").context("failed to load page 4");

        let message = presenter_error_message(&error, Some(Path::new("/tmp/quick-presenter.log")));

        assert_eq!(
            message.text(),
            "Could not render this page. Try another PDF or page."
        );
    }

    #[test]
    fn page_render_errors_are_render_errors() {
        let error = anyhow!("bitmap failure").context("failed to render page 2");

        let message = presenter_error_message(&error, Some(Path::new("/tmp/quick-presenter.log")));

        assert_eq!(
            message.text(),
            "Could not render this page. Try another PDF or page."
        );
    }

    #[test]
    fn unknown_errors_show_diagnostic_log_path() {
        let error = anyhow!("something unusual happened");

        let message = presenter_error_message(&error, Some(Path::new("/tmp/quick-presenter.log")));

        assert_eq!(
            message.text(),
            "Unexpected error. Diagnostic log: /tmp/quick-presenter.log"
        );
    }

    #[test]
    fn unknown_errors_handle_missing_diagnostic_log_path() {
        let error = anyhow!("something unusual happened");

        let message = presenter_error_message(&error, None);

        assert_eq!(
            message.text(),
            "Unexpected error. Diagnostic log unavailable."
        );
    }

    #[test]
    fn notes_errors_are_non_blocking_warnings() {
        let error = anyhow!("annotation read failed");

        let message = speaker_notes_warning(&error);

        assert_eq!(message.text(), "Ready. Speaker notes unavailable.");
        assert_eq!(message.severity(), MessageSeverity::Warning);
    }

    #[test]
    fn render_worker_failures_show_diagnostic_log_path() {
        let message = render_worker_failed_message(Some(Path::new("/tmp/quick-presenter.log")));

        assert_eq!(
            message.text(),
            "Rendering stopped. Open the PDF again. Diagnostic log: /tmp/quick-presenter.log"
        );
        assert_eq!(message.severity(), MessageSeverity::Error);
    }

    #[test]
    fn render_worker_failures_work_without_diagnostic_log_path() {
        let message = render_worker_failed_message(None);

        assert_eq!(message.text(), "Rendering stopped. Open the PDF again.");
        assert_eq!(message.severity(), MessageSeverity::Error);
    }
}

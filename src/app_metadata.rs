pub const APP_NAME: &str = "Quick Presenter";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_LICENSE_ID: &str = "Apache-2.0";
pub const APP_LICENSE_SUMMARY: &str = "Quick Presenter is licensed under Apache License 2.0.";

pub const PDFIUM_LICENSE_NOTICE: &str = "\
Packaged builds may bundle PDFium native libraries. PDFium and its third-party \
dependencies are distributed under their respective licenses. Windows and macOS \
packages must include the full PDFium license files alongside the bundled \
PDFium binaries.";

pub fn about_version_label() -> String {
    format!("Version {APP_VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_label_uses_cargo_package_version() {
        assert_eq!(
            about_version_label(),
            format!("Version {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn app_license_is_apache_only() {
        assert_eq!(APP_LICENSE_ID, "Apache-2.0");
        assert!(APP_LICENSE_SUMMARY.contains("Apache License 2.0"));
        assert!(!APP_LICENSE_SUMMARY.contains("MIT"));
    }

    #[test]
    fn pdfium_notice_mentions_packaged_license_files() {
        assert!(PDFIUM_LICENSE_NOTICE.contains("PDFium"));
        assert!(PDFIUM_LICENSE_NOTICE.contains("license files"));
    }
}

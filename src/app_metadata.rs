pub const APP_NAME: &str = "Quick Presenter";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const LINUX_DESKTOP_APP_ID: &str = "quick-presenter";
pub const APP_LICENSE_ID: &str = "GPL-3.0-only";
pub const APP_LICENSE_SUMMARY: &str =
    "Quick Presenter is licensed under the GNU General Public License v3.0 only.";
pub const PDFIUM_VERSION_UNKNOWN_LABEL: &str = "PDFium version: unknown";

pub const PDFIUM_LICENSE_SUMMARY: &str = "\
PDFium is distributed under the PDFium/BSD-style license and includes \
third-party components under their respective licenses. Packaged builds must \
include the full PDFium license files alongside the bundled \
PDFium binaries.";

pub struct AboutMetadata {
    pub app_name: &'static str,
    pub app_version_label: String,
    pub app_license_id: &'static str,
    pub app_license_summary: &'static str,
    pub pdfium_version_label: String,
    pub pdfium_license_summary: &'static str,
}

pub fn about_metadata(pdfium_version_label: impl Into<String>) -> AboutMetadata {
    AboutMetadata {
        app_name: APP_NAME,
        app_version_label: about_version_label(),
        app_license_id: APP_LICENSE_ID,
        app_license_summary: APP_LICENSE_SUMMARY,
        pdfium_version_label: pdfium_version_label.into(),
        pdfium_license_summary: PDFIUM_LICENSE_SUMMARY,
    }
}

pub fn about_version_label() -> String {
    format!("Version {APP_VERSION}")
}

pub(crate) fn pdfium_version_label_from_path(path: impl AsRef<std::path::Path>) -> String {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| parse_pdfium_version(&contents))
        .map(|version| format!("PDFium version: {version}"))
        .unwrap_or_else(|| PDFIUM_VERSION_UNKNOWN_LABEL.to_string())
}

fn parse_pdfium_version(contents: &str) -> Option<String> {
    let major = version_field(contents, "MAJOR")?;
    let minor = version_field(contents, "MINOR")?;
    let build = version_field(contents, "BUILD")?;
    let patch = version_field(contents, "PATCH")?;

    Some(format!("{major}.{minor}.{build}.{patch}"))
}

fn version_field(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (field, value) = line.split_once('=')?;
        (field.trim() == key)
            .then(|| value.trim())
            .filter(|value| !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()))
            .map(str::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{ErrorKind, Write},
        path::PathBuf,
    };

    #[test]
    fn about_metadata_uses_cargo_package_version() {
        assert_eq!(
            about_metadata(PDFIUM_VERSION_UNKNOWN_LABEL).app_version_label,
            format!("Version {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn about_metadata_uses_gplv3_only_license() {
        let metadata = about_metadata(PDFIUM_VERSION_UNKNOWN_LABEL);

        assert_eq!(metadata.app_license_id, "GPL-3.0-only");
        assert!(metadata
            .app_license_summary
            .contains("GNU General Public License v3.0 only"));
        assert!(!metadata.app_license_summary.contains("MIT"));
    }

    #[test]
    fn linux_desktop_app_id_matches_desktop_entry_basename() {
        assert_eq!(LINUX_DESKTOP_APP_ID, "quick-presenter");
    }

    #[test]
    fn parse_pdfium_version_reads_major_minor_build_patch() {
        assert_eq!(
            parse_pdfium_version("MAJOR=151\nMINOR=0\nBUILD=7881\nPATCH=0\n"),
            Some("151.0.7881.0".to_string())
        );
    }

    #[test]
    fn parse_pdfium_version_rejects_missing_fields() {
        assert_eq!(
            parse_pdfium_version("MAJOR=151\nMINOR=0\nBUILD=7881\n"),
            None
        );
    }

    #[test]
    fn parse_pdfium_version_rejects_non_numeric_fields() {
        assert_eq!(
            parse_pdfium_version("MAJOR=151\nMINOR=0\nBUILD=7881\nPATCH=dev\n"),
            None
        );
    }

    #[test]
    fn pdfium_version_label_reads_version_file() {
        let path = temp_version_file("MAJOR=151\nMINOR=0\nBUILD=7881\nPATCH=0\n");

        assert_eq!(
            pdfium_version_label_from_path(&path),
            "PDFium version: 151.0.7881.0"
        );

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn pdfium_version_label_falls_back_to_unknown_when_missing_or_invalid() {
        let path = temp_version_file("MAJOR=151\n");
        let missing_path = path.with_extension("missing");

        assert_eq!(
            pdfium_version_label_from_path(&path),
            PDFIUM_VERSION_UNKNOWN_LABEL
        );
        assert_eq!(
            pdfium_version_label_from_path(missing_path),
            PDFIUM_VERSION_UNKNOWN_LABEL
        );

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn pdfium_license_summary_mentions_full_license_files() {
        let metadata = about_metadata(PDFIUM_VERSION_UNKNOWN_LABEL);

        assert!(metadata.pdfium_license_summary.contains("PDFium"));
        assert!(metadata.pdfium_license_summary.contains("license files"));
    }

    fn temp_version_file(contents: &str) -> PathBuf {
        for attempt in 0..100 {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "quick-presenter-pdfium-version-test-{}-{nonce}-{attempt}.txt",
                std::process::id(),
            ));

            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    file.write_all(contents.as_bytes())
                        .expect("temp version file should be writable");
                    return path;
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("temp version file should be created: {error}"),
            }
        }

        panic!("unique temp version file path should be available");
    }
}

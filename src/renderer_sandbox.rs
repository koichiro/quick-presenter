//! Authority reduction before the first untrusted PDF reaches PDFium.
use anyhow::Result;
use std::path::Path;

pub fn enter(document: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    macos::enter(document)?;
    let _ = document;
    Ok(())
}

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::{ensure, Context, Result};
    use std::{ffi::CString, path::Path};

    #[link(name = "sandbox")]
    unsafe extern "C" {
        fn sandbox_init(
            profile: *const libc::c_char,
            flags: u64,
            error: *mut *mut libc::c_char,
        ) -> libc::c_int;
        fn sandbox_free_error(error: *mut libc::c_char);
    }

    fn literal(path: &Path) -> Result<String> {
        let path = path.to_str().context("sandbox path must be UTF-8")?;
        ensure!(!path.chars().any(char::is_control), "invalid sandbox path");
        Ok(format!(
            "\"{}\"",
            path.replace('\\', "\\\\").replace('"', "\\\"")
        ))
    }

    pub(super) fn enter(document: &Path) -> Result<()> {
        // Trusted initialization must precede confinement, but no PDF is parsed.
        crate::pdf::initialize_renderer_runtime()?;
        let document = document.canonicalize()?;
        let profile = format!(
            "(version 1) (deny default) \
             (allow file-read* (literal {}) (subpath \"/System/Library\") \
             (subpath \"/usr/lib\") (subpath \"/Library/Fonts\") \
             (literal \"/dev/urandom\") (literal \"/dev/random\")) \
             (allow sysctl-read) (allow signal (target self))",
            literal(&document)?
        );
        #[cfg(debug_assertions)]
        let mut profile = profile;
        // Fault markers are test-owned and never granted by release builds.
        #[cfg(debug_assertions)]
        if let Some(marker) = std::env::var_os("QUICK_PRESENTER_HELPER_TEST_FAULT_ONCE") {
            if let Ok(marker) = Path::new(&marker).canonicalize() {
                profile.push_str(&format!(
                    " (allow file-read* file-write-unlink (literal {}))",
                    literal(&marker)?
                ));
            }
        }
        let profile = CString::new(profile)?;
        let mut error = std::ptr::null_mut();
        // SAFETY: valid NUL-terminated profile and writable error pointer. This
        // is irreversible process-wide confinement, installed before PDF parsing.
        let status = unsafe { sandbox_init(profile.as_ptr(), 0, &mut error) };
        if !error.is_null() {
            // Never log a compiler error containing the selected PDF path.
            unsafe { sandbox_free_error(error) };
        }
        ensure!(
            status == 0,
            "renderer sandbox unavailable; refusing PDF work"
        );
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn profile_literals_cannot_inject_rules() {
            assert_eq!(
                literal(Path::new("/tmp/a\"b\\c")).unwrap(),
                "\"/tmp/a\\\"b\\\\c\""
            );
            assert!(literal(Path::new("/tmp/a\nb")).is_err());
        }
    }
}

/// A release-capable packaged denial gate. The broker supplies a readable
/// sentinel outside the grant; no switch can disable production confinement.
pub fn verify_denials() -> Result<()> {
    let Some(sentinel) = std::env::var_os("QUICK_PRESENTER_SANDBOX_DENIAL_PROBE") else {
        return Ok(());
    };
    anyhow::ensure!(
        std::fs::File::open(&sentinel).is_err(),
        "sandbox allowed unrelated file read"
    );
    anyhow::ensure!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&sentinel)
            .is_err(),
        "sandbox allowed unrelated file write"
    );
    anyhow::ensure!(
        std::net::TcpListener::bind("127.0.0.1:0").is_err(),
        "sandbox allowed network listen"
    );
    // No external endpoint or DNS is contacted. A broker-owned loopback listener
    // separately verifies connection denial in the packaged gate.
    if let Ok(address) = std::env::var("QUICK_PRESENTER_SANDBOX_CONNECT_PROBE") {
        let address = address.parse()?;
        anyhow::ensure!(
            std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(1))
                .is_err(),
            "sandbox allowed network connect"
        );
    }
    #[cfg(unix)]
    anyhow::ensure!(
        std::process::Command::new("/usr/bin/true")
            .status()
            .is_err(),
        "sandbox allowed child execution"
    );
    Ok(())
}

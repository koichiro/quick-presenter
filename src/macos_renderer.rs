//! Signed XPC bootstrap; a distinct proxy gives each document its own service.
use anyhow::{ensure, Context, Result};
use std::os::unix::{
    io::{AsRawFd, FromRawFd},
    process::CommandExt,
};
use std::{
    fs::File,
    path::{Path, PathBuf},
    process::Command,
};

const SERVICE: &str = "org.quickpresenter.renderer.xpc";
pub fn verify_denials() -> Result<()> {
    let Some(path) = std::env::var_os("QUICK_PRESENTER_SANDBOX_DENIAL_PROBE") else {
        return Ok(());
    };
    ensure!(
        File::open(&path).is_err(),
        "sandbox allowed unrelated file read"
    );
    ensure!(
        std::fs::OpenOptions::new().write(true).open(&path).is_err(),
        "sandbox allowed unrelated file write"
    );
    ensure!(
        std::net::TcpListener::bind("127.0.0.1:0").is_err(),
        "sandbox allowed network listen"
    );
    if let Ok(address) = std::env::var("QUICK_PRESENTER_SANDBOX_CONNECT_PROBE") {
        ensure!(
            std::net::TcpStream::connect_timeout(
                &address.parse()?,
                std::time::Duration::from_secs(1)
            )
            .is_err(),
            "sandbox allowed network connect"
        );
    }
    ensure!(
        Command::new("/usr/bin/true").status().is_err(),
        "renderer allowed child creation"
    );
    Ok(())
}
unsafe extern "C" {
    fn qp_xpc_proxy() -> libc::c_int;
    fn qp_xpc_service();
}
fn app_contents(exe: &Path) -> Option<PathBuf> {
    let directory = exe.parent()?;
    let contents = directory.parent()?;
    (contents.file_name()? == "Contents" && contents.parent()?.extension()? == "app")
        .then(|| contents.to_owned())
}
pub fn configure(command: Command, document: Option<&Path>) -> Result<Command> {
    let exe = std::env::current_exe()?;
    let Some(contents) = app_contents(&exe) else {
        ensure!(
            cfg!(debug_assertions),
            "macOS release renderer requires a signed app bundle"
        );
        return Ok(command);
    };
    let proxy = contents.join("Helpers/RendererProxy.app/Contents/MacOS/quick-presenter-proxy");
    let document = document.context("XPC requires a broker-selected PDF")?;
    crate::pdf::preflight_pdf_input(document)?;
    let file = File::open(document)?;
    let mut configured = Command::new(proxy);
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            configured.env(key, value);
        } else {
            configured.env_remove(key);
        }
    }
    #[cfg(debug_assertions)]
    if let Some(marker) = std::env::var_os("QUICK_PRESENTER_HELPER_TEST_FAULT_ONCE") {
        if std::fs::remove_file(marker).is_err() {
            configured.env_remove("QUICK_PRESENTER_HELPER_TEST_FAULT");
        }
    }
    // The only extra inherited object is the broker's read-only PDF descriptor.
    // File lives until spawn's pre_exec; the closure owns it without allocation
    // or locks in the forked process.
    unsafe {
        configured.pre_exec(move || {
            if libc::dup2(file.as_raw_fd(), 3) < 0 || libc::fcntl(3, libc::F_SETFD, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(configured)
}
pub fn entry() -> Option<Result<()>> {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => return Some(Err(error.into())),
    };
    if let Some(service) = exe
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == SERVICE))
    {
        return Some((|| {
            let _contents = service
                .parent()
                .and_then(Path::parent)
                .context("invalid XPC nesting")?;
            // SAFETY: signature-authenticated bootstrap transfers owned FDs.
            unsafe {
                qp_xpc_service();
            }
            anyhow::bail!("XPC service returned unexpectedly")
        })());
    }
    if exe
        .file_name()
        .is_some_and(|name| name == "quick-presenter-proxy")
    {
        return Some((|| {
            let _contents = app_contents(&exe).context("invalid proxy bundle")?;
            ensure!(
                std::env::args_os().nth(1).as_deref()
                    == Some(std::ffi::OsStr::new(
                        crate::renderer_helper::HELPER_ARGUMENT
                    )),
                "invalid proxy mode"
            );
            // SAFETY: XPC transfers only stdio and the broker-owned read-only FD.
            ensure!(
                unsafe { qp_xpc_proxy() } == 0,
                "XPC renderer startup failed"
            );
            Ok(())
        })());
    }
    None
}
#[no_mangle]
extern "C" fn qp_renderer_run(document_fd: libc::c_int) {
    // The authenticated bootstrap duplicates this FD exactly once. Never unwind
    // through libdispatch's C callback boundary or expose PDF errors to logs.
    let result = std::panic::catch_unwind(|| {
        let file = unsafe { File::from_raw_fd(document_fd) };
        crate::renderer_helper::run_with_input(Some(file))
    });
    if !matches!(result, Ok(Ok(()))) {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundle_paths_require_an_actual_app_contents_directory() {
        assert_eq!(
            app_contents(Path::new("/tmp/Q.app/Contents/MacOS/quick-presenter")),
            Some("/tmp/Q.app/Contents".into())
        );
        assert_eq!(
            app_contents(Path::new(
                "/tmp/Q.app/Contents/Helpers/quick-presenter-proxy"
            )),
            Some("/tmp/Q.app/Contents".into())
        );
        assert!(app_contents(Path::new("/tmp/Contents/MacOS/quick-presenter")).is_none());
        assert!(app_contents(Path::new("/tmp/Q.app/quick-presenter")).is_none());
    }
}

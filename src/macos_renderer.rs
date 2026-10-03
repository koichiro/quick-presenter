//! Signed XPC bootstrap; a distinct proxy gives each document its own service.
use anyhow::{ensure, Context, Result};
use std::os::unix::{
    io::{AsRawFd, FromRawFd},
    process::CommandExt,
};
use std::{
    fs::{File, Metadata},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, OnceLock,
    },
    time::{Duration, Instant, SystemTime},
};

use crate::{
    renderer_helper::ProcessGroup,
    renderer_limits::Operation,
    renderer_supervision::{FailureKind, HelperFailure},
};

pub(crate) type FileIdentity = (u64, Option<SystemTime>);

// POSIX file-provider/network IO cannot reliably be interrupted in-process.
// Bound both the caller's wait and the number of abandoned acquisition threads.
const MAX_INPUT_ACQUISITIONS: usize = 2;
struct AcquisitionSlot(Arc<AtomicUsize>);
impl Drop for AcquisitionSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn acquire_input_with(
    slots: Arc<AtomicUsize>,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
    acquire: impl FnOnce() -> Result<(File, Metadata)> + Send + 'static,
) -> Result<(File, Metadata)> {
    ensure!(!cancelled(), "renderer shutdown already requested");
    let deadline = Instant::now() + timeout;
    slots
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < MAX_INPUT_ACQUISITIONS).then_some(count + 1)
        })
        .map_err(|_| anyhow::anyhow!("PDF input acquisition busy; retry after storage responds"))?;
    let slot = AcquisitionSlot(slots);
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("pdf-input-acquisition".into())
        .spawn(move || {
            let _slot = slot;
            // A late result, including its owned FD, is dropped if the caller left.
            let _ = sender.send(acquire());
        })
        .context("cannot start PDF input acquisition")?;
    loop {
        ensure!(!cancelled(), "renderer shutdown already requested");
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(HelperFailure {
                kind: FailureKind::Timeout,
                operation: Operation::Open,
            }
            .into());
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(25))) {
            Ok(result) => {
                ensure!(!cancelled(), "renderer shutdown already requested");
                return result;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("PDF input acquisition failed")
            }
        }
    }
}

fn acquire_input(path: &Path, group: &ProcessGroup) -> Result<(File, Metadata)> {
    static SLOTS: OnceLock<Arc<AtomicUsize>> = OnceLock::new();
    let path = path.to_owned();
    acquire_input_with(
        Arc::clone(SLOTS.get_or_init(|| Arc::new(AtomicUsize::new(0)))),
        Operation::Open.deadline(),
        || group.is_stopped(),
        move || {
            #[cfg(debug_assertions)]
            if std::env::var("QUICK_PRESENTER_HELPER_TEST_FAULT").as_deref()
                == Ok("hang-before-input")
                && std::env::var_os("QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE")
                    .is_none_or(|title| path.file_name() == Some(title.as_os_str()))
            {
                std::thread::sleep(Operation::Open.deadline() * 4);
            }
            // O_NONBLOCK prevents a replaced path/FIFO from blocking open before
            // descriptor-based regular-file validation. Regular file IO can still stall.
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(path)?;
            let metadata = file.metadata()?;
            ensure!(
                metadata.is_file()
                    && metadata.len() > 0
                    && metadata.len() <= crate::renderer_limits::MAX_PDF_BYTES,
                "invalid brokered PDF size/type"
            );
            Ok((file, metadata))
        },
    )
}

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
    fn qp_xpc_has_signing_team() -> bool;
    fn qp_xpc_proxy() -> libc::c_int;
    fn qp_xpc_service();
}
fn app_contents(exe: &Path) -> Option<PathBuf> {
    let directory = exe.parent()?;
    let contents = directory.parent()?;
    (contents.file_name()? == "Contents" && contents.parent()?.extension()? == "app")
        .then(|| contents.to_owned())
}
pub fn configure(
    command: Command,
    document: Option<&Path>,
    group: &ProcessGroup,
) -> Result<(Command, Option<FileIdentity>)> {
    let exe = std::env::current_exe()?;
    let contents = app_contents(&exe);
    if contents.is_some() {
        // SAFETY: this only reads signing metadata for the running broker.
        // Reject layout-only bundles before acquiring a PDF or spawning a proxy.
        ensure!(
            unsafe { qp_xpc_has_signing_team() },
            "macOS app bundle has no signing Team ID; sign it with scripts/sign_macos_app.sh before PDF validation"
        );
    }
    if contents.is_none() {
        ensure!(
            cfg!(debug_assertions),
            "macOS release renderer requires a signed app bundle"
        );
    }
    let acquired = document
        .map(|path| acquire_input(path, group))
        .transpose()?;
    let identity = acquired
        .as_ref()
        .map(|(_, metadata)| (metadata.len(), metadata.modified().ok()));
    let Some(contents) = contents else {
        return Ok((command, identity));
    };
    let proxy = contents.join("Helpers/RendererProxy.app/Contents/MacOS/quick-presenter-proxy");
    let (file, _) = acquired.context("XPC requires a broker-selected PDF")?;
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
    Ok((configured, identity))
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
    fn acquired_fixture() -> Result<(File, Metadata)> {
        let file = File::open(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/marp-speaker-notes.pdf"),
        )?;
        let metadata = file.metadata()?;
        Ok((file, metadata))
    }
    fn wait_for_slots(slots: &AtomicUsize, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while slots.load(Ordering::Acquire) != count {
            assert!(
                Instant::now() < deadline,
                "acquisition slot was not released"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn stalled_input_times_out_bounds_threads_and_discards_late_results() {
        let slots = Arc::new(AtomicUsize::new(0));
        let mut releases = Vec::new();
        for _ in 0..MAX_INPUT_ACQUISITIONS {
            let (release, blocked) = mpsc::channel();
            releases.push(release);
            let start = Instant::now();
            let error = acquire_input_with(
                Arc::clone(&slots),
                Duration::from_millis(50),
                || false,
                move || {
                    blocked.recv().unwrap();
                    acquired_fixture()
                },
            )
            .unwrap_err();
            assert!(start.elapsed() < Duration::from_secs(1));
            let failure = error.downcast_ref::<HelperFailure>().unwrap();
            assert_eq!(failure.kind, FailureKind::Timeout);
            assert_eq!(failure.operation, Operation::Open);
        }
        let error = acquire_input_with(
            Arc::clone(&slots),
            Duration::from_secs(1),
            || false,
            || panic!("busy acquisition must not start more IO"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("acquisition busy"));
        for release in releases {
            release.send(()).unwrap();
        }
        wait_for_slots(&slots, 0);
        assert!(
            acquire_input_with(slots, Duration::from_secs(1), || false, acquired_fixture).is_ok()
        );
    }
    #[test]
    fn shutdown_interrupts_input_wait_before_deadline() {
        let slots = Arc::new(AtomicUsize::new(0));
        let (release, blocked) = mpsc::channel();
        let start = Instant::now();
        let error = acquire_input_with(
            Arc::clone(&slots),
            Duration::from_secs(30),
            || start.elapsed() >= Duration::from_millis(50),
            move || {
                blocked.recv().unwrap();
                acquired_fixture()
            },
        )
        .unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(error.to_string().contains("shutdown"));
        release.send(()).unwrap();
        wait_for_slots(&slots, 0);
    }
    #[test]
    fn input_acquisition_checks_descriptor_type_without_reading_pdf_header() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let (_, metadata) = acquire_input(&path, &ProcessGroup::default()).unwrap();
        assert!(metadata.is_file());
        assert!(acquire_input(Path::new("/dev/null"), &ProcessGroup::default()).is_err());
    }
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

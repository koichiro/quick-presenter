use std::{
    env,
    ffi::OsString,
    fs,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use anyhow::{ensure, Context, Result};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::app_metadata::APP_NAME;

const LOG_FILE_NAME: &str = "quick-presenter.log";
const LINUX_STATE_DIR_NAME: &str = "quick-presenter";
const WINDOWS_LOG_DIR_NAME: &str = "Logs";
const LOG_GENERATION_BYTES: u64 = 10 * 1024 * 1024;
const PREVIOUS_LOG_SUFFIX: &str = ".1";

pub struct Diagnostics {
    log_path: Option<PathBuf>,
    _guard: Option<WorkerGuard>,
}

impl Diagnostics {
    pub fn log_path(&self) -> Option<&Path> {
        self.log_path.as_deref()
    }
}

pub fn init_diagnostics(log_file_path: Option<PathBuf>) -> Result<Diagnostics> {
    let explicit_log_path = log_file_path.is_some();
    let log_path = log_file_path.or_else(default_log_file_path);

    match log_path {
        Some(path) => match init_file_tracing(&path, explicit_log_path) {
            Ok(guard) => Ok(Diagnostics {
                log_path: Some(path),
                _guard: Some(guard),
            }),
            Err(err) if explicit_log_path => Err(err),
            Err(err) => {
                eprintln!("failed to initialize diagnostic log file: {err:#}");
                init_stderr_tracing()?;
                Ok(Diagnostics {
                    log_path: None,
                    _guard: None,
                })
            }
        },
        None => {
            init_stderr_tracing()?;
            Ok(Diagnostics {
                log_path: None,
                _guard: None,
            })
        }
    }
}

pub fn default_log_file_path() -> Option<PathBuf> {
    let env = HostEnvironment;

    diagnostic_log_file_path(current_platform(), &env)
}

fn init_file_tracing(path: &Path, explicit_log_path: bool) -> Result<WorkerGuard> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create diagnostic log directory: {}",
                parent.display()
            )
        })?;
    }

    let writer = RotatingLogWriter::new(path, LOG_GENERATION_BYTES)?;
    let writer = DiagnosticLogWriter::new(writer, !explicit_log_path);
    let (writer, guard) = tracing_appender::non_blocking(writer);

    tracing_subscriber::fmt()
        .with_env_filter(default_env_filter()?)
        .with_writer(writer)
        .init();

    Ok(guard)
}

struct DiagnosticLogWriter<W> {
    writer: Option<W>,
    fallback_to_stderr: bool,
}

impl<W> DiagnosticLogWriter<W> {
    fn new(writer: W, fallback_to_stderr: bool) -> Self {
        Self {
            writer: Some(writer),
            fallback_to_stderr,
        }
    }
}

impl<W: Write> Write for DiagnosticLogWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.writer.is_none() && self.fallback_to_stderr {
            return io::stderr().write(bytes);
        }

        let result = self
            .writer
            .as_mut()
            .ok_or_else(file_logging_disabled_error)
            .and_then(|writer| writer.write(bytes));

        match result {
            Ok(written) => Ok(written),
            Err(error) if self.fallback_to_stderr => {
                eprintln!("diagnostic file logging disabled: {error}");
                self.writer = None;
                io::stderr().write(bytes)
            }
            Err(error) => Err(error),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.writer.is_none() && self.fallback_to_stderr {
            return io::stderr().flush();
        }

        let result = self
            .writer
            .as_mut()
            .ok_or_else(file_logging_disabled_error)
            .and_then(Write::flush);

        match result {
            Ok(()) => Ok(()),
            Err(error) if self.fallback_to_stderr => {
                eprintln!("diagnostic file logging disabled: {error}");
                self.writer = None;
                io::stderr().flush()
            }
            Err(error) => Err(error),
        }
    }
}

fn file_logging_disabled_error() -> io::Error {
    io::Error::other("diagnostic file logging is disabled")
}

struct RotatingLogWriter {
    active_path: PathBuf,
    previous_path: PathBuf,
    file: Option<File>,
    active_bytes: u64,
    generation_limit: u64,
}

impl RotatingLogWriter {
    fn new(active_path: &Path, generation_limit: u64) -> Result<Self> {
        ensure!(
            generation_limit > 0,
            "diagnostic log limit must be positive"
        );

        let previous_path = previous_log_path(active_path);
        prepare_existing_generation(active_path, generation_limit)?;
        prepare_existing_generation(&previous_path, generation_limit)?;

        let file = open_log_file(active_path).with_context(|| {
            format!(
                "failed to open diagnostic log file: {}",
                active_path.display()
            )
        })?;
        secure_log_file_permissions(active_path).with_context(|| {
            format!(
                "failed to secure diagnostic log file: {}",
                active_path.display()
            )
        })?;
        let active_bytes = file
            .metadata()
            .with_context(|| {
                format!(
                    "failed to inspect diagnostic log file: {}",
                    active_path.display()
                )
            })?
            .len();

        Ok(Self {
            active_path: active_path.to_owned(),
            previous_path,
            file: Some(file),
            active_bytes,
            generation_limit,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        let mut file = self.file.take().ok_or_else(file_logging_disabled_error)?;
        file.flush()?;
        drop(file);

        if self.previous_path.try_exists()? {
            fs::remove_file(&self.previous_path)?;
        }
        fs::rename(&self.active_path, &self.previous_path)?;
        secure_log_file_permissions(&self.previous_path)?;

        let file = open_log_file(&self.active_path)?;
        secure_log_file_permissions(&self.active_path)?;
        self.file = Some(file);
        self.active_bytes = 0;
        Ok(())
    }

    fn write_payload(&mut self, bytes: &[u8]) -> io::Result<()> {
        let file = self.file.as_mut().ok_or_else(file_logging_disabled_error)?;
        file.write_all(bytes)?;
        self.active_bytes = self.active_bytes.saturating_add(bytes.len() as u64);
        Ok(())
    }
}

impl Write for RotatingLogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }

        let incoming_bytes = bytes.len() as u64;
        if incoming_bytes >= self.generation_limit {
            if self.active_bytes > 0 {
                self.rotate()?;
            }
            let keep = usize::try_from(self.generation_limit)
                .map_err(|_| io::Error::other("diagnostic log limit exceeds platform capacity"))?;
            self.write_payload(&bytes[bytes.len() - keep..])?;
            return Ok(bytes.len());
        }

        if self.active_bytes.saturating_add(incoming_bytes) > self.generation_limit {
            self.rotate()?;
        }
        self.write_payload(bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file
            .as_mut()
            .ok_or_else(file_logging_disabled_error)?
            .flush()
    }
}

fn previous_log_path(active_path: &Path) -> PathBuf {
    let mut path = OsString::from(active_path.as_os_str());
    path.push(PREVIOUS_LOG_SUFFIX);
    PathBuf::from(path)
}

fn prepare_existing_generation(path: &Path, generation_limit: u64) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect diagnostic log generation: {}",
                    path.display()
                )
            });
        }
    };
    ensure!(
        metadata.is_file(),
        "diagnostic log generation is not a regular file: {}",
        path.display()
    );

    secure_log_file_permissions(path).with_context(|| {
        format!(
            "failed to secure diagnostic log generation: {}",
            path.display()
        )
    })?;

    let size = metadata.len();
    if size <= generation_limit {
        return Ok(());
    }

    let keep = usize::try_from(generation_limit)
        .context("diagnostic log limit exceeds platform capacity")?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| {
            format!(
                "failed to open oversized diagnostic log: {}",
                path.display()
            )
        })?;
    file.seek(SeekFrom::Start(size - generation_limit))
        .with_context(|| format!("failed to seek diagnostic log: {}", path.display()))?;
    let mut tail = vec![0; keep];
    file.read_exact(&mut tail)
        .with_context(|| format!("failed to read diagnostic log tail: {}", path.display()))?;
    file.seek(SeekFrom::Start(0))
        .with_context(|| format!("failed to rewind diagnostic log: {}", path.display()))?;
    file.write_all(&tail)
        .with_context(|| format!("failed to rewrite diagnostic log: {}", path.display()))?;
    file.set_len(generation_limit)
        .with_context(|| format!("failed to truncate diagnostic log: {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush diagnostic log: {}", path.display()))?;
    Ok(())
}

fn open_log_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path)
}

#[cfg(unix)]
fn secure_log_file_permissions(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn secure_log_file_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn init_stderr_tracing() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(default_env_filter()?)
        .init();
    Ok(())
}

fn default_env_filter() -> Result<EnvFilter> {
    Ok(EnvFilter::from_default_env().add_directive("info".parse()?))
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(dead_code)]
enum Platform {
    Macos,
    Windows,
    Linux,
}

#[cfg(target_os = "macos")]
fn current_platform() -> Platform {
    Platform::Macos
}

#[cfg(target_os = "windows")]
fn current_platform() -> Platform {
    Platform::Windows
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn current_platform() -> Platform {
    Platform::Linux
}

trait Environment {
    fn var_os(&self, key: &str) -> Option<std::ffi::OsString>;
}

struct HostEnvironment;

impl Environment for HostEnvironment {
    fn var_os(&self, key: &str) -> Option<std::ffi::OsString> {
        env::var_os(key)
    }
}

fn diagnostic_log_file_path(platform: Platform, env: &dyn Environment) -> Option<PathBuf> {
    match platform {
        Platform::Macos => env
            .var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Logs").join(APP_NAME).join(LOG_FILE_NAME)),
        Platform::Windows => env
            .var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|local_app_data| {
                local_app_data
                    .join(APP_NAME)
                    .join(WINDOWS_LOG_DIR_NAME)
                    .join(LOG_FILE_NAME)
            }),
        Platform::Linux => env
            .var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                env.var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".local/state"))
            })
            .map(|state_home| state_home.join(LINUX_STATE_DIR_NAME).join(LOG_FILE_NAME)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        ffi::OsString,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[derive(Default)]
    struct FakeEnvironment {
        vars: BTreeMap<String, OsString>,
    }

    impl FakeEnvironment {
        fn with_var(mut self, key: &str, value: &str) -> Self {
            self.vars.insert(key.to_owned(), OsString::from(value));
            self
        }
    }

    impl Environment for FakeEnvironment {
        fn var_os(&self, key: &str) -> Option<OsString> {
            self.vars.get(key).cloned()
        }
    }

    #[test]
    fn macos_log_path_uses_user_library_logs() {
        let env = FakeEnvironment::default().with_var("HOME", "/Users/speaker");

        assert_eq!(
            diagnostic_log_file_path(Platform::Macos, &env),
            Some(PathBuf::from(
                "/Users/speaker/Library/Logs/Quick Presenter/quick-presenter.log"
            ))
        );
    }

    #[test]
    fn windows_log_path_uses_local_app_data() {
        let env =
            FakeEnvironment::default().with_var("LOCALAPPDATA", r"C:\Users\speaker\AppData\Local");

        assert_eq!(
            diagnostic_log_file_path(Platform::Windows, &env),
            Some(PathBuf::from(
                r"C:\Users\speaker\AppData\Local/Quick Presenter/Logs/quick-presenter.log"
            ))
        );
    }

    #[test]
    fn linux_log_path_prefers_xdg_state_home() {
        let env = FakeEnvironment::default()
            .with_var("XDG_STATE_HOME", "/home/speaker/.state")
            .with_var("HOME", "/home/speaker");

        assert_eq!(
            diagnostic_log_file_path(Platform::Linux, &env),
            Some(PathBuf::from(
                "/home/speaker/.state/quick-presenter/quick-presenter.log"
            ))
        );
    }

    #[test]
    fn linux_log_path_falls_back_to_home_local_state() {
        let env = FakeEnvironment::default().with_var("HOME", "/home/speaker");

        assert_eq!(
            diagnostic_log_file_path(Platform::Linux, &env),
            Some(PathBuf::from(
                "/home/speaker/.local/state/quick-presenter/quick-presenter.log"
            ))
        );
    }

    #[test]
    fn log_path_is_unavailable_without_required_environment() {
        let env = FakeEnvironment::default();

        assert_eq!(diagnostic_log_file_path(Platform::Macos, &env), None);
        assert_eq!(diagnostic_log_file_path(Platform::Windows, &env), None);
        assert_eq!(diagnostic_log_file_path(Platform::Linux, &env), None);
    }

    #[test]
    fn writer_rotates_only_after_the_generation_limit() {
        let temp = TempDirectory::new("rotation-boundary");
        let active = temp.path().join("app.log");
        let previous = previous_log_path(&active);
        let mut writer = RotatingLogWriter::new(&active, 8).unwrap();

        writer.write_all(b"1234567").unwrap();
        assert_eq!(fs::read(&active).unwrap(), b"1234567");
        assert!(!previous.exists());

        writer.write_all(b"8").unwrap();
        writer.flush().unwrap();
        assert_eq!(fs::read(&active).unwrap(), b"12345678");
        assert!(!previous.exists());

        writer.write_all(b"9").unwrap();
        writer.flush().unwrap();
        assert_eq!(fs::read(&active).unwrap(), b"9");
        assert_eq!(fs::read(&previous).unwrap(), b"12345678");
    }

    #[test]
    fn repeated_rotation_keeps_exactly_one_previous_generation() {
        let temp = TempDirectory::new("repeated-rotation");
        let active = temp.path().join("app.log");
        let previous = previous_log_path(&active);
        let mut writer = RotatingLogWriter::new(&active, 4).unwrap();

        writer.write_all(b"1234").unwrap();
        writer.write_all(b"5").unwrap();
        writer.write_all(b"678").unwrap();
        writer.write_all(b"9").unwrap();
        writer.flush().unwrap();

        assert_eq!(fs::read(&active).unwrap(), b"9");
        assert_eq!(fs::read(&previous).unwrap(), b"5678");
        assert!(!temp.path().join("app.log.2").exists());
    }

    #[test]
    fn startup_trims_oversized_generations_to_their_newest_bytes() {
        let temp = TempDirectory::new("startup-trim");
        let active = temp.path().join("app.log");
        let previous = previous_log_path(&active);
        fs::write(&active, b"abcdefghijkl").unwrap();
        fs::write(&previous, b"0123456789ABC").unwrap();

        let mut writer = RotatingLogWriter::new(&active, 8).unwrap();
        writer.flush().unwrap();

        assert_eq!(fs::read(&active).unwrap(), b"efghijkl");
        assert_eq!(fs::read(&previous).unwrap(), b"56789ABC");
    }

    #[test]
    fn oversized_write_keeps_only_its_newest_bytes() {
        let temp = TempDirectory::new("oversized-write");
        let active = temp.path().join("app.log");
        let mut writer = RotatingLogWriter::new(&active, 8).unwrap();

        assert_eq!(writer.write(b"abcdefghijkl").unwrap(), 12);
        writer.flush().unwrap();

        assert_eq!(fs::read(&active).unwrap(), b"efghijkl");
    }

    #[test]
    fn rotation_preserves_unrelated_sibling_files() {
        let temp = TempDirectory::new("cleanup-scope");
        let active = temp.path().join("app.log");
        let unrelated = temp.path().join("app.log.notes");
        fs::write(&unrelated, b"keep me").unwrap();
        let mut writer = RotatingLogWriter::new(&active, 4).unwrap();

        writer.write_all(b"1234").unwrap();
        writer.write_all(b"5").unwrap();
        writer.flush().unwrap();

        assert_eq!(fs::read(&unrelated).unwrap(), b"keep me");
    }

    #[test]
    fn startup_rejects_non_file_generation_without_removing_it() {
        let temp = TempDirectory::new("non-file-generation");
        let active = temp.path().join("app.log");
        fs::create_dir(&active).unwrap();

        let error = match RotatingLogWriter::new(&active, 8) {
            Ok(_) => panic!("directory must not be accepted as a log generation"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("not a regular file"));
        assert!(active.is_dir());
    }

    #[test]
    fn default_writer_falls_back_after_file_failure() {
        let mut writer = DiagnosticLogWriter::new(AlwaysFailWriter, true);

        assert_eq!(writer.write(b"fallback diagnostic\n").unwrap(), 20);
        assert!(writer.writer.is_none());
    }

    #[test]
    fn explicit_writer_surfaces_file_failure() {
        let mut writer = DiagnosticLogWriter::new(AlwaysFailWriter, false);

        let error = writer.write(b"diagnostic").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(writer.writer.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn writer_creates_and_repairs_owner_only_generations() {
        let temp = TempDirectory::new("unix-permissions");
        let active = temp.path().join("app.log");
        let previous = previous_log_path(&active);
        fs::write(&active, b"1234").unwrap();
        fs::write(&previous, b"old").unwrap();
        fs::set_permissions(&active, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&previous, fs::Permissions::from_mode(0o644)).unwrap();

        let mut writer = RotatingLogWriter::new(&active, 4).unwrap();
        assert_eq!(file_mode(&active), 0o600);
        assert_eq!(file_mode(&previous), 0o600);

        writer.write_all(b"5").unwrap();
        writer.flush().unwrap();
        assert_eq!(file_mode(&active), 0o600);
        assert_eq!(file_mode(&previous), 0o600);
    }

    struct AlwaysFailWriter;

    impl Write for AlwaysFailWriter {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("injected log failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("injected log failure"))
        }
    }

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new(label: &str) -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = env::temp_dir().join(format!(
                "quick-presenter-diagnostics-{label}-{}-{unique}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(unix)]
    fn file_mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }
}

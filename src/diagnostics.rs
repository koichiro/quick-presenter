use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

use crate::app_metadata::APP_NAME;

const LOG_FILE_NAME: &str = "quick-presenter.log";
const LINUX_STATE_DIR_NAME: &str = "quick-presenter";
const WINDOWS_LOG_DIR_NAME: &str = "Logs";

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
        Some(path) => match init_file_tracing(&path) {
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

fn init_file_tracing(path: &Path) -> Result<WorkerGuard> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create diagnostic log directory: {}",
                parent.display()
            )
        })?;
    }

    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open diagnostic log file: {}", path.display()))?;
    let (writer, guard) = tracing_appender::non_blocking(file);

    tracing_subscriber::fmt()
        .with_env_filter(default_env_filter()?)
        .with_writer(writer)
        .init();

    Ok(guard)
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
    use std::{collections::BTreeMap, ffi::OsString};

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
}

use anyhow::{Context, Result};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temp_path = temporary_sibling_path(path);

    let write_result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .private_file_permissions()
            .open(&temp_path)
            .with_context(|| format!("failed to create temporary file: {}", temp_path.display()))?;
        restrict_private_file_permissions(&temp_path).with_context(|| {
            format!(
                "failed to restrict temporary file permissions: {}",
                temp_path.display()
            )
        })?;
        file.write_all(contents)
            .with_context(|| format!("failed to write temporary file: {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync temporary file: {}", temp_path.display()))?;
        drop(file);

        replace_file(&temp_path, path).with_context(|| {
            format!(
                "failed to replace {} with {}",
                path.display(),
                temp_path.display()
            )
        })?;
        sync_directory(parent);
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    write_result
}

trait PrivateFileOpenOptionsExt {
    fn private_file_permissions(&mut self) -> &mut Self;
}

impl PrivateFileOpenOptionsExt for OpenOptions {
    #[cfg(unix)]
    fn private_file_permissions(&mut self) -> &mut Self {
        self.mode(0o600)
    }

    #[cfg(not(unix))]
    fn private_file_permissions(&mut self) -> &mut Self {
        self
    }
}

#[cfg(unix)]
fn restrict_private_file_permissions(path: &Path) -> std::io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_private_file_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn temporary_sibling_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temp_name = format!(".{file_name}.{}.{}.tmp", std::process::id(), unique);

    path.with_file_name(temp_name)
}

#[cfg(not(target_os = "windows"))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(target_os = "windows")]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain([0]).collect();
    let destination: Vec<u16> = destination.as_os_str().encode_wide().chain([0]).collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn sync_directory(path: &Path) {
    if let Ok(directory) = fs::File::open(path) {
        let _ = directory.sync_all();
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn platform_config_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support/Quick Presenter"))
}

#[cfg(target_os = "windows")]
pub(crate) fn platform_config_dir() -> Option<PathBuf> {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|appdata| appdata.join("Quick Presenter"))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub(crate) fn platform_config_dir() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|config| config.join("quick-presenter"))
}

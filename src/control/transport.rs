use std::{io, path::PathBuf};

#[cfg(unix)]
pub fn endpoint() -> io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let uid = unsafe { libc::geteuid() };
    let base = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(path) => {
            let path = PathBuf::from(path);
            let metadata = std::fs::symlink_metadata(&path)?;
            if !path.is_absolute()
                || !metadata.is_dir()
                || metadata.uid() != uid
                || metadata.mode() & 0o077 != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Unsafe XDG_RUNTIME_DIR",
                ));
            }
            path.join("quick-presenter")
        }
        None => PathBuf::from(format!("/tmp/quick-presenter-{uid}")),
    };
    match std::fs::DirBuilder::new().mode(0o700).create(&base) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    validate_directory(&base)?;
    Ok(base.join("control.sock"))
}
#[cfg(unix)]
pub fn validate_directory(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Control directory must be owned by this user with mode 0700",
        ));
    }
    Ok(())
}
#[cfg(unix)]
pub fn validate_socket(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    validate_directory(
        path.parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing socket parent"))?,
    )?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Unsafe control socket",
        ));
    }
    Ok(())
}
#[cfg(not(unix))]
pub fn endpoint() -> io::Result<PathBuf> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Local control is currently available only on macOS and Linux",
    ))
}

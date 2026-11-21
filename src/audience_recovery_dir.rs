//! Small OS boundary for a recovery directory and relative file operations.
use std::{ffi::OsStr, fs::File, io, path::Path};

pub(crate) struct Directory {
    #[cfg(unix)]
    file: File,
    #[cfg(not(unix))]
    path: std::path::PathBuf,
    #[cfg(windows)]
    file: File,
}

#[cfg(unix)]
impl Directory {
    pub fn open(path: &Path) -> io::Result<Self> {
        use std::{
            os::fd::{AsRawFd, FromRawFd},
            os::unix::fs::{MetadataExt, OpenOptionsExt},
            path::Component,
        };
        let uid = unsafe { libc::geteuid() };
        #[cfg(target_os = "linux")]
        let traverse = libc::O_PATH;
        #[cfg(not(target_os = "linux"))]
        let traverse = libc::O_RDONLY;
        let mut current = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(traverse | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")?;
        let root = current.metadata()?;
        if root.uid() != 0 && root.uid() != uid
            || root.mode() & 0o022 != 0
                && !(root.uid() == 0 && root.mode() & libc::S_ISVTX as u32 != 0)
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let parts = path
            .components()
            .filter_map(|c| match c {
                Component::Normal(name) => Some(Ok(name)),
                Component::RootDir => None,
                _ => Some(Err(io::Error::from(io::ErrorKind::InvalidInput))),
            })
            .collect::<io::Result<Vec<_>>>()?;
        let count = parts.len();
        for (index, name) in parts.into_iter().enumerate() {
            let access = if index + 1 == count {
                libc::O_RDONLY
            } else {
                traverse
            };
            let name = name_cstring(name)?;
            let fd = unsafe {
                libc::openat(
                    current.as_raw_fd(),
                    name.as_ptr(),
                    access | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            current = unsafe { File::from_raw_fd(fd) };
            let metadata = current.metadata()?;
            let trusted_owner = metadata.uid() == 0 || metadata.uid() == uid;
            let protected_sticky =
                metadata.uid() == 0 && metadata.mode() & libc::S_ISVTX as u32 != 0;
            if !trusted_owner || metadata.mode() & 0o022 != 0 && !protected_sticky {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
        }
        let metadata = current.metadata()?;
        if metadata.uid() != uid || metadata.mode() & 0o022 != 0 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(Self { file: current })
    }

    pub fn matches(&self, path: &Path) -> io::Result<bool> {
        use std::os::unix::fs::MetadataExt;
        let current = Self::open(&path.canonicalize()?)?;
        let left = self.file.metadata()?;
        let right = current.file.metadata()?;
        Ok(left.dev() == right.dev() && left.ino() == right.ino())
    }

    fn open_at(&self, name: &OsStr, flags: i32) -> io::Result<File> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let name = name_cstring(name)?;
        let fd = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_CLOEXEC | libc::O_NONBLOCK,
                0o600,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    pub fn lock_file(&self, name: &OsStr) -> io::Result<File> {
        self.open_at(name, libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW)
    }

    pub fn read_file(&self, name: &OsStr, follow: bool) -> io::Result<File> {
        self.open_at(
            name,
            libc::O_RDONLY | if follow { 0 } else { libc::O_NOFOLLOW },
        )
    }

    pub fn write_atomic(&self, name: &OsStr, bytes: &[u8]) -> io::Result<()> {
        use std::{io::Write, os::fd::AsRawFd};
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(io::Error::other)?;
        let mut temp = name.to_os_string();
        temp.push(format!(
            ".{}.tmp",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ));
        let temp_c = name_cstring(&temp)?;
        let target = name_cstring(name)?;
        let result = (|| {
            let mut file = self.open_at(
                &temp,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
            )?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            if unsafe {
                libc::renameat(
                    self.file.as_raw_fd(),
                    temp_c.as_ptr(),
                    self.file.as_raw_fd(),
                    target.as_ptr(),
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            self.file.sync_all()
        })();
        if result.is_err() {
            unsafe {
                libc::unlinkat(self.file.as_raw_fd(), temp_c.as_ptr(), 0);
            }
        }
        result
    }
}

#[cfg(unix)]
fn name_cstring(name: &OsStr) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    if name.is_empty() || Path::new(name).components().count() != 1 || name == "." || name == ".." {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    std::ffi::CString::new(name.as_bytes()).map_err(|_| io::ErrorKind::InvalidInput.into())
}

#[cfg(not(unix))]
impl Directory {
    pub fn open(path: &Path) -> io::Result<Self> {
        Ok(Self {
            path: path.to_owned(),
            #[cfg(windows)]
            file: windows_directory(path)?,
        })
    }
    #[cfg(windows)]
    fn checked_directory(&self) -> io::Result<File> {
        let file = windows_directory(&self.path)?;
        if !same_windows_directory(&self.file, &file)? {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(file)
    }
    pub fn matches(&self, path: &Path) -> io::Result<bool> {
        #[cfg(windows)]
        {
            return same_windows_directory(&self.file, &windows_directory(path)?);
        }
        #[cfg(not(windows))]
        Ok(self.path.canonicalize()? == path.canonicalize()?)
    }
    pub fn lock_file(&self, name: &OsStr) -> io::Result<File> {
        #[cfg(windows)]
        let _directory = self.checked_directory()?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
            );
        }
        options.open(self.path.join(name))
    }
    pub fn read_file(&self, name: &OsStr, follow: bool) -> io::Result<File> {
        #[cfg(windows)]
        let _directory = self.checked_directory()?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            if !follow {
                options.custom_flags(
                    windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
                );
            }
        }
        #[cfg(not(windows))]
        let _ = follow;
        options.open(self.path.join(name))
    }
    pub fn write_atomic(&self, name: &OsStr, bytes: &[u8]) -> io::Result<()> {
        #[cfg(windows)]
        let _directory = self.checked_directory()?;
        crate::settings_storage::write_atomic(&self.path.join(name), bytes)
            .map_err(|_| io::Error::other("Cannot save recovery record"))
    }
}

#[cfg(windows)]
fn windows_directory(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::*;
    // Deny deletion/renaming of the leaf while this recovery owner is alive.
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(windows)]
fn same_windows_directory(left: &File, right: &File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT,
    };
    fn identity(file: &File) -> io::Result<(u32, u32, u32)> {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    }
    Ok(identity(left)? == identity(right)?)
}

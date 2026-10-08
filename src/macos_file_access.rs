//! UI-owned read-only access to previously selected PDFs in App Sandbox.
use crate::settings_storage::{platform_config_dir, write_atomic};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{c_char, c_void, CStr, CString},
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr::NonNull,
};

const MAX_BOOKMARK_BYTES: usize = 1024 * 1024;
const MAX_STORE_BYTES: u64 = 32 * 1024 * 1024;

unsafe extern "C" {
    fn qp_ui_is_sandboxed() -> bool;
    fn qp_ui_bookmark_create(path: *const c_char, length: *mut usize) -> *mut c_void;
    fn qp_ui_bookmark_open(
        bytes: *const u8,
        length: usize,
        path: *mut u8,
        capacity: usize,
        stale: *mut bool,
    ) -> *mut c_void;
    fn qp_ui_bookmark_close(handle: *mut c_void);
    fn qp_ui_bookmark_free(bytes: *mut c_void);
}

struct Scope(NonNull<c_void>);
impl Drop for Scope {
    fn drop(&mut self) {
        // The native bridge returns a retained URL after exactly one successful start.
        unsafe { qp_ui_bookmark_close(self.0.as_ptr()) }
    }
}

#[derive(Serialize, Deserialize)]
struct Bookmark {
    path: PathBuf,
    data: Vec<u8>,
}

#[derive(Default)]
struct Access {
    bookmarks: Vec<Bookmark>,
    scopes: HashMap<PathBuf, Scope>,
    active: Option<PathBuf>,
    loaded: bool,
}

thread_local! { static ACCESS: RefCell<Access> = RefCell::new(Access::default()); }

fn store_path() -> Result<PathBuf> {
    platform_config_dir()
        .map(|p| p.join("pdf-bookmarks.json"))
        .context("PDF access settings directory unavailable")
}

impl Access {
    fn load(&mut self) -> Result<()> {
        if self.loaded {
            return Ok(());
        }
        let path = store_path()?;
        if path.exists() {
            ensure!(
                fs::metadata(&path)?.len() <= MAX_STORE_BYTES,
                "PDF access settings too large"
            );
            self.bookmarks = decode_history(&fs::read(path)?)?;
        }
        self.loaded = true;
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let path = store_path()?;
        fs::create_dir_all(
            path.parent()
                .context("PDF access settings parent unavailable")?,
        )?;
        write_atomic(&path, &serde_json::to_vec(&self.bookmarks)?)
    }

    fn remember(&mut self, path: &Path) -> Result<()> {
        self.load()?;
        let path_c = CString::new(path.as_os_str().as_bytes())?;
        let mut length = 0;
        // The returned allocation is copied and freed on every result path.
        let bytes = unsafe { qp_ui_bookmark_create(path_c.as_ptr(), &mut length) };
        ensure!(!bytes.is_null(), "could not save read-only PDF access");
        let data = if length > 0 && length <= MAX_BOOKMARK_BYTES {
            Some(unsafe { std::slice::from_raw_parts(bytes.cast::<u8>(), length) }.to_vec())
        } else {
            None
        };
        unsafe { qp_ui_bookmark_free(bytes) };
        let data = data.context("invalid PDF bookmark size")?;
        replace_bookmark(&mut self.bookmarks, path, data);
        self.scopes.retain(|p, _| {
            self.active.as_ref() == Some(p) || self.bookmarks.iter().any(|b| &b.path == p)
        });
        self.save()
    }

    fn restore(&mut self, path: &Path) -> Result<PathBuf> {
        self.load()?;
        if self.scopes.contains_key(path) {
            return Ok(path.to_owned());
        }
        let Some(bookmark) = self.bookmarks.iter().find(|b| b.path == path) else {
            return Ok(path.to_owned());
        };
        let mut output = vec![0u8; 4096];
        let mut stale = false;
        let handle = unsafe {
            qp_ui_bookmark_open(
                bookmark.data.as_ptr(),
                bookmark.data.len(),
                output.as_mut_ptr(),
                output.len(),
                &mut stale,
            )
        };
        let scope = Scope(
            NonNull::new(handle)
                .context("could not restore read-only PDF access; select the PDF again")?,
        );
        // The native bridge guarantees a terminated filesystem representation on success.
        let bytes = unsafe { CStr::from_ptr(output.as_ptr().cast()) }.to_bytes();
        let resolved = PathBuf::from(std::ffi::OsStr::from_bytes(bytes));
        self.scopes.insert(resolved.clone(), scope);
        if stale || resolved != path {
            self.bookmarks.retain(|b| b.path != path);
            self.remember(&resolved)?;
        }
        Ok(resolved)
    }
}

fn sandboxed() -> bool {
    unsafe { qp_ui_is_sandboxed() }
}

pub fn remember_selected(path: &Path) -> Result<()> {
    if sandboxed() {
        ACCESS.with(|a| a.borrow_mut().remember(path))
    } else {
        Ok(())
    }
}

pub fn restore(path: PathBuf) -> Result<PathBuf> {
    if sandboxed() {
        ACCESS.with(|a| a.borrow_mut().restore(&path))
    } else {
        Ok(path)
    }
}

pub fn committed(path: &Path) {
    ACCESS.with(|a| {
        let mut a = a.borrow_mut();
        a.active = Some(path.to_owned());
        a.scopes.retain(|p, _| p == path);
    });
}

fn decode_history(bytes: &[u8]) -> Result<Vec<Bookmark>> {
    ensure!(
        bytes.len() as u64 <= MAX_STORE_BYTES,
        "PDF access settings too large"
    );
    let entries: Vec<Bookmark> = serde_json::from_slice(bytes)?;
    ensure!(
        entries.len() <= crate::recent::MAX_RECENT_FILES,
        "too many PDF access entries"
    );
    ensure!(
        entries
            .iter()
            .all(|b| !b.data.is_empty() && b.data.len() <= MAX_BOOKMARK_BYTES),
        "invalid PDF access entry"
    );
    Ok(entries)
}

fn replace_bookmark(entries: &mut Vec<Bookmark>, path: &Path, data: Vec<u8>) {
    entries.retain(|b| b.path != path);
    entries.insert(
        0,
        Bookmark {
            path: path.to_owned(),
            data,
        },
    );
    entries.truncate(crate::recent::MAX_RECENT_FILES);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_refreshes_access_and_evicts_oldest_history() {
        let mut entries = Vec::new();
        for i in 0..6 {
            replace_bookmark(&mut entries, Path::new(&format!("{i}.pdf")), vec![i]);
        }
        assert_eq!(entries.len(), 5);
        assert!(!entries.iter().any(|b| b.path == Path::new("0.pdf")));
        replace_bookmark(&mut entries, Path::new("3.pdf"), vec![99]);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].path, Path::new("3.pdf"));
        assert_eq!(entries[0].data, vec![99]);
        let decoded = decode_history(&serde_json::to_vec(&entries).unwrap()).unwrap();
        assert_eq!(decoded[0].data, vec![99]);
    }

    #[test]
    fn malformed_and_unbounded_history_is_rejected_before_native_resolution() {
        assert!(decode_history(b"not json").is_err());
        let empty = vec![Bookmark {
            path: "deck.pdf".into(),
            data: Vec::new(),
        }];
        assert!(decode_history(&serde_json::to_vec(&empty).unwrap()).is_err());
        let many: Vec<_> = (0..6)
            .map(|i| Bookmark {
                path: format!("{i}.pdf").into(),
                data: vec![1],
            })
            .collect();
        assert!(decode_history(&serde_json::to_vec(&many).unwrap()).is_err());
        let oversized = vec![Bookmark {
            path: "deck.pdf".into(),
            data: vec![1; MAX_BOOKMARK_BYTES + 1],
        }];
        assert!(decode_history(&serde_json::to_vec(&oversized).unwrap()).is_err());
    }
}

pub fn clear_history() -> Result<()> {
    if !sandboxed() {
        return Ok(());
    }
    ACCESS.with(|a| {
        let mut a = a.borrow_mut();
        a.load()?;
        a.bookmarks.clear();
        // Active access remains alive until replacement or process exit.
        a.save()
    })
}

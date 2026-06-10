use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};

pub const MAX_RECENT_FILES: usize = 5;

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct RecentFiles {
    paths: Vec<PathBuf>,
}

impl RecentFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_paths(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut recent = Self::new();
        for path in paths {
            if !recent.paths.contains(&path) {
                recent.paths.push(path);
            }
            recent.paths.truncate(MAX_RECENT_FILES);
        }
        recent
    }

    pub fn add(&mut self, path: PathBuf) {
        self.paths.retain(|existing| existing != &path);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENT_FILES);
    }

    pub fn clear(&mut self) {
        self.paths.clear();
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct RecentFileStore {
    path: PathBuf,
}

impl RecentFileStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<RecentFiles> {
        if !self.path.exists() {
            return Ok(RecentFiles::new());
        }

        let contents = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read recent files: {}", self.path.display()))?;
        Ok(RecentFiles::from_paths(
            contents
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(PathBuf::from),
        ))
    }

    pub fn save(&self, recent_files: &RecentFiles) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create recent files directory: {}",
                    parent.display()
                )
            })?;
        }

        let mut contents = String::new();
        for path in recent_files.paths() {
            contents.push_str(&path.to_string_lossy());
            contents.push('\n');
        }

        fs::write(&self.path, contents)
            .with_context(|| format!("failed to write recent files: {}", self.path.display()))
    }
}

pub fn default_recent_file_store() -> Option<RecentFileStore> {
    default_recent_files_path().map(RecentFileStore::new)
}

fn default_recent_files_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("QP_RECENT_FILES_PATH") {
        return Some(PathBuf::from(path));
    }

    platform_config_dir().map(|dir| dir.join("recent-files.txt"))
}

#[cfg(target_os = "macos")]
fn platform_config_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support/Quick Presenter"))
}

#[cfg(target_os = "windows")]
fn platform_config_dir() -> Option<PathBuf> {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|appdata| appdata.join("Quick Presenter"))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn platform_config_dir() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|config| config.join("quick-presenter"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_path_puts_it_first() {
        let mut recent = RecentFiles::new();

        recent.add(PathBuf::from("a.pdf"));
        recent.add(PathBuf::from("b.pdf"));

        assert_eq!(
            recent.paths(),
            &[PathBuf::from("b.pdf"), PathBuf::from("a.pdf")]
        );
    }

    #[test]
    fn adding_existing_path_moves_it_to_front() {
        let mut recent = RecentFiles::from_paths([
            PathBuf::from("a.pdf"),
            PathBuf::from("b.pdf"),
            PathBuf::from("c.pdf"),
        ]);

        recent.add(PathBuf::from("b.pdf"));

        assert_eq!(
            recent.paths(),
            &[
                PathBuf::from("b.pdf"),
                PathBuf::from("a.pdf"),
                PathBuf::from("c.pdf")
            ]
        );
    }

    #[test]
    fn recent_files_keeps_at_most_five_paths() {
        let mut recent = RecentFiles::new();

        for name in ["a.pdf", "b.pdf", "c.pdf", "d.pdf", "e.pdf", "f.pdf"] {
            recent.add(PathBuf::from(name));
        }

        assert_eq!(
            recent.paths(),
            &[
                PathBuf::from("f.pdf"),
                PathBuf::from("e.pdf"),
                PathBuf::from("d.pdf"),
                PathBuf::from("c.pdf"),
                PathBuf::from("b.pdf")
            ]
        );
    }

    #[test]
    fn clear_removes_all_paths() {
        let mut recent = RecentFiles::from_paths([PathBuf::from("a.pdf")]);

        recent.clear();

        assert!(recent.is_empty());
    }

    #[test]
    fn store_round_trips_recent_files() {
        let path = env::temp_dir().join(format!(
            "quick-presenter-recent-test-{}.txt",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let store = RecentFileStore::new(path.clone());
        let recent = RecentFiles::from_paths([PathBuf::from("a.pdf"), PathBuf::from("b.pdf")]);

        store.save(&recent).unwrap();
        let loaded = store.load().unwrap();

        assert_eq!(loaded, recent);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn missing_store_file_loads_empty_recent_files() {
        let path = env::temp_dir().join(format!(
            "quick-presenter-missing-recent-test-{}.txt",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let store = RecentFileStore::new(path);

        assert!(store.load().unwrap().is_empty());
    }
}

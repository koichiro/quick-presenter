use crate::settings_storage::{platform_config_dir, write_atomic};
use anyhow::{Context, Result};
#[cfg(all(test, unix))]
use std::os::unix::fs::PermissionsExt;
use std::{env, fs, path::PathBuf};
#[cfg(test)]
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_RECENT_FILES: usize = 5;
pub const EMPTY_RECENT_FILE_LABEL: &str = "No Recent Files";

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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RecentMenuSnapshot {
    paths: Vec<PathBuf>,
    labels: [String; MAX_RECENT_FILES],
    enabled: [bool; MAX_RECENT_FILES],
}

impl RecentMenuSnapshot {
    pub fn from_recent_files(recent_files: &RecentFiles) -> Self {
        let paths = recent_files.paths().to_vec();
        let labels = std::array::from_fn(|index| {
            paths
                .get(index)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| {
                    if index == 0 && paths.is_empty() {
                        EMPTY_RECENT_FILE_LABEL.to_owned()
                    } else {
                        String::new()
                    }
                })
        });
        let enabled = std::array::from_fn(|index| index < paths.len());

        Self {
            paths,
            labels,
            enabled,
        }
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn labels(&self) -> &[String; MAX_RECENT_FILES] {
        &self.labels
    }

    pub fn enabled(&self) -> &[bool; MAX_RECENT_FILES] {
        &self.enabled
    }

    pub fn has_recent_files(&self) -> bool {
        !self.paths.is_empty()
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

        let contents = fs::read(&self.path)
            .with_context(|| format!("failed to read recent files: {}", self.path.display()))?;
        let Ok(contents) = String::from_utf8(contents) else {
            return Ok(RecentFiles::new());
        };

        Ok(parse_recent_files(&contents))
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

        write_atomic(&self.path, serialize_recent_files(recent_files).as_bytes())
            .with_context(|| format!("failed to write recent files: {}", self.path.display()))
    }
}

fn parse_recent_files(contents: &str) -> RecentFiles {
    RecentFiles::from_paths(
        contents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter(|line| !line.contains('\0'))
            .map(PathBuf::from),
    )
}

fn serialize_recent_files(recent_files: &RecentFiles) -> String {
    let mut contents = String::new();
    for path in recent_files.paths() {
        contents.push_str(&path.to_string_lossy());
        contents.push('\n');
    }
    contents
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
    fn menu_snapshot_keeps_paths_labels_and_enabled_state_aligned() {
        let recent =
            RecentFiles::from_paths([PathBuf::from("first.pdf"), PathBuf::from("second.pdf")]);

        let snapshot = RecentMenuSnapshot::from_recent_files(&recent);

        assert_eq!(snapshot.paths(), recent.paths());
        assert_eq!(snapshot.labels()[0], "first.pdf");
        assert_eq!(snapshot.labels()[1], "second.pdf");
        assert_eq!(snapshot.labels()[2], "");
        assert_eq!(snapshot.enabled(), &[true, true, false, false, false]);
        assert!(snapshot.has_recent_files());
    }

    #[test]
    fn empty_menu_snapshot_has_one_disabled_placeholder() {
        let snapshot = RecentMenuSnapshot::from_recent_files(&RecentFiles::new());

        assert!(snapshot.paths().is_empty());
        assert_eq!(snapshot.labels()[0], EMPTY_RECENT_FILE_LABEL);
        assert_eq!(snapshot.labels()[1], "");
        assert_eq!(snapshot.enabled(), &[false; MAX_RECENT_FILES]);
        assert!(!snapshot.has_recent_files());
    }

    #[test]
    fn parser_ignores_empty_duplicate_extra_and_corrupt_lines() {
        let recent = parse_recent_files(
            "a.pdf\n\nb.pdf\na.pdf\nbad\0path.pdf\nc.pdf\nd.pdf\ne.pdf\nf.pdf\n",
        );

        assert_eq!(
            recent.paths(),
            &[
                PathBuf::from("a.pdf"),
                PathBuf::from("b.pdf"),
                PathBuf::from("c.pdf"),
                PathBuf::from("d.pdf"),
                PathBuf::from("e.pdf")
            ]
        );
    }

    #[test]
    fn serializer_writes_only_paths() {
        let recent = RecentFiles::from_paths([PathBuf::from("a.pdf"), PathBuf::from("b.pdf")]);

        assert_eq!(serialize_recent_files(&recent), "a.pdf\nb.pdf\n");
    }

    #[test]
    fn store_round_trips_recent_files() {
        let path = temp_recent_path("round-trip");
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
        let path = temp_recent_path("missing");
        let _ = fs::remove_file(&path);
        let store = RecentFileStore::new(path);

        assert!(store.load().unwrap().is_empty());
    }

    #[test]
    fn invalid_utf8_store_file_loads_empty_recent_files() {
        let path = temp_recent_path("invalid-utf8");
        fs::write(&path, [0xff, 0xfe, b'\n']).unwrap();
        let store = RecentFileStore::new(path.clone());

        assert!(store.load().unwrap().is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn saving_overwrites_existing_file_without_temp_file() {
        let path = temp_recent_path("overwrite");
        fs::write(&path, "old.pdf\n").unwrap();
        let store = RecentFileStore::new(path.clone());
        let recent = RecentFiles::from_paths([PathBuf::from("new.pdf")]);

        store.save(&recent).unwrap();

        assert_eq!(store.load().unwrap(), recent);
        assert!(temporary_files_for(&path).is_empty());

        let _ = fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn saving_creates_owner_only_recent_file_on_unix() {
        let path = temp_recent_path("owner-only");
        let _ = fs::remove_file(&path);
        let store = RecentFileStore::new(path.clone());
        let recent = RecentFiles::from_paths([PathBuf::from("deck.pdf")]);

        store.save(&recent).unwrap();

        assert_eq!(file_mode(&path), 0o600);

        let _ = fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn saving_repairs_broad_existing_recent_file_permissions_on_unix() {
        let path = temp_recent_path("repair-permissions");
        fs::write(&path, "old.pdf\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let store = RecentFileStore::new(path.clone());
        let recent = RecentFiles::from_paths([PathBuf::from("new.pdf")]);

        store.save(&recent).unwrap();

        assert_eq!(store.load().unwrap(), recent);
        assert_eq!(file_mode(&path), 0o600);

        let _ = fs::remove_file(path);
    }

    fn temp_recent_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        env::temp_dir().join(format!(
            "quick-presenter-{label}-{}-{unique}.txt",
            std::process::id()
        ))
    }

    fn temporary_files_for(path: &Path) -> Vec<PathBuf> {
        let Some(parent) = path.parent() else {
            return Vec::new();
        };
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return Vec::new();
        };
        let prefix = format!(".{file_name}.");

        fs::read_dir(parent)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with(&prefix) && name.ends_with(".tmp"))
                    .unwrap_or(false)
            })
            .collect()
    }

    #[cfg(unix)]
    fn file_mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }
}

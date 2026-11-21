//! Private, bounded recovery journal. Never stores bearer or room credentials.
use crate::app_config::RelayEndpoint;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const READ_ERROR: &str =
    "Cannot read Audience recovery data. Keep the same configuration and retry.";
pub(crate) const WRITE_ERROR: &str =
    "Cannot save Audience recovery data. Check the configuration directory and retry.";

#[cfg(unix)]
const DIRECTORY_ERROR: &str = "Audience recovery needs protected, user-owned settings. Check ownership; run chmod 700 on writable folders you own, then retry.";
#[cfg(not(unix))]
const DIRECTORY_ERROR: &str =
    "Audience recovery requires a protected, user-owned configuration directory.";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub version: u8,
    pub binding: String,
    pub key: Option<String>,
    pub may_exist: bool,
    pub end_requested: bool,
}

pub(crate) struct Journal {
    path: PathBuf,
    directory: crate::audience_recovery_dir::Directory,
    config_name: std::ffi::OsString,
    // A separate inode stays locked across atomic journal replacements.
    _lock: File,
}

fn validate_file(file: &File) -> Result<(), &'static str> {
    let metadata = file.metadata().map_err(|_| READ_ERROR)?;
    if !metadata.is_file() {
        return Err(READ_ERROR);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err("Audience recovery data must be owner-only (chmod 600).");
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(READ_ERROR);
        }
    }
    Ok(())
}

pub(crate) fn binding(endpoint: &RelayEndpoint) -> String {
    let mut bytes = endpoint.origin.as_str().as_bytes().to_vec();
    bytes.push(0);
    bytes.extend_from_slice(endpoint.create_token.as_bytes());
    ring::digest::digest(&ring::digest::SHA256, &bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Journal {
    fn open_record(config: &Path) -> Result<(Self, Option<Record>), &'static str> {
        let parent = config
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        crate::settings_storage::create_private_dir_all(parent).map_err(|_| READ_ERROR)?;
        // Anchor the selected filename once; later cwd/directory aliases must
        // not redirect the journal or create a second lock for that filename.
        let config = parent
            .canonicalize()
            .map_err(|_| READ_ERROR)?
            .join(config.file_name().ok_or(READ_ERROR)?);
        if std::fs::symlink_metadata(&config).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Audience configuration destination must not be a symbolic link.");
        }
        let directory =
            crate::audience_recovery_dir::Directory::open(config.parent().ok_or(READ_ERROR)?)
                .map_err(|_| DIRECTORY_ERROR)?;
        let mut name = config.as_os_str().to_os_string();
        name.push(".audience-session.json");
        let path = PathBuf::from(name);
        let mut lock_name = path.as_os_str().to_os_string();
        lock_name.push(".lock");
        let lock = directory
            .lock_file(Path::new(&lock_name).file_name().ok_or(READ_ERROR)?)
            .map_err(|_| {
                "Cannot lock Audience recovery data. Check the configuration directory."
            })?;
        validate_file(&lock)?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => {
                "Another app is using this Audience configuration."
            }
            _ => "Cannot lock Audience recovery data. Check the configuration directory.",
        })?;
        let journal = Self {
            path,
            directory,
            config_name: config.file_name().ok_or(READ_ERROR)?.to_owned(),
            _lock: lock,
        };
        let record = journal.read()?;
        Ok((journal, record))
    }

    pub fn open(
        config: &Path,
        endpoint: &RelayEndpoint,
    ) -> Result<(Self, Option<Record>), &'static str> {
        let (journal, record) = Self::open_record(config)?;
        if let Some(record) = &record {
            // A definitive initial rejection permits correcting credentials. An
            // ambiguous request can only be recovered with its original binding.
            if record.binding != binding(endpoint) && (record.may_exist || record.end_requested) {
                return Err("Audience recovery requires the original configuration. Import the same configuration and retry.");
            }
        }
        Ok((journal, record))
    }
    pub fn for_reset(config: &Path) -> Result<Self, &'static str> {
        let (journal, record) = Self::open_record(config)?;
        if record.is_some_and(|r| r.may_exist || r.end_requested) {
            return Err("Recover the previous Audience session before resetting settings.");
        }
        Ok(journal)
    }

    pub fn validate_configuration(&self, expected: &RelayEndpoint) -> Result<(), &'static str> {
        let file = self
            .directory
            .read_file(&self.config_name, false)
            .map_err(|_| READ_ERROR)?;
        let actual = crate::app_config::load_opened(file)?;
        if actual
            .as_ref()
            .is_none_or(|a| a.origin != expected.origin || a.create_token != expected.create_token)
        {
            return Err("Audience configuration changed during startup. Reload the configuration and retry.");
        }
        Ok(())
    }

    pub fn save_configuration(
        &self,
        destination: &Path,
        endpoint: Option<&RelayEndpoint>,
    ) -> Result<(), &'static str> {
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if destination.file_name() != Some(self.config_name.as_os_str())
            || !self.directory.matches(parent).map_err(|_| WRITE_ERROR)?
        {
            return Err("Audience configuration directory changed. Restart with the original configuration.");
        }
        let text = self
            .directory
            .read_file(&self.config_name, false)
            .and_then(|file| {
                if !file.metadata()?.is_file() {
                    return Err(std::io::ErrorKind::InvalidInput.into());
                }
                let mut text = String::new();
                file.take(16385).read_to_string(&mut text)?;
                Ok(text)
            });
        let text = crate::app_config::merge_section(
            text,
            "audience",
            crate::app_config::audience_fields(endpoint),
        )?;
        self.directory
            .write_atomic(&self.config_name, text.as_bytes())
            .map_err(|_| WRITE_ERROR)
    }

    fn read(&self) -> Result<Option<Record>, &'static str> {
        let file = match self
            .directory
            .read_file(self.path.file_name().ok_or(READ_ERROR)?, false)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(READ_ERROR),
        };
        validate_file(&file)?;
        let mut bytes = Vec::new();
        file.take(4097)
            .read_to_end(&mut bytes)
            .map_err(|_| READ_ERROR)?;
        if bytes.len() > 4096 {
            return Err(READ_ERROR);
        }
        let record: Record = serde_json::from_slice(&bytes).map_err(|_| READ_ERROR)?;
        if record.version != 1
            || record.binding.len() != 64
            || !record.binding.bytes().all(|b| b.is_ascii_hexdigit())
            || record
                .key
                .as_ref()
                .is_some_and(|k| k.len() != 64 || !k.bytes().all(|b| b.is_ascii_hexdigit()))
            || (record.may_exist && record.key.is_none())
        {
            return Err(READ_ERROR);
        }
        Ok(Some(record))
    }

    pub fn save(&self, record: &Record) -> Result<(), &'static str> {
        let bytes = serde_json::to_vec(record).map_err(|_| WRITE_ERROR)?;
        self.directory
            .write_atomic(self.path.file_name().ok_or(WRITE_ERROR)?, &bytes)
            .map_err(|_| WRITE_ERROR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint() -> RelayEndpoint {
        RelayEndpoint {
            origin: url::Url::parse("https://relay.example/").unwrap(),
            create_token: "A".repeat(43),
        }
    }
    fn record(endpoint: &RelayEndpoint) -> Record {
        Record {
            version: 1,
            binding: binding(endpoint),
            key: Some("b".repeat(64)),
            may_exist: true,
            end_requested: true,
        }
    }

    #[cfg(unix)]
    #[test]
    fn profile_creators_under_shared_umask_preserve_recovery_access() {
        use std::os::unix::fs::PermissionsExt;
        const CHILD: &str = "QP_PRIVATE_DIRECTORY_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "audience_recovery::tests::profile_creators_under_shared_umask_preserve_recovery_access", "--test-threads=1", "--nocapture"])
                .env(CHILD, "1").output().unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        // This exact test runs alone in a child, never changing the parent's umask.
        unsafe {
            libc::umask(0o002);
        }
        for route in [
            "comment-display",
            "audience-display",
            "startup",
            "recent",
            "diagnostics",
        ] {
            let profile = root.path().join(route);
            let config = profile.join("config.toml");
            match route {
                "comment-display" => {
                    crate::app_config::save_comment_display(&config, Default::default()).unwrap()
                }
                "audience-display" => {
                    crate::app_config::save_audience_display(&config, Default::default()).unwrap()
                }
                "startup" => {
                    crate::startup_state::StartupStore::new(profile.join("startup-state.json"))
                        .save(&Default::default())
                        .unwrap()
                }
                "recent" => crate::recent::RecentFileStore::new(profile.join("recent-files.txt"))
                    .save(&crate::recent::RecentFiles::new())
                    .unwrap(),
                "diagnostics" => {
                    let _log =
                        crate::diagnostics::init_diagnostics(Some(profile.join("logs/test.log")))
                            .unwrap();
                }
                _ => unreachable!(),
            }
            assert_eq!(
                std::fs::metadata(&profile).unwrap().permissions().mode() & 0o777,
                0o700,
                "{route}"
            );
            let (journal, _) = Journal::open(&config, &endpoint()).unwrap();
            journal
                .save_configuration(&config, Some(&endpoint()))
                .unwrap();
            journal.validate_configuration(&endpoint()).unwrap();
        }
        let existing = root.path().join("existing");
        std::fs::create_dir(&existing).unwrap();
        std::fs::set_permissions(&existing, std::fs::Permissions::from_mode(0o775)).unwrap();
        crate::settings_storage::create_private_dir_all(&existing).unwrap();
        assert_eq!(
            std::fs::metadata(&existing).unwrap().permissions().mode() & 0o777,
            0o775
        );
        let error = Journal::open(&existing.join("config.toml"), &endpoint())
            .err()
            .unwrap();
        assert!(error.contains("chmod 700"));
        assert_eq!(
            std::fs::metadata(&existing).unwrap().permissions().mode() & 0o777,
            0o775
        );
    }

    #[test]
    fn cold_load_retains_identity_and_end_intent_without_bearer_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let endpoint = endpoint();
        let (journal, empty) = Journal::open(&config, &endpoint).unwrap();
        assert!(empty.is_none());
        journal.save(&record(&endpoint)).unwrap();
        let bytes = std::fs::read_to_string(&journal.path).unwrap();
        assert!(!bytes.contains(&endpoint.create_token));
        assert!(!bytes.contains("presenter_token"));
        let path = journal.path.clone();
        drop(journal);
        let (_, restored) = Journal::open(&config, &endpoint).unwrap();
        let restored = restored.unwrap();
        assert_eq!(restored.key.as_deref(), Some("b".repeat(64).as_str()));
        assert!(restored.may_exist && restored.end_requested);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        #[cfg(not(unix))]
        let _ = path;
    }

    #[test]
    fn active_recovery_is_bound_to_origin_and_creation_credential() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let original = endpoint();
        let (journal, _) = Journal::open(&config, &original).unwrap();
        journal.save(&record(&original)).unwrap();
        drop(journal);
        let mut changed = original.clone();
        changed.create_token = "C".repeat(43);
        assert!(Journal::open(&config, &changed).is_err());
        changed = original.clone();
        changed.origin = url::Url::parse("https://another.example/").unwrap();
        assert!(Journal::open(&config, &changed).is_err());
        let (journal, _) = Journal::open(&config, &original).unwrap();
        let mut rejected = record(&original);
        rejected.may_exist = false;
        rejected.end_requested = false;
        journal.save(&rejected).unwrap();
        drop(journal);
        assert!(Journal::open(&config, &changed).is_ok());
    }

    #[test]
    fn lock_excludes_a_second_journal_owner_until_the_first_drops() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let (journal, _) = Journal::open(&config, &endpoint()).unwrap();
        assert!(Journal::open(&config, &endpoint()).is_err());
        drop(journal);
        assert!(Journal::open(&config, &endpoint()).is_ok());
    }

    #[test]
    fn malformed_or_oversized_records_are_not_discarded_or_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let (journal, _) = Journal::open(&config, &endpoint()).unwrap();
        let path = journal.path.clone();
        drop(journal);
        let mut wrong_version = record(&endpoint());
        wrong_version.version = 2;
        let mut invalid_key = record(&endpoint());
        invalid_key.key = Some("bad".into());
        for text in [
            "{".to_owned(),
            "x".repeat(4097),
            serde_json::to_string(&wrong_version).unwrap(),
            serde_json::to_string(&invalid_key).unwrap(),
            format!(
                "{{\"unexpected\":true,{}}}",
                &serde_json::to_string(&record(&endpoint())).unwrap()[1..].trim_end_matches('}')
            ),
        ] {
            crate::settings_storage::write_atomic(&path, text.as_bytes()).unwrap();
            assert!(Journal::open(&config, &endpoint()).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_fifos_and_shared_records_fail_closed() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let (journal, _) = Journal::open(&config, &endpoint()).unwrap();
        let path = journal.path.clone();
        drop(journal);
        let target = dir.path().join("target");
        std::fs::write(&target, b"do not modify").unwrap();
        symlink(&target, &path).unwrap();
        assert!(Journal::open(&config, &endpoint()).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"do not modify");
        std::fs::remove_file(&path).unwrap();
        let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(Journal::open(&config, &endpoint()).is_err());
        std::fs::remove_file(&path).unwrap();
        crate::settings_storage::write_atomic(
            &path,
            &serde_json::to_vec(&record(&endpoint())).unwrap(),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Journal::open(&config, &endpoint()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn writable_shared_directory_is_rejected_before_creating_lock_or_state() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(Journal::open(&dir.path().join("config.toml"), &endpoint()).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn directory_handle_pins_lock_reads_and_writes_after_an_ancestor_moves() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original");
        std::fs::create_dir_all(original.join("profile")).unwrap();
        let config = original.join("profile/config.toml");
        let (journal, _) = Journal::open(&config, &endpoint()).unwrap();
        let moved = dir.path().join("moved");
        std::fs::rename(&original, &moved).unwrap();
        std::fs::create_dir_all(original.join("profile")).unwrap();
        journal.save(&record(&endpoint())).unwrap();
        assert!(journal.read().unwrap().unwrap().end_requested);
        assert!(moved
            .join("profile/config.toml.audience-session.json")
            .exists());
        assert!(!original
            .join("profile/config.toml.audience-session.json")
            .exists());
        assert!(!original
            .join("profile/config.toml.audience-session.json.lock")
            .exists());
    }

    #[cfg(unix)]
    #[test]
    fn moved_configuration_cannot_split_import_from_its_recovery_owner() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original");
        std::fs::create_dir(&original).unwrap();
        let config = original.join("config.toml");
        crate::app_config::save_import(&config, &endpoint()).unwrap();
        let (journal, _) = Journal::open(&config, &endpoint()).unwrap();
        let moved = dir.path().join("moved");
        std::fs::rename(&original, &moved).unwrap();
        std::fs::create_dir(&original).unwrap();
        std::fs::write(&config, "replacement").unwrap();
        let old = std::fs::read(moved.join("config.toml")).unwrap();
        assert!(journal.save_configuration(&config, None).is_err());
        assert_eq!(std::fs::read(&config).unwrap(), b"replacement");
        assert_eq!(std::fs::read(moved.join("config.toml")).unwrap(), old);
        assert!(!original
            .join("config.toml.audience-session.json.lock")
            .exists());
    }

    #[cfg(unix)]
    #[test]
    fn final_configuration_symlinks_cannot_create_a_second_recovery_owner() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        crate::app_config::save_import(&config, &endpoint()).unwrap();
        let (_journal, _) = Journal::open(&config, &endpoint()).unwrap();
        let alias = dir.path().join("alias.toml");
        std::os::unix::fs::symlink(&config, &alias).unwrap();
        assert!(Journal::open(&alias, &endpoint()).is_err());
        assert!(!dir
            .path()
            .join("alias.toml.audience-session.json.lock")
            .exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn searchable_unreadable_intermediate_directory_is_supported() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let ancestor = dir.path().join("searchable");
        let leaf = ancestor.join("private");
        std::fs::create_dir_all(&leaf).unwrap();
        std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o711)).unwrap();
        std::fs::set_permissions(&leaf, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (journal, _) = Journal::open(&leaf.join("config.toml"), &endpoint()).unwrap();
        journal.save(&record(&endpoint())).unwrap();
        std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn writable_ancestor_is_rejected_even_when_the_leaf_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let ancestor = dir.path().join("shared");
        std::fs::create_dir_all(ancestor.join("private")).unwrap();
        std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o777)).unwrap();
        std::fs::set_permissions(
            ancestor.join("private"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert!(Journal::open(&ancestor.join("private/config.toml"), &endpoint()).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_symlink_is_rejected_without_modifying_target() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let (journal, _) = Journal::open(&config, &endpoint()).unwrap();
        let path = journal.path.clone();
        drop(journal);
        let target = dir.path().join("target");
        std::fs::write(&target, b"do not modify").unwrap();
        if let Err(error) = std::os::windows::fs::symlink_file(&target, &path) {
            if error.raw_os_error() == Some(1314) {
                eprintln!("SKIP: Windows symbolic-link privilege is unavailable");
                return;
            }
            panic!("Cannot create symlink fixture: {error}");
        }
        assert!(Journal::open(&config, &endpoint()).is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"do not modify");
    }
}

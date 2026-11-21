//! Private, bounded recovery journal. Never stores bearer or room credentials.
use crate::app_config::RelayEndpoint;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

const READ_ERROR: &str =
    "Cannot read Audience recovery data. Keep the same configuration and retry.";
pub(crate) const WRITE_ERROR: &str =
    "Cannot save Audience recovery data. Check the configuration directory and retry.";

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
    // A separate inode stays locked across atomic journal replacements.
    _lock: File,
}

fn regular_options(write: bool) -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(write);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options
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
    pub fn open(
        config: &Path,
        endpoint: &RelayEndpoint,
    ) -> Result<(Self, Option<Record>), &'static str> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let parent = config
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let metadata = std::fs::metadata(parent).map_err(|_| READ_ERROR)?;
            if !metadata.is_dir()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o022 != 0
            {
                return Err("Audience recovery requires a user-owned configuration directory without group/world write access.");
            }
        }
        let mut name = config.as_os_str().to_os_string();
        name.push(".audience-session.json");
        let path = PathBuf::from(name);
        let mut lock_name = path.as_os_str().to_os_string();
        lock_name.push(".lock");
        let lock = regular_options(true)
            .create(true)
            .truncate(false)
            .open(PathBuf::from(lock_name))
            .map_err(|_| {
                "Cannot lock Audience recovery data. Check the configuration directory."
            })?;
        validate_file(&lock)?;
        lock.try_lock()
            .map_err(|_| "Another app is using this Audience configuration.")?;
        let journal = Self { path, _lock: lock };
        let record = journal.read()?;
        if let Some(record) = &record {
            // A definitive initial rejection permits correcting credentials. An
            // ambiguous request can only be recovered with its original binding.
            if record.binding != binding(endpoint) && (record.may_exist || record.end_requested) {
                return Err("Audience recovery requires the original configuration. Import the same configuration and retry.");
            }
        }
        Ok((journal, record))
    }

    fn read(&self) -> Result<Option<Record>, &'static str> {
        let file = match regular_options(false).open(&self.path) {
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
        crate::settings_storage::write_atomic(&self.path, &bytes).map_err(|_| WRITE_ERROR)
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
        std::os::windows::fs::symlink_file(&target, &path).unwrap();
        assert!(Journal::open(&config, &endpoint()).is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"do not modify");
    }
}

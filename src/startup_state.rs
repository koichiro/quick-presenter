use crate::{
    cli::StartupOptions,
    settings_storage::{platform_config_dir, write_atomic},
    window_placement::PlacementRecord,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_SETTINGS_BYTES: u64 = 256 * 1024;
fn os_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "encoding", content = "value")]
enum NativePath {
    Utf8(String),
    UnixBytes(Vec<u8>),
    WindowsUtf16(Vec<u16>),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPdf {
    platform: String,
    path: NativePath,
}
impl SavedPdf {
    pub fn capture(path: &Path) -> Result<Self> {
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        let encoded = if let Some(value) = absolute.to_str() {
            NativePath::Utf8(value.into())
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;
                NativePath::UnixBytes(absolute.as_os_str().as_bytes().to_vec())
            }
            #[cfg(windows)]
            {
                use std::os::windows::ffi::OsStrExt;
                NativePath::WindowsUtf16(absolute.as_os_str().encode_wide().collect())
            }
        };
        let record = Self {
            platform: os_platform().into(),
            path: encoded,
        };
        anyhow::ensure!(record.decode().is_some(), "invalid active PDF path");
        Ok(record)
    }
    pub fn decode(&self) -> Option<PathBuf> {
        if self.platform != os_platform() {
            return None;
        }
        let path: PathBuf = match &self.path {
            NativePath::Utf8(value) if !value.contains('\0') => value.into(),
            #[cfg(unix)]
            NativePath::UnixBytes(bytes) if !bytes.contains(&0) => {
                use std::os::unix::ffi::OsStringExt;
                std::ffi::OsString::from_vec(bytes.clone()).into()
            }
            #[cfg(windows)]
            NativePath::WindowsUtf16(units) if !units.contains(&0) => {
                use std::os::windows::ffi::OsStringExt;
                std::ffi::OsString::from_wide(units).into()
            }
            _ => return None,
        };
        path.is_absolute().then_some(path)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Default)]
pub struct StartupState {
    pub window_placement: Option<PlacementRecord>,
    pub last_pdf: Option<SavedPdf>,
}
#[derive(Serialize)]
struct Envelope<'a> {
    version: u32,
    #[serde(flatten)]
    state: &'a StartupState,
}

#[derive(Debug)]
pub struct StartupStore {
    path: PathBuf,
}
impl StartupStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn default_store() -> Option<Self> {
        platform_config_dir().map(|p| Self::new(p.join("startup-state.json")))
    }
    pub fn load(&self) -> Result<StartupState> {
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StartupState::default())
            }
            Err(error) => return Err(error).context("failed to read startup settings"),
        };
        let mut bytes = Vec::new();
        file.take(MAX_SETTINGS_BYTES + 1).read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_SETTINGS_BYTES,
            "startup settings exceed the size limit"
        );
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            value.get("version").and_then(|v| v.as_u64()) == Some(1),
            "unsupported startup settings version"
        );
        let window_placement = decode_section::<PlacementRecord>(&value, "window_placement")
            .filter(|record| {
                let valid = record.valid();
                if !valid {
                    tracing::warn!("invalid or incompatible startup placement");
                }
                valid
            });
        let last_pdf = decode_section::<SavedPdf>(&value, "last_pdf").filter(|p| {
            let valid = p.decode().is_some();
            if !valid {
                tracing::warn!("invalid or incompatible startup PDF path");
            }
            valid
        });
        Ok(StartupState {
            window_placement,
            last_pdf,
        })
    }
    pub fn save(&self, state: &StartupState) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(&Envelope { version: 1, state })?;
        if bytes.len() as u64 > MAX_SETTINGS_BYTES {
            bail!("startup settings exceed the size limit");
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomic(&self.path, &bytes)
    }
}
fn decode_section<T: serde::de::DeserializeOwned>(
    value: &serde_json::Value,
    key: &str,
) -> Option<T> {
    let section = value.get(key)?;
    if section.is_null() {
        return None;
    }
    match serde_json::from_value(section.clone()) {
        Ok(record) => Some(record),
        Err(_) => {
            tracing::warn!(section = key, "invalid startup settings section");
            None
        }
    }
}

pub struct StartupPersistence {
    pub store: Option<StartupStore>,
    pub loaded: StartupState,
}
impl StartupPersistence {
    pub fn for_options(
        options: &StartupOptions,
        make_store: impl FnOnce() -> Option<StartupStore>,
    ) -> Option<Self> {
        if options.is_smoke() {
            return None;
        }
        let store = make_store();
        let loaded = store
            .as_ref()
            .map(|store| {
                store.load().unwrap_or_else(|error| {
                    tracing::warn!(error = %error, "could not load startup settings");
                    StartupState::default()
                })
            })
            .unwrap_or_default();
        Some(Self { store, loaded })
    }
    pub fn select_pdf(&self, explicit: Option<PathBuf>) -> Option<StartupPdf> {
        if let Some(path) = explicit {
            Some(StartupPdf {
                path,
                automatic: false,
            })
        } else {
            self.loaded
                .last_pdf
                .as_ref()?
                .decode()
                .map(|path| StartupPdf {
                    path,
                    automatic: true,
                })
        }
    }
    pub fn save_if_changed(&self, state: StartupState) -> Result<()> {
        if state != self.loaded {
            if let Some(store) = &self.store {
                store.save(&state)?;
            }
        }
        Ok(())
    }
}
pub struct StartupPdf {
    pub path: PathBuf,
    pub automatic: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{parse_startup_options, StartupRequest};
    fn options(args: &[&str]) -> StartupOptions {
        let StartupRequest::Run(options) = parse_startup_options(args.iter().copied()).unwrap()
        else {
            panic!()
        };
        options
    }
    fn temp_store() -> StartupStore {
        let dir = std::env::temp_dir().join(format!(
            "qp-startup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        StartupStore::new(dir.join("startup-state.json"))
    }
    #[test]
    fn unchanged_state_does_not_attempt_a_write() {
        let store = temp_store();
        fs::create_dir_all(&store.path).unwrap();
        let persistence = StartupPersistence {
            store: Some(store),
            loaded: StartupState::default(),
        };
        // A changed state would fail to replace the directory at this path.
        persistence
            .save_if_changed(StartupState::default())
            .unwrap();
        assert!(persistence.store.as_ref().unwrap().path.is_dir());
        fs::remove_dir_all(persistence.store.unwrap().path.parent().unwrap()).unwrap();
    }
    #[test]
    fn invalid_pdf_does_not_discard_placement_and_paths_are_validated() {
        let store = temp_store();
        let placement = PlacementRecord {
            platform: crate::window_placement::platform().into(),
            source_display: crate::window_placement::DisplayRect {
                origin: [0, 0],
                extent: [1920, 1080],
                scale: 1.0,
            },
            relative_center: [0.5; 2],
            logical_inner_size: [1024.0, 576.0],
        };
        store
            .save(&StartupState {
                window_placement: Some(placement.clone()),
                last_pdf: None,
            })
            .unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&store.path).unwrap()).unwrap();
        value["last_pdf"] = serde_json::json!({"platform":os_platform(),"path":{"encoding":"Utf8","value":"relative.pdf"}});
        fs::write(&store.path, serde_json::to_vec(&value).unwrap()).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.window_placement, Some(placement));
        assert!(loaded.last_pdf.is_none());
        let mut path = SavedPdf::capture(Path::new("deck.pdf")).unwrap();
        assert!(path.decode().unwrap().is_absolute());
        path.path = NativePath::Utf8("/deck\0.pdf".into());
        assert!(path.decode().is_none());
        path = SavedPdf::capture(Path::new("deck.pdf")).unwrap();
        path.platform = "other".into();
        assert!(path.decode().is_none());
        fs::remove_dir_all(store.path.parent().unwrap()).unwrap();
    }

    #[test]
    fn smoke_modes_never_construct_store() {
        for args in [
            vec!["--smoke-open-pdf", "test.pdf"],
            vec!["--gui-smoke", "test.pdf"],
            vec![
                "--gui-smoke",
                "test.pdf",
                "--gui-smoke-report",
                "report.txt",
                "--log-file",
                "log.txt",
            ],
            vec!["--smoke-open-pdf", "missing.pdf", "--log-file", "log.txt"],
        ] {
            assert!(StartupPersistence::for_options(&options(&args), || panic!(
                "smoke accessed settings"
            ))
            .is_none());
        }
        assert!(
            StartupPersistence::for_options(&options(&["--log-file", "log.txt"]), || None)
                .is_some()
        );
    }
    #[test]
    fn explicit_pdf_wins_and_null_clears_saved_pdf() {
        let store = temp_store();
        let saved = SavedPdf::capture(Path::new("deck.pdf")).unwrap();
        let state = StartupState {
            last_pdf: Some(saved.clone()),
            ..Default::default()
        };
        store.save(&state).unwrap();
        let persistence = StartupPersistence {
            loaded: store.load().unwrap(),
            store: Some(store),
        };
        assert!(persistence.select_pdf(None).unwrap().automatic);
        let chosen = persistence.select_pdf(Some("explicit.pdf".into())).unwrap();
        assert!(!chosen.automatic);
        assert_eq!(chosen.path, PathBuf::from("explicit.pdf"));
        persistence
            .save_if_changed(StartupState::default())
            .unwrap();
        assert!(persistence
            .store
            .as_ref()
            .unwrap()
            .load()
            .unwrap()
            .last_pdf
            .is_none());
        fs::remove_dir_all(persistence.store.unwrap().path.parent().unwrap()).unwrap();
    }
    #[test]
    fn independent_sections_and_unknown_schema() {
        let store = temp_store();
        store
            .save(&StartupState {
                last_pdf: Some(SavedPdf::capture(Path::new("deck.pdf")).unwrap()),
                ..Default::default()
            })
            .unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&store.path).unwrap()).unwrap();
        value["window_placement"] = serde_json::json!({"platform":"invalid"});
        fs::write(&store.path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(store.load().unwrap().last_pdf.is_some());
        value["version"] = 2.into();
        fs::write(&store.path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(store.load().is_err());
        fs::write(&store.path, vec![b' '; MAX_SETTINGS_BYTES as usize + 1]).unwrap();
        assert!(store.load().is_err());
        fs::remove_dir_all(store.path.parent().unwrap()).unwrap();
    }
    #[test]
    fn missing_corrupt_and_write_failure_are_recoverable() {
        let store = temp_store();
        assert_eq!(store.load().unwrap(), StartupState::default());
        fs::create_dir_all(store.path.parent().unwrap()).unwrap();
        fs::write(&store.path, b"bad JSON").unwrap();
        assert!(store.load().is_err());
        fs::remove_file(&store.path).unwrap();
        fs::create_dir(&store.path).unwrap();
        assert!(store.save(&StartupState::default()).is_err());
        fs::remove_dir_all(store.path.parent().unwrap()).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn native_non_utf8_path_round_trips_with_private_permissions() {
        use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};
        let path = PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', b'd', 0xff]));
        let saved = SavedPdf::capture(&path).unwrap();
        assert_eq!(saved.decode(), Some(path));
        let store = temp_store();
        store
            .save(&StartupState {
                last_pdf: Some(saved),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            fs::metadata(&store.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(store.path.parent().unwrap()).unwrap();
    }
}

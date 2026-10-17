//! User-owned TOML configuration; all file IO is performed by the initialization worker.
use serde::Deserialize;
use std::{fs::File, io::Read, path::PathBuf};

#[derive(Clone)]
pub struct RelayEndpoint {
    pub origin: url::Url,
    pub create_token: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    audience: Option<Audience>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Audience {
    transport: Option<String>,
    relay: Option<String>,
    create_token: Option<String>,
}

pub fn load(explicit: Option<PathBuf>) -> Result<Option<RelayEndpoint>, &'static str> {
    load_from(
        explicit,
        crate::settings_storage::platform_config_dir().map(|p| p.join("config.toml")),
    )
}

fn load_from(
    explicit: Option<PathBuf>,
    default: Option<PathBuf>,
) -> Result<Option<RelayEndpoint>, &'static str> {
    let required = explicit.is_some();
    let Some(path) = explicit.or(default) else {
        return Ok(None);
    };
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if !required && e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Cannot open Audience configuration."),
    };
    let metadata = file
        .metadata()
        .map_err(|_| "Cannot inspect Audience configuration.")?;
    if !metadata.is_file() {
        return Err("Audience configuration must be a regular file.");
    }
    let mut text = String::new();
    file.take(16385)
        .read_to_string(&mut text)
        .map_err(|_| "Cannot read Audience configuration.")?;
    if text.len() > 16384 {
        return Err("Audience configuration is too large.");
    }
    let mut table: toml::Table =
        toml::from_str(&text).map_err(|_| "Invalid Audience TOML configuration.")?;
    table.retain(|key, _| key == "audience");
    let endpoint =
        parse(&toml::to_string(&table).map_err(|_| "Invalid Audience TOML configuration.")?)?;
    #[cfg(unix)]
    if endpoint.is_some() {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err("Audience configuration must be owner-only (chmod 600).");
        }
    }
    Ok(endpoint)
}

fn parse(text: &str) -> Result<Option<RelayEndpoint>, &'static str> {
    let bad = "Invalid Audience TOML configuration.";
    let config: Config = toml::from_str(text).map_err(|_| bad)?;
    let a = config.audience.unwrap_or_default();
    match a.transport.as_deref().unwrap_or("local") {
        "local" if a.relay.is_none() && a.create_token.is_none() => Ok(None),
        "remote" => {
            let origin = url::Url::parse(a.relay.as_deref().ok_or(bad)?).map_err(|_| bad)?;
            let create_token = a.create_token.ok_or(bad)?;
            let loopback = origin.host_str().is_some_and(|h| {
                h.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            });
            if origin.host_str().is_none()
                || !origin.username().is_empty()
                || origin.password().is_some()
                || origin.path() != "/"
                || origin.query().is_some()
                || origin.fragment().is_some()
                || !(origin.scheme() == "https" || origin.scheme() == "http" && loopback)
                || create_token.len() != 43
                || !create_token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(bad);
            }
            Ok(Some(RelayEndpoint {
                origin,
                create_token,
            }))
        }
        _ => Err(bad),
    }
}

/// Preview an imported download without requiring its original permissions.
/// The destination is protected separately before credential bytes are written.
pub fn preview_import(path: &std::path::Path) -> Result<RelayEndpoint, &'static str> {
    let file = File::open(path).map_err(|_| "Cannot open configuration import.")?;
    if !file
        .metadata()
        .map_err(|_| "Cannot inspect configuration import.")?
        .is_file()
    {
        return Err("Configuration import must be a regular file.");
    }
    let mut text = String::new();
    file.take(16385)
        .read_to_string(&mut text)
        .map_err(|_| "Cannot read configuration import.")?;
    if text.len() > 16384 {
        return Err("Configuration import is too large.");
    }
    parse(&text)?.ok_or("Import requires a remote Audience configuration.")
}

pub fn import_destination(explicit: Option<PathBuf>) -> Result<PathBuf, &'static str> {
    explicit
        .or_else(|| crate::settings_storage::platform_config_dir().map(|p| p.join("config.toml")))
        .ok_or("Cannot locate configuration directory.")
}

/// Merge only Audience fields; an invalid existing file is never overwritten.
pub fn save_import(path: &std::path::Path, endpoint: &RelayEndpoint) -> Result<(), &'static str> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Configuration destination must not be a symbolic link.");
    }
    let mut current: toml::Table = match std::fs::read_to_string(path) {
        Ok(text) if text.len() <= 16384 => toml::from_str(&text)
            .map_err(|_| "Existing configuration is invalid; it was not changed.")?,
        Ok(_) => return Err("Existing configuration is too large; it was not changed."),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
        Err(_) => return Err("Cannot read existing configuration; it was not changed."),
    };
    let mut audience = toml::Table::new();
    audience.insert("transport".into(), "remote".into());
    audience.insert("relay".into(), endpoint.origin.as_str().into());
    audience.insert("create_token".into(), endpoint.create_token.clone().into());
    current.insert("audience".into(), audience.into());
    let text = toml::to_string(&current).map_err(|_| "Cannot serialize configuration.")?;
    if text.len() > 16384 {
        return Err("Merged configuration is too large.");
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "Cannot create configuration directory.")?;
    }
    crate::settings_storage::write_atomic(path, text.as_bytes())
        .map_err(|_| "Cannot save protected configuration; previous settings are retained.")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn remote(origin: &str) -> String {
        format!(
            "[audience]\ntransport='remote'\nrelay='{origin}'\ncreate_token='{}'\n",
            "A".repeat(43)
        )
    }

    #[test]
    fn import_merges_unrelated_settings_and_reloads_without_restart() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("download.toml");
        let destination = dir.path().join("config.toml");
        std::fs::write(&source, remote("https://relay.example")).unwrap();
        std::fs::write(&destination, "[presentation]\nkeep = 'unchanged'\n").unwrap();
        let endpoint = preview_import(&source).unwrap();
        save_import(&destination, &endpoint).unwrap();
        let saved = std::fs::read_to_string(&destination).unwrap();
        assert!(saved.contains("unchanged"));
        assert_eq!(
            load(Some(destination.clone())).unwrap().unwrap().origin,
            endpoint.origin
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&destination)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        std::fs::write(&source, "invalid?").unwrap();
        assert!(preview_import(&source).is_err());
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), saved);
        std::fs::write(&destination, "invalid?").unwrap();
        assert!(save_import(&destination, &endpoint).is_err());
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "invalid?");
    }

    #[test]
    fn import_rejects_oversized_local_and_non_file_sources() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("download.toml");
        assert!(preview_import(dir.path()).is_err());
        std::fs::write(&source, " ".repeat(16385)).unwrap();
        assert!(preview_import(&source).is_err());
        std::fs::write(&source, "[audience]\ntransport='local'").unwrap();
        assert!(preview_import(&source).is_err());
        assert_eq!(import_destination(Some(source.clone())).unwrap(), source);
    }

    #[test]
    fn configuration_size_limit_counts_utf8_bytes_and_accepts_exact_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        // A TOML comment supplies valid multibyte content without changing the mode.
        let exact = format!("#{} ", "é".repeat(8191));
        assert_eq!(exact.len(), 16384);
        std::fs::write(&path, &exact).unwrap();
        assert!(load_from(Some(path.clone()), None).unwrap().is_none());
        std::fs::write(&path, format!("{exact} ")).unwrap();
        assert_eq!(
            load_from(Some(path), None).err(),
            Some("Audience configuration is too large.")
        );
    }

    #[test]
    fn invalid_utf8_and_configuration_errors_do_not_expose_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private-config.toml");
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert_eq!(
            load_from(Some(path.clone()), None).err(),
            Some("Cannot read Audience configuration.")
        );
        std::fs::write(&path, "[audience]\ncreate_token='private-test-value'").unwrap();
        assert_eq!(
            load_from(Some(path), None).err(),
            Some("Invalid Audience TOML configuration.")
        );
    }

    #[test]
    fn relay_creation_token_uses_exact_base64url_length_and_alphabet() {
        let base = remote("https://relay.example");
        let valid = format!("{}_-9", "a".repeat(40));
        assert_eq!(valid.len(), 43);
        let endpoint = parse(&base.replace(&"A".repeat(43), &valid))
            .unwrap()
            .unwrap();
        assert_eq!(endpoint.create_token, valid);
        assert_eq!(endpoint.origin.as_str(), "https://relay.example/");
        for token in [
            "a".repeat(42),
            "a".repeat(44),
            format!("{}+", "a".repeat(42)),
            format!("{}/", "a".repeat(42)),
            format!("{}=", "a".repeat(42)),
            format!("{}é", "a".repeat(41)),
        ] {
            assert!(
                parse(&base.replace(&"A".repeat(43), &token)).is_err(),
                "{token}"
            );
        }
    }

    #[test]
    fn strict_modes_origins_and_credentials() {
        assert!(parse("").unwrap().is_none());
        assert!(parse("[audience]\ntransport='local'").unwrap().is_none());
        for origin in [
            "https://relay.example",
            "http://127.0.0.1:1234",
            "http://[::1]:1234",
        ] {
            assert!(parse(&remote(origin)).unwrap().is_some());
        }
        for origin in [
            "http://relay.example",
            "http://localhost:1234",
            "https://user@relay.example",
            "https://relay.example/base",
            "https://relay.example/?k=a",
            "https://relay.example/#a",
            "ftp://relay.example",
        ] {
            assert!(parse(&remote(origin)).is_err());
        }
        for text in [
            "unknown=1".into(),
            "[audience]\ntransport='bad'".into(),
            "[audience]\nrelay='https://relay.example'".into(),
            "[audience]\ntransport='remote'".into(),
            remote("https://relay.example").replace(&"A".repeat(43), "secret"),
            format!("{}transport='remote'", remote("https://relay.example")),
            format!("{}unknown=1", remote("https://relay.example")),
        ] {
            assert!(parse(&text).is_err());
        }
    }
    #[test]
    fn default_absence_is_local_but_explicit_file_replaces_without_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let default = dir.path().join("config.toml");
        let explicit = dir.path().join("alternate.toml");
        assert!(load_from(None, None).unwrap().is_none());
        assert!(load_from(None, Some(default.clone())).unwrap().is_none());
        std::fs::write(&default, "[audience]\ntransport='local'").unwrap();
        assert!(load_from(Some(explicit.clone()), Some(default.clone())).is_err());
        std::fs::write(&explicit, "invalid?").unwrap();
        assert!(load_from(Some(explicit.clone()), Some(default.clone())).is_err());
        std::fs::write(&default, "invalid?").unwrap();
        std::fs::write(&explicit, "[audience]\ntransport='local'").unwrap();
        assert!(load_from(Some(explicit), Some(default.clone()))
            .unwrap()
            .is_none());
        assert!(load_from(None, Some(default)).is_err());
    }

    #[test]
    fn explicit_file_errors_and_bounds_are_redacted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.toml");
        assert_eq!(
            load(Some(path.clone())).err(),
            Some("Cannot open Audience configuration.")
        );
        std::fs::write(&path, "?").unwrap();
        assert!(load(Some(path.clone())).is_err());
        std::fs::write(&path, " ".repeat(16385)).unwrap();
        assert_eq!(
            load(Some(path.clone())).err(),
            Some("Audience configuration is too large.")
        );
        std::fs::write(&path, "[audience]\ntransport='local'").unwrap();
        assert!(load(Some(path.clone())).unwrap().is_none());
        std::fs::write(&path, remote("https://relay.example")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(load(Some(path.clone())).is_err());
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(load(Some(path)).unwrap().is_some());
        assert!(load(Some(dir.path().into())).is_err());
    }
}
